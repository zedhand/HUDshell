//! R2: ConPTY host (system ConPTY, flags = 0). Launches real pwsh (or Windows PowerShell as a
//! last resort) inside a pseudoconsole, with a kill-on-close Job Object and separate
//! reader / writer / exit-watcher threads. Output is buffered with a hard cap so a flood
//! cannot grow memory without bound (the reader blocks, which naturally backpressures ConPTY).

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, ResumeThread,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTUPINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::log;

/// dwFlags passed to CreatePseudoConsole (0 by default; experiments: 0x08/0x10/0x18/0x20 glyph width).
#[cfg(feature = "harness")]
pub static PTY_FLAGS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Test mode: log the exact bytes the terminal sends to the shell.
#[cfg(feature = "harness")]
pub static LOG_INPUT: AtomicBool = AtomicBool::new(false);

pub const WM_APP_PTY: u32 = 0x8000 + 1;
pub const WM_APP_PTY_EXIT: u32 = 0x8000 + 2;

/// Hard cap on bytes buffered between the reader thread and the UI thread.
const MAX_QUEUED_BYTES: usize = 1 << 20;
/// Input waiting for a shell that does not read it is dropped beyond this (it would otherwise pile up without bound).
const MAX_PENDING_INPUT: usize = 8 << 20;

/// Id of a shell session (the page names its views with the same number).
pub type SessionId = u32;

struct Queue {
    chunks: VecDeque<Vec<u8>>,
    bytes: usize,
}

/// State shared between a session's threads and the UI thread.
struct Shared {
    queue: Mutex<Queue>,
    cv: Condvar,
    /// The pseudoconsole has been (or is being) closed; closing twice would be a bug. Only changed while holding `pc`.
    closed: AtomicBool,
    /// The session is being released: the reader stops waiting for room in the queue and ends.
    closing: AtomicBool,
    /// The shell process has exited (set by the exit watcher; the session record stays until replaced).
    exited: AtomicBool,
    /// Held around every call on the pseudoconsole handle, so a resize never runs during or after its close.
    pc: Mutex<()>,
    /// Bytes sent to the writer thread and not written yet.
    pending_in: AtomicUsize,
    /// When the reader last got output (ms since the session started), for typing the profile's start command.
    last_output_ms: std::sync::atomic::AtomicU64,
}

impl Shared {
    /// Close the pseudoconsole once, under the handle lock. Call it off the UI thread: it can wait for the console host.
    fn close_pc(&self, hpc: isize) {
        let _g = self.pc.lock().unwrap_or_else(|p| p.into_inner());
        if !self.closed.swap(true, Ordering::SeqCst) {
            unsafe { ClosePseudoConsole(HPCON(hpc)) };
        }
    }

    /// Tell the reader to stop (it may be waiting for room in a full queue that nobody drains any more).
    fn stop_reader(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let _q = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        self.cv.notify_all();
    }
}

struct Session {
    id: SessionId,
    /// Process id of the shell itself (used to find the program running below it).
    pid: u32,
    hpc: isize,
    job: isize,
    tx: mpsc::Sender<Vec<u8>>,
    /// Sizes for the resizer thread (resizing waits for the console host, so it never runs on the UI thread).
    resize_tx: mpsc::Sender<(i16, i16)>,
    cols: i16,
    rows: i16,
    shared: Arc<Shared>,
}

static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());
/// One wake message at a time is enough: the UI thread drains every session each time.
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);
/// The session harness scripts and `write` talk to (the one on screen).
static ACTIVE: AtomicU32 = AtomicU32::new(1);
/// Round-robin start so one busy session cannot starve the others.
static NEXT_FROM: AtomicUsize = AtomicUsize::new(0);

pub fn set_active(id: SessionId) {
    ACTIVE.store(id, Ordering::SeqCst);
}

#[cfg(feature = "harness")]
pub fn active() -> SessionId {
    ACTIVE.load(Ordering::SeqCst)
}

/// Census of control sequences the child programs emit (what Claude/Codex/pwsh really use).
#[cfg(feature = "harness")]
const PATTERNS: &[(&str, &[u8])] = &[
    ("sync-begin ?2026h", b"\x1b[?2026h"),
    ("sync-end ?2026l", b"\x1b[?2026l"),
    ("alt-screen-on ?1049h", b"\x1b[?1049h"),
    ("alt-screen-off ?1049l", b"\x1b[?1049l"),
    ("ED2 clear-screen", b"\x1b[2J"),
    ("ED3 clear-scrollback", b"\x1b[3J"),
    ("win32-input ?9001h", b"\x1b[?9001h"),
    ("bracketed-paste ?2004h", b"\x1b[?2004h"),
    ("focus-events ?1004h", b"\x1b[?1004h"),
    ("mouse ?1000h", b"\x1b[?1000h"),
    ("mouse-sgr ?1006h", b"\x1b[?1006h"),
    ("query DA1 ESC[c", b"\x1b[c"),
    ("query XTVERSION ESC[>q", b"\x1b[>q"),
    ("query XTVERSION ESC[>0q", b"\x1b[>0q"),
    ("query CPR ESC[6n", b"\x1b[6n"),
    ("query DECRQM ?2026$p", b"\x1b[?2026$p"),
    ("query kitty ESC[?u", b"\x1b[?u"),
    ("kitty pop ESC[<u", b"\x1b[<u"),
    ("kitty pop ESC[<1u", b"\x1b[<1u"),
    ("OSC52 clipboard", b"\x1b]52;"),
    ("OSC8 hyperlink", b"\x1b]8;"),
    ("OSC9 notification (or ConEmu OSC 9;n)", b"\x1b]9;"),
    ("OSC9;4 progress", b"\x1b]9;4;"),
    ("OSC777 notification", b"\x1b]777;"),
    ("OSC0 title", b"\x1b]0;"),
    ("OSC2 title", b"\x1b]2;"),
];
#[cfg(feature = "harness")]
static COUNTS: [std::sync::atomic::AtomicU64; 32] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 32];
#[cfg(feature = "harness")]
static KITTY_PUSH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "harness")]
fn census_scan(carry: &mut Vec<u8>, chunk: &[u8]) {
    let mut buf = std::mem::take(carry);
    let old = buf.len();
    buf.extend_from_slice(chunk);
    for (i, (_, pat)) in PATTERNS.iter().enumerate() {
        let mut from = 0;
        while let Some(pos) = buf[from..].windows(pat.len()).position(|w| w == *pat) {
            let start = from + pos;
            if start + pat.len() > old {
                COUNTS[i].fetch_add(1, Ordering::Relaxed);
            }
            from = start + 1;
        }
    }
    // kitty keyboard push: ESC [ > <digits> u
    let mut from = 0;
    while let Some(pos) = buf[from..].windows(3).position(|w| w == b"\x1b[>") {
        let start = from + pos;
        let mut j = start + 3;
        while j < buf.len() && buf[j].is_ascii_digit() {
            j += 1;
        }
        if j < buf.len() && buf[j] == b'u' && j > start + 3 && j + 1 > old {
            KITTY_PUSH.fetch_add(1, Ordering::Relaxed);
        }
        from = start + 1;
    }
    let keep = buf.len().min(15);
    *carry = buf[buf.len() - keep..].to_vec();
}

#[cfg(feature = "harness")]
pub fn kitty_pushes() -> u64 {
    KITTY_PUSH.load(Ordering::Relaxed)
}

#[cfg(feature = "harness")]
pub fn census_report() -> String {
    let mut s = String::from("terminal control-sequence census (child output):\n");
    for (i, (name, _)) in PATTERNS.iter().enumerate() {
        let n = COUNTS[i].load(Ordering::Relaxed);
        if n > 0 {
            s.push_str(&format!("   {name} = {n}\n"));
        }
    }
    let k = KITTY_PUSH.load(Ordering::Relaxed);
    if k > 0 {
        s.push_str(&format!("   kitty keyboard push ESC[>Nu = {k}\n"));
    }
    s
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The window that receives "output is waiting" and "shell ended" messages. With several windows it is one that is still open
/// (the first); each message handler then looks at every window.
static WAKE_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub fn set_wake_target(hwnd: isize) {
    WAKE_HWND.store(hwnd, Ordering::SeqCst);
}

fn wake_target(spawned_for: isize) -> isize {
    match WAKE_HWND.load(Ordering::SeqCst) {
        0 => spawned_for,
        h => h,
    }
}

fn wake(hwnd: isize) {
    if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
        let posted = unsafe {
            PostMessageW(Some(HWND(wake_target(hwnd) as *mut c_void)), WM_APP_PTY, WPARAM(0), LPARAM(0))
        };
        if posted.is_err() {
            // the window is gone (or its queue is full): no handler will clear the flag, so the next chunk must post again
            WAKE_PENDING.store(false, Ordering::SeqCst);
        }
    }
}

pub fn clear_wake() {
    WAKE_PENDING.store(false, Ordering::SeqCst);
}

/// Pop one buffered output chunk from one of the sessions `ids` (UI thread), rotating between them.
pub fn take_chunk_for(ids: &[SessionId]) -> Option<(SessionId, Vec<u8>)> {
    let list: Vec<(SessionId, Arc<Shared>)> =
        SESSIONS.lock().ok()?.iter().filter(|s| ids.contains(&s.id)).map(|s| (s.id, s.shared.clone())).collect();
    if list.is_empty() {
        return None;
    }
    let start = NEXT_FROM.fetch_add(1, Ordering::Relaxed);
    for k in 0..list.len() {
        let (id, sh) = &list[(start + k) % list.len()];
        let Ok(mut q) = sh.queue.lock() else { continue };
        if let Some(c) = q.chunks.pop_front() {
            q.bytes -= c.len();
            sh.cv.notify_all();
            return Some((*id, c));
        }
    }
    None
}

/// A shell is running (started and not yet exited) for this session.
pub fn has_session(id: SessionId) -> bool {
    SESSIONS
        .lock()
        .map(|s| s.iter().any(|x| x.id == id && !x.shared.exited.load(Ordering::SeqCst)))
        .unwrap_or(false)
}

/// Ids of all sessions that exist (running or finished), in creation order.
pub fn session_ids() -> Vec<SessionId> {
    SESSIONS.lock().map(|s| s.iter().map(|x| x.id).collect()).unwrap_or_default()
}

static SHELL_OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

/// Optional full path of the shell from the config file; tried before the automatic search.
pub fn set_shell(s: Option<String>) {
    if let Ok(mut o) = SHELL_OVERRIDE.lock() {
        *o = s.filter(|p| !p.trim().is_empty());
    }
}

/// Candidate shell executables in preference order: configured shell, PowerShell 7, Windows PowerShell last.
/// A configured shell path HUDshell will start: absolute (so no folder search can pick another file) and without a quote
/// (it is quoted on the command line).
fn usable_shell_path(p: &str) -> bool {
    let p = p.trim();
    !p.is_empty() && !p.contains('"') && std::path::Path::new(p).is_absolute()
}

fn shell_candidates(first: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for p in [first.map(str::to_string), SHELL_OVERRIDE.lock().ok().and_then(|o| o.clone())].into_iter().flatten() {
        if usable_shell_path(&p) {
            v.push(p.trim().to_string());
        } else if !p.trim().is_empty() {
            log("pty: a configured shell was skipped: it must be a full path without quotes");
        }
    }
    // The machine-wide install first: a pwsh.exe dropped in a folder on PATH must not win over it.
    if let Ok(d) = std::env::var("ProgramFiles") {
        v.push(format!("{d}\\PowerShell\\7\\pwsh.exe"));
    }
    if let Ok(path) = std::env::var("PATH") {
        // only absolute entries: a relative one would be looked up in whatever folder HUDshell was started from
        for dir in path.split(';').map(str::trim).filter(|d| !d.is_empty() && std::path::Path::new(d).is_absolute()) {
            let p = format!("{}\\pwsh.exe", dir.trim_end_matches('\\'));
            // symlink_metadata does not follow app-execution-alias reparse points.
            if std::fs::symlink_metadata(&p).is_ok() {
                v.push(p);
            }
        }
    }
    v.push(crate::config::local_app_data().join("Microsoft\\WindowsApps\\pwsh.exe").display().to_string());
    if let Ok(d) = std::env::var("USERPROFILE") {
        v.push(format!("{d}\\.dotnet\\tools\\pwsh.exe"));
    }
    if let Ok(d) = std::env::var("SystemRoot") {
        v.push(format!("{d}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"));
    }
    let mut seen = std::collections::HashSet::new();
    v.retain(|p| seen.insert(p.to_lowercase()));
    v
}

/// How to start one shell: where, which executable, and a line to type once it is up.
#[derive(Clone, Default, Debug)]
pub struct SpawnOpts {
    pub dir: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
}

/// Start a shell for session `id` (replacing a finished one with the same id).
pub fn spawn(hwnd: isize, id: SessionId, cols: i64, rows: i64, opts: &SpawnOpts) -> Result<String, String> {
    let cols = cols.clamp(1, 500) as i16;
    let rows = rows.clamp(1, 300) as i16;
    // A finished session with this id is replaced; its handles are released first.
    close(id);
    unsafe { spawn_inner(hwnd, id, cols, rows, opts) }
}

/// Handles a failed start must not leak: closed on drop unless taken over by the session's threads.
struct StartGuard {
    handles: Vec<HANDLE>,
    hpc: Option<isize>,
}

impl StartGuard {
    fn keep(&mut self, h: HANDLE) {
        self.handles.retain(|x| x.0 != h.0);
    }
}

impl Drop for StartGuard {
    fn drop(&mut self) {
        for h in self.handles.drain(..) {
            if !h.is_invalid() && !h.0.is_null() {
                let _ = unsafe { CloseHandle(h) };
            }
        }
        if let Some(hpc) = self.hpc.take() {
            // ClosePseudoConsole can wait for the console host: never on the UI thread
            std::thread::spawn(move || unsafe { ClosePseudoConsole(HPCON(hpc)) });
        }
    }
}

/// Whether `d` is a folder, without letting an unreachable network path freeze the caller for the SMB timeout.
fn dir_exists_quick(d: &str) -> bool {
    if !d.starts_with("\\\\") {
        return std::path::Path::new(d).is_dir();
    }
    let (tx, rx) = mpsc::channel();
    let owned = d.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(std::path::Path::new(&owned).is_dir());
    });
    rx.recv_timeout(Duration::from_millis(1500)).unwrap_or(false)
}

/// The environment block for the shell (UTF-16, `NAME=value\0...\0`): HUDshell's own environment plus an honest
/// terminal identity and truecolor. Built per shell, so HUDshell never changes its own environment while threads run.
fn child_environment() -> Vec<u16> {
    let mut vars: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| !k.eq_ignore_ascii_case("COLORTERM") && !k.eq_ignore_ascii_case("TERM_PROGRAM"))
        .collect();
    // Do NOT impersonate Windows Terminal.
    vars.push(("COLORTERM".into(), "truecolor".into()));
    vars.push(("TERM_PROGRAM".into(), "HUDshell".into()));
    vars.sort_by_key(|(k, _)| k.to_uppercase());
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in vars {
        block.extend(format!("{k}={v}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

unsafe fn spawn_inner(hwnd: isize, id: SessionId, cols: i16, rows: i16, opts: &SpawnOpts) -> Result<String, String> {
    let e = |what: &str, err: windows::core::Error| format!("{what}: {err}");
    let (mut in_r, mut in_w, mut out_r, mut out_w) =
        (HANDLE::default(), HANDLE::default(), HANDLE::default(), HANDLE::default());
    let mut guard = StartGuard { handles: Vec::new(), hpc: None };
    unsafe {
        CreatePipe(&mut in_r, &mut in_w, None, 0).map_err(|x| e("CreatePipe(in)", x))?;
        guard.handles.extend([in_r, in_w]);
        CreatePipe(&mut out_r, &mut out_w, None, 0).map_err(|x| e("CreatePipe(out)", x))?;
        guard.handles.extend([out_r, out_w]);
        #[cfg(feature = "harness")]
        let flags = PTY_FLAGS.load(Ordering::SeqCst);
        #[cfg(not(feature = "harness"))]
        let flags = 0u32;
        log(&format!("pty: CreatePseudoConsole flags=0x{flags:X}"));
        let hpc = CreatePseudoConsole(COORD { X: cols, Y: rows }, in_r, out_w, flags)
            .map_err(|x| e("CreatePseudoConsole", x))?;
        guard.hpc = Some(hpc.0);
        // The pseudoconsole owns its ends now; drop our copies so EOF can be detected.
        guard.keep(in_r);
        guard.keep(out_w);
        let _ = CloseHandle(in_r);
        let _ = CloseHandle(out_w);

        // Process attribute list carrying the HPCON (u64 storage: the list needs pointer alignment).
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut attr_buf = vec![0u64; size.div_ceil(8)];
        let attr = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut c_void);
        InitializeProcThreadAttributeList(Some(attr), 1, None, &mut size)
            .map_err(|x| e("InitializeProcThreadAttributeList", x))?;
        let upd = UpdateProcThreadAttribute(
            attr,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(hpc.0 as *const c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        );
        if let Err(x) = upd {
            DeleteProcThreadAttributeList(attr);
            return Err(e("UpdateProcThreadAttribute", x));
        }

        let mut si = STARTUPINFOEXW::default();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.lpAttributeList = attr;
        let env = child_environment();

        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
        let start_dir = match opts.dir.as_deref() {
            Some(d) if dir_exists_quick(d) => d.to_string(),
            Some(d) => {
                log(&format!("pty: start directory of session {id} does not exist; using the home folder (dir length {})", d.len()));
                home
            }
            None => home,
        };
        let cwd = wide(&start_dir);

        // Kill-on-close job, made before the shell so the shell can join it before it runs any code.
        let job = match CreateJobObjectW(None, PCWSTR::null()) {
            Ok(j) => {
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let r = SetInformationJobObject(
                    j,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if r.is_err() {
                    log("pty: the job object could not be set to kill on close; the shell runs without it");
                    let _ = CloseHandle(j);
                    None
                } else {
                    Some(j)
                }
            }
            Err(x) => {
                log(&format!("pty: no job object ({x}); the shell runs without one"));
                None
            }
        };

        let mut launched: Option<(String, PROCESS_INFORMATION)> = None;
        let mut last_err = String::new();
        for path in shell_candidates(opts.shell.as_deref()) {
            let mut cmd = wide(&format!("\"{path}\" -NoLogo"));
            let mut pi = PROCESS_INFORMATION::default();
            match CreateProcessW(
                None,
                Some(PWSTR(cmd.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                Some(env.as_ptr() as *const c_void),
                PCWSTR(cwd.as_ptr()),
                &si.StartupInfo,
                &mut pi,
            ) {
                Ok(()) => {
                    launched = Some((path, pi));
                    break;
                }
                Err(err) => {
                    last_err = format!("{path}: {err}");
                    log(&format!("pty: candidate failed: {last_err}"));
                }
            }
        }
        DeleteProcThreadAttributeList(attr);
        drop(attr_buf);
        let Some((path, pi)) = launched else {
            if let Some(j) = job {
                let _ = CloseHandle(j);
            }
            return Err(format!("no shell could be started (last: {last_err})"));
        };
        let assigned = job.map(|j| AssignProcessToJobObject(j, pi.hProcess).is_ok()).unwrap_or(false);
        log(&format!("pty: job object configured={} assigned={assigned}", job.is_some()));
        // Started suspended: it runs only now, inside its job.
        ResumeThread(pi.hThread);
        let _ = CloseHandle(pi.hThread);

        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue { chunks: VecDeque::new(), bytes: 0 }),
            cv: Condvar::new(),
            closed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            pc: Mutex::new(()),
            pending_in: AtomicUsize::new(0),
            last_output_ms: std::sync::atomic::AtomicU64::new(0),
        });
        let started = std::time::Instant::now();

        // From here the threads own the handles.
        guard.keep(out_r);
        guard.keep(in_w);
        guard.hpc = None;
        drop(guard);

        // Reader: blocking ReadFile, bounded queue, wake the UI thread. Owns `out_r`.
        let out_i = out_r.0 as isize;
        let rd_shared = shared.clone();
        std::thread::spawn(move || {
            let h = HANDLE(out_i as *mut c_void);
            let mut buf = vec![0u8; 32 * 1024];
            let mut total: u64 = 0;
            #[cfg(feature = "harness")]
            let mut carry: Vec<u8> = Vec::new();
            'read: loop {
                let mut n = 0u32;
                if unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_err() || n == 0 {
                    break;
                }
                total += n as u64;
                rd_shared.last_output_ms.store(started.elapsed().as_millis() as u64, Ordering::SeqCst);
                #[cfg(feature = "harness")]
                census_scan(&mut carry, &buf[..n as usize]);
                let chunk = buf[..n as usize].to_vec();
                {
                    let mut q = rd_shared.queue.lock().unwrap_or_else(|p| p.into_inner());
                    while q.bytes > MAX_QUEUED_BYTES && !rd_shared.closing.load(Ordering::SeqCst) {
                        q = rd_shared.cv.wait(q).unwrap_or_else(|p| p.into_inner());
                    }
                    if rd_shared.closing.load(Ordering::SeqCst) {
                        break 'read; // released: nobody will drain the queue any more
                    }
                    q.bytes += chunk.len();
                    q.chunks.push_back(chunk);
                }
                wake(hwnd);
            }
            // Closing our end also unblocks a console host still writing to the pipe.
            let _ = unsafe { CloseHandle(h) };
            log(&format!("pty reader {id}: EOF after {total} bytes"));
        });

        // Writer: serializes input so ordering (including query replies) is preserved. Owns `in_w`.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let in_i = in_w.0 as isize;
        let wr_shared = shared.clone();
        std::thread::spawn(move || {
            let h = HANDLE(in_i as *mut c_void);
            'write: for data in rx {
                let mut off = 0usize;
                while off < data.len() {
                    let mut w = 0u32;
                    if unsafe { WriteFile(h, Some(&data[off..]), Some(&mut w), None) }.is_err() {
                        log("pty writer: WriteFile failed");
                        break 'write;
                    }
                    off += w as usize;
                }
                wr_shared.pending_in.fetch_sub(data.len(), Ordering::SeqCst);
            }
            let _ = unsafe { CloseHandle(h) };
        });

        // Resizer: applies the newest size (older ones still queued are skipped), under the handle lock.
        let (resize_tx, resize_rx) = mpsc::channel::<(i16, i16)>();
        let rs_shared = shared.clone();
        let hpc_r = hpc.0;
        std::thread::spawn(move || {
            while let Ok(mut size) = resize_rx.recv() {
                while let Ok(newer) = resize_rx.try_recv() {
                    size = newer;
                }
                let _g = rs_shared.pc.lock().unwrap_or_else(|p| p.into_inner());
                if rs_shared.closed.load(Ordering::SeqCst) {
                    continue;
                }
                let r = unsafe { ResizePseudoConsole(HPCON(hpc_r), COORD { X: size.0, Y: size.1 }) };
                log(&format!("pty: resize session {id} -> {}x{} ok={}", size.0, size.1, r.is_ok()));
            }
        });

        // Exit watcher: tell the UI, let final output drain, then close the pseudoconsole. Owns `hProcess`.
        let proc_i = pi.hProcess.0 as isize;
        let hpc_i = hpc.0;
        let ex_shared = shared.clone();
        std::thread::spawn(move || {
            let hp = HANDLE(proc_i as *mut c_void);
            unsafe { WaitForSingleObject(hp, INFINITE) };
            let _ = unsafe { CloseHandle(hp) };
            log(&format!("pty: shell process of session {id} exited"));
            ex_shared.exited.store(true, Ordering::SeqCst);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(wake_target(hwnd) as *mut c_void)),
                    WM_APP_PTY_EXIT,
                    WPARAM(id as usize),
                    LPARAM(0),
                );
            }
            std::thread::sleep(Duration::from_millis(300));
            ex_shared.close_pc(hpc_i);
        });

        // A profile's command is typed once the shell has started and gone quiet (its prompt is drawn), at the earliest
        // after 0.9 s and at the latest after 6 s.
        if let Some(cmd) = opts.command.clone().filter(|c| !c.trim().is_empty()) {
            let tx2 = tx.clone();
            let cmd_shared = shared.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(900));
                loop {
                    let now = started.elapsed().as_millis() as u64;
                    let last = cmd_shared.last_output_ms.load(Ordering::SeqCst);
                    if cmd_shared.closing.load(Ordering::SeqCst) || cmd_shared.exited.load(Ordering::SeqCst) {
                        return;
                    }
                    if (last > 0 && now.saturating_sub(last) >= 400) || now >= 6000 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let bytes = format!("{cmd}\r").into_bytes();
                cmd_shared.pending_in.fetch_add(bytes.len(), Ordering::SeqCst);
                if tx2.send(bytes).is_err() {
                    cmd_shared.pending_in.store(0, Ordering::SeqCst);
                }
            });
        }
        if let Ok(mut s) = SESSIONS.lock() {
            s.push(Session {
                id,
                pid: pi.dwProcessId,
                hpc: hpc.0,
                job: job.map(|j| j.0 as isize).unwrap_or(0),
                tx,
                resize_tx,
                cols,
                rows,
                shared,
            });
        }
        Ok(path)
    }
}

/// Write to the session on screen (harness scripts and callers that do not name a session).
#[cfg(feature = "harness")]
pub fn write(bytes: Vec<u8>) {
    write_to(active(), bytes);
}

/// (session id, shell process id) of every shell that is still running.
pub fn shell_pids() -> Vec<(SessionId, u32)> {
    SESSIONS
        .lock()
        .map(|s| s.iter().filter(|x| !x.shared.exited.load(Ordering::SeqCst)).map(|x| (x.id, x.pid)).collect())
        .unwrap_or_default()
}

pub fn write_to(id: SessionId, bytes: Vec<u8>) {
    #[cfg(feature = "harness")]
    if LOG_INPUT.load(Ordering::Relaxed) {
        let hex: Vec<String> = bytes.iter().take(64).map(|b| format!("{b:02x}")).collect();
        log(&format!("PTY-IN {} byte(s): {}", bytes.len(), hex.join(" ")));
    }
    if let Ok(s) = SESSIONS.lock()
        && let Some(s) = s.iter().find(|s| s.id == id) {
            let n = bytes.len();
            if s.shared.pending_in.load(Ordering::SeqCst) + n > MAX_PENDING_INPUT {
                log(&format!("pty: session {id} is not reading its input; {n} byte(s) dropped"));
                return;
            }
            s.shared.pending_in.fetch_add(n, Ordering::SeqCst);
            if s.tx.send(bytes).is_err() {
                s.shared.pending_in.fetch_sub(n, Ordering::SeqCst);
            }
        }
}

/// Make the shell of session `id` repaint its screen: the pseudoconsole redraws when its size changes, so it is made one column narrower and
/// then given `cols` x `rows` (used when a shell moves into a new window whose page has no history of it).
pub fn redraw(id: SessionId, cols: i64, rows: i64) {
    resize(id, (cols - 1).max(2), rows);
    // the second size follows from a worker thread, so the UI thread does not sleep
    let narrow = ((cols - 1).max(2).clamp(1, 500) as i16, rows.clamp(1, 300) as i16);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        // only if nothing resized it meanwhile (a newer size from the page wins)
        let unchanged = SESSIONS.lock().map(|l| l.iter().any(|s| s.id == id && (s.cols, s.rows) == narrow)).unwrap_or(false);
        if unchanged {
            resize(id, cols, rows);
        }
    });
}

pub fn resize(id: SessionId, cols: i64, rows: i64) {
    let cols = cols.clamp(1, 500) as i16;
    let rows = rows.clamp(1, 300) as i16;
    if let Ok(mut list) = SESSIONS.lock()
        && let Some(s) = list.iter_mut().find(|s| s.id == id) {
            if s.cols == cols && s.rows == rows {
                return; // drop no-op resizes
            }
            // The resizer thread does the call (it can wait for the console host) and skips a closed pseudoconsole.
            let _ = s.resize_tx.send((cols, rows));
            s.cols = cols;
            s.rows = rows;
        }
}

/// Release one session: kill its process tree (job close), close its pseudoconsole on a worker thread
/// and forget it. Safe for a session that already exited or does not exist.
pub fn close(id: SessionId) {
    let taken = SESSIONS.lock().ok().and_then(|mut l| {
        let i = l.iter().position(|s| s.id == id)?;
        Some(l.remove(i))
    });
    if let Some(s) = taken {
        release(s);
        log(&format!("pty: session {id} closed"));
    }
}

fn release(s: Session) {
    if s.job != 0 {
        unsafe {
            let _ = CloseHandle(HANDLE(s.job as *mut c_void));
        }
    }
    s.shared.stop_reader();
    let hpc = s.hpc;
    let shared = s.shared.clone();
    std::thread::spawn(move || shared.close_pc(hpc));
    // dropping `s` drops the input and resize senders: the writer and resizer threads end and close their handles
}

/// Kill every process tree (job close) first, then close the pseudoconsoles on worker threads.
pub fn shutdown() {
    let all: Vec<Session> = SESSIONS.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
    if all.is_empty() {
        return;
    }
    for s in all {
        release(s);
    }
    std::thread::sleep(Duration::from_millis(150));
    log("pty: shutdown done (jobs closed, pseudoconsoles closing)");
}
