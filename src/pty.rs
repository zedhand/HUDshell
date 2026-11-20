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
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
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
    /// The pseudoconsole has been (or is being) closed; closing twice would be a bug.
    closed: AtomicBool,
    /// The shell process has exited (set by the exit watcher; the session record stays until replaced).
    exited: AtomicBool,
}

struct Session {
    id: SessionId,
    hpc: isize,
    job: isize,
    tx: mpsc::Sender<Vec<u8>>,
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

fn wake(hwnd: isize) {
    if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut c_void)),
                WM_APP_PTY,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub fn clear_wake() {
    WAKE_PENDING.store(false, Ordering::SeqCst);
}

/// Pop one buffered output chunk from any session (UI thread), rotating between sessions.
pub fn take_chunk() -> Option<(SessionId, Vec<u8>)> {
    let list: Vec<(SessionId, Arc<Shared>)> =
        SESSIONS.lock().ok()?.iter().map(|s| (s.id, s.shared.clone())).collect();
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
#[cfg(feature = "harness")]
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
fn shell_candidates(first: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    if let Some(p) = first.filter(|p| !p.trim().is_empty()) {
        v.push(p.to_string());
    }
    if let Some(Some(p)) = SHELL_OVERRIDE.lock().ok().map(|o| o.clone()) {
        v.push(p);
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(';').filter(|d| !d.is_empty()) {
            let p = format!("{}\\pwsh.exe", dir.trim_end_matches('\\'));
            // symlink_metadata does not follow app-execution-alias reparse points.
            if std::fs::symlink_metadata(&p).is_ok() {
                v.push(p);
            }
        }
    }
    if let Ok(d) = std::env::var("LOCALAPPDATA") {
        v.push(format!("{d}\\Microsoft\\WindowsApps\\pwsh.exe"));
    }
    if let Ok(d) = std::env::var("ProgramFiles") {
        v.push(format!("{d}\\PowerShell\\7\\pwsh.exe"));
    }
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

unsafe fn spawn_inner(hwnd: isize, id: SessionId, cols: i16, rows: i16, opts: &SpawnOpts) -> Result<String, String> {
    let e = |what: &str, err: windows::core::Error| format!("{what}: {err}");
    let (mut in_r, mut in_w, mut out_r, mut out_w) =
        (HANDLE::default(), HANDLE::default(), HANDLE::default(), HANDLE::default());
    unsafe {
        CreatePipe(&mut in_r, &mut in_w, None, 0).map_err(|x| e("CreatePipe(in)", x))?;
        CreatePipe(&mut out_r, &mut out_w, None, 0).map_err(|x| e("CreatePipe(out)", x))?;
        #[cfg(feature = "harness")]
        let flags = PTY_FLAGS.load(Ordering::SeqCst);
        #[cfg(not(feature = "harness"))]
        let flags = 0u32;
        log(&format!("pty: CreatePseudoConsole flags=0x{flags:X}"));
        let hpc = CreatePseudoConsole(COORD { X: cols, Y: rows }, in_r, out_w, flags)
            .map_err(|x| e("CreatePseudoConsole", x))?;
        // The pseudoconsole owns its ends now; drop our copies so EOF can be detected.
        let _ = CloseHandle(in_r);
        let _ = CloseHandle(out_w);

        // Process attribute list carrying the HPCON.
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut attr_buf = vec![0u8; size];
        let attr = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut c_void);
        InitializeProcThreadAttributeList(Some(attr), 1, None, &mut size)
            .map_err(|x| e("InitializeProcThreadAttributeList", x))?;
        UpdateProcThreadAttribute(
            attr,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(hpc.0 as *const c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(|x| e("UpdateProcThreadAttribute", x))?;

        let mut si = STARTUPINFOEXW::default();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.lpAttributeList = attr;

        // Environment hygiene: honest identity, truecolor. Do NOT impersonate Windows Terminal.
        std::env::set_var("COLORTERM", "truecolor");
        std::env::set_var("TERM_PROGRAM", "HUDshell");

        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
        let start_dir = match opts.dir.as_deref() {
            Some(d) if std::path::Path::new(d).is_dir() => d.to_string(),
            Some(d) => {
                log(&format!("pty: start directory of session {id} does not exist; using the home folder (dir length {})", d.len()));
                home
            }
            None => home,
        };
        let cwd = wide(&start_dir);
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
                EXTENDED_STARTUPINFO_PRESENT,
                None,
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
        let (path, pi) = match launched {
            Some(x) => x,
            None => {
                let _ = ClosePseudoConsole(hpc);
                return Err(format!("no shell could be started (last: {last_err})"));
            }
        };
        let _ = CloseHandle(pi.hThread);

        // Kill-on-close job so no orphan pwsh/OpenConsole/children survive a HUDshell crash.
        let job = CreateJobObjectW(None, PCWSTR::null()).map_err(|x| e("CreateJobObjectW", x))?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let r1 = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        let r2 = AssignProcessToJobObject(job, pi.hProcess);
        log(&format!(
            "pty: job object configured={} assigned={}",
            r1.is_ok(),
            r2.is_ok()
        ));

        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue { chunks: VecDeque::new(), bytes: 0 }),
            cv: Condvar::new(),
            closed: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        });

        // Reader: blocking ReadFile, bounded queue, wake the UI thread.
        let out_i = out_r.0 as isize;
        let rd_shared = shared.clone();
        std::thread::spawn(move || {
            let h = HANDLE(out_i as *mut c_void);
            let mut buf = vec![0u8; 32 * 1024];
            let mut total: u64 = 0;
            #[cfg(feature = "harness")]
            let mut carry: Vec<u8> = Vec::new();
            loop {
                let mut n = 0u32;
                if unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_err() || n == 0 {
                    break;
                }
                total += n as u64;
                #[cfg(feature = "harness")]
                census_scan(&mut carry, &buf[..n as usize]);
                let chunk = buf[..n as usize].to_vec();
                if let Ok(mut q) = rd_shared.queue.lock() {
                    while q.bytes > MAX_QUEUED_BYTES {
                        q = rd_shared.cv.wait(q).unwrap_or_else(|p| p.into_inner());
                    }
                    q.bytes += chunk.len();
                    q.chunks.push_back(chunk);
                }
                wake(hwnd);
            }
            log(&format!("pty reader {id}: EOF after {total} bytes"));
        });

        // Writer: serializes input so ordering (including query replies) is preserved.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let in_i = in_w.0 as isize;
        std::thread::spawn(move || {
            let h = HANDLE(in_i as *mut c_void);
            for data in rx {
                let mut off = 0usize;
                while off < data.len() {
                    let mut w = 0u32;
                    if unsafe { WriteFile(h, Some(&data[off..]), Some(&mut w), None) }.is_err() {
                        log("pty writer: WriteFile failed");
                        return;
                    }
                    off += w as usize;
                }
            }
        });

        // Exit watcher: tell the UI, let final output drain, then close the pseudoconsole.
        let proc_i = pi.hProcess.0 as isize;
        let hpc_i = hpc.0;
        let ex_shared = shared.clone();
        std::thread::spawn(move || {
            unsafe { WaitForSingleObject(HANDLE(proc_i as *mut c_void), INFINITE) };
            log(&format!("pty: shell process of session {id} exited"));
            ex_shared.exited.store(true, Ordering::SeqCst);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_APP_PTY_EXIT,
                    WPARAM(id as usize),
                    LPARAM(0),
                );
            }
            std::thread::sleep(Duration::from_millis(300));
            if !ex_shared.closed.swap(true, Ordering::SeqCst) {
                unsafe { ClosePseudoConsole(HPCON(hpc_i)) };
            }
        });

        // A profile's command is typed into the new shell once PowerShell has had a moment to start.
        if let Some(cmd) = opts.command.clone().filter(|c| !c.trim().is_empty()) {
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(900));
                let _ = tx2.send(format!("{cmd}\r").into_bytes());
            });
        }
        if let Ok(mut s) = SESSIONS.lock() {
            s.push(Session { id, hpc: hpc.0, job: job.0 as isize, tx, cols, rows, shared });
        }
        Ok(path)
    }
}

/// Write to the session on screen (harness scripts and callers that do not name a session).
#[cfg(feature = "harness")]
pub fn write(bytes: Vec<u8>) {
    write_to(active(), bytes);
}

pub fn write_to(id: SessionId, bytes: Vec<u8>) {
    #[cfg(feature = "harness")]
    if LOG_INPUT.load(Ordering::Relaxed) {
        let hex: Vec<String> = bytes.iter().take(64).map(|b| format!("{b:02x}")).collect();
        log(&format!("PTY-IN {} byte(s): {}", bytes.len(), hex.join(" ")));
    }
    if let Ok(s) = SESSIONS.lock() {
        if let Some(s) = s.iter().find(|s| s.id == id) {
            let _ = s.tx.send(bytes);
        }
    }
}

pub fn resize(id: SessionId, cols: i64, rows: i64) {
    let cols = cols.clamp(1, 500) as i16;
    let rows = rows.clamp(1, 300) as i16;
    if let Ok(mut list) = SESSIONS.lock() {
        if let Some(s) = list.iter_mut().find(|s| s.id == id) {
            if s.cols == cols && s.rows == rows {
                return; // drop no-op resizes
            }
            // A finished shell's pseudoconsole may already be closed: only resize live ones.
            if s.shared.closed.load(Ordering::SeqCst) {
                s.cols = cols;
                s.rows = rows;
                return;
            }
            let r = unsafe { ResizePseudoConsole(HPCON(s.hpc), COORD { X: cols, Y: rows }) };
            log(&format!("pty: resize session {id} {}x{} -> {}x{} ok={}", s.cols, s.rows, cols, rows, r.is_ok()));
            s.cols = cols;
            s.rows = rows;
        }
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
    unsafe {
        let _ = CloseHandle(HANDLE(s.job as *mut c_void));
    }
    let hpc = s.hpc;
    let shared = s.shared.clone();
    std::thread::spawn(move || {
        if !shared.closed.swap(true, Ordering::SeqCst) {
            unsafe { ClosePseudoConsole(HPCON(hpc)) };
        }
    });
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
