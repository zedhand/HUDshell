//! HUDshell: a compact always-on-top PowerShell 7 terminal for Windows 11 (Win32 window +
//! WebView2/xterm.js renderer + ConPTY host).
//!
//! Global hotkeys (RegisterHotKey only - no hooks, no injection; all configurable in config.json):
//!   Ctrl+Alt+T  toggle click-through      Ctrl+Alt+G  grab / release keyboard focus
//!   Ctrl+Alt+V  show / hide               Ctrl+Alt+C  next colour theme
//!   Ctrl+Alt+X  ghost (click-through + dim)   Ctrl+Alt+Q  quit
//! Settings: %APPDATA%\HUDshell\config.json. Log: %LOCALAPPDATA%\HUDshell\hudshell.log (size-capped,
//! no window titles and no terminal content). The test harness (`--features harness`) is not
//! part of a normal build.

#![windows_subsystem = "windows"]
// Win32/WebView2 calls are wrapped in explicit unsafe blocks even inside unsafe functions (Rust 2024),
// which makes some nested blocks redundant; they are kept so each unsafe call stays visibly marked.
#![allow(unused_unsafe)]

mod assets;
mod config;
#[cfg(feature = "harness")]
mod harness;
mod place;
mod pty;
mod shells;
mod wv;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, SetFocus, HOT_KEY_MODIFIERS};
use windows::Win32::UI::WindowsAndMessaging::*;

const HK_CLICKTHROUGH: i32 = 1;
const HK_FOCUS: i32 = 2;
const HK_SHOWHIDE: i32 = 3;
const HK_QUIT: i32 = 4;
const HK_THEME: i32 = 5;
const HK_OPACITY_UP: i32 = 6;
const HK_OPACITY_DOWN: i32 = 7;
const HK_SETTINGS: i32 = 8;
const HK_GHOST: i32 = 9;
const HK_CORNER_NEXT: i32 = 10;
const HK_CORNER_PREV: i32 = 11;
const HK_SIZE: i32 = 12;
const HK_FONT_UP: i32 = 13;
const HK_FONT_DOWN: i32 = 14;
const HK_SHELL_NEXT: i32 = 20;
const HK_SHELL_PREV: i32 = 21;
const HK_SHELL_NEW: i32 = 22;
const HK_SHELL_CLOSE: i32 = 23;
const HK_SHELL_LIST: i32 = 24;
const HK_SHELL_PICKER: i32 = 25;
const HK_SHELL_ATTN: i32 = 26;
/// Jump hotkeys use ids 30..=38 for shells 1..=9.
const HK_SHELL_JUMP: i32 = 30;
const WM_TRAY: u32 = 0x8000 + 3;
/// Height of the slim drag strip along the top edge, in 96-dpi pixels (scaled per monitor).
const DRAG_STRIP_PX: i32 = 22;
/// Width of the invisible resize border on every edge, in 96-dpi pixels.
const RESIZE_BORDER_PX: i32 = 6;

/// The user's pointer-mode setting. Default: click-through, so HUDshell never
/// catches mouse input meant for the game. While the terminal holds keyboard focus the window is
/// temporarily clickable regardless; see refresh_pointer_style.
static CLICK_THROUGH: AtomicBool = AtomicBool::new(true);
static TERMINAL_FOCUS: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(true);
static PREV_FOREGROUND: AtomicIsize = AtomicIsize::new(0);


fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

unsafe fn describe(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return "none".to_string();
    }
    let mut class = [0u16; 64];
    let cn = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    // Window titles can reveal what the user is doing, so only test builds ever record them.
    #[cfg(feature = "harness")]
    let title = {
        let mut t = [0u16; 96];
        let tn = unsafe { GetWindowTextW(hwnd, &mut t) } as usize;
        String::from_utf16_lossy(&t[..tn])
    };
    #[cfg(not(feature = "harness"))]
    let title = String::from("(not recorded)");
    format!(
        "0x{:X} class='{}' title='{}'",
        hwnd.0 as usize,
        String::from_utf16_lossy(&class[..cn]),
        title
    )
}

/// Test builds log next to the source (the scripts read it there); a normal build logs to
/// %LOCALAPPDATA%\HUDshell\hudshell.log and keeps one rotated copy.
fn log_path() -> &'static std::path::PathBuf {
    static P: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        #[cfg(feature = "harness")]
        {
            std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/hudshell-test.log"))
        }
        #[cfg(not(feature = "harness"))]
        {
            let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
            let dir = std::path::PathBuf::from(base).join("HUDshell");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("hudshell.log")
        }
    })
}

/// Keep the log small: when it passes 1 MB, move it to `<name>.old` (replacing the old copy).
#[cfg(not(feature = "harness"))]
fn rotate_log_if_large() {
    let p = log_path();
    if std::fs::metadata(p).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::rename(p, p.with_extension("log.old"));
    }
}

fn log(msg: &str) {
    static WRITES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let line = format!("{} {}", now_ms(), msg);
    // Cheap size check at start-up and then every 500 lines (not in test builds, which keep everything).
    #[cfg(not(feature = "harness"))]
    if WRITES.fetch_add(1, Ordering::Relaxed) % 500 == 0 {
        rotate_log_if_large();
    }
    #[cfg(feature = "harness")]
    let _ = &WRITES;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{line}");
    }
}

unsafe fn foreground_note(tag: &str) {
    let fg = unsafe { GetForegroundWindow() };
    log(&format!("{tag}: foreground = {}", unsafe { describe(fg) }));
}

unsafe fn ex_style(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

unsafe fn set_ex_flag(hwnd: HWND, flag: WINDOW_EX_STYLE, on: bool) {
    let mut s = unsafe { ex_style(hwnd) };
    if on {
        s |= flag.0;
    } else {
        s &= !flag.0;
    }
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, s as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

unsafe fn reassert_topmost(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Effective pointer behaviour = click-through setting, except while the terminal has keyboard
/// focus (then the window is clickable so you can select, scroll and click). Single place that
/// touches WS_EX_TRANSPARENT.
unsafe fn refresh_pointer_style(hwnd: HWND) {
    let setting = CLICK_THROUGH.load(Ordering::SeqCst);
    let focused = TERMINAL_FOCUS.load(Ordering::SeqCst);
    let effective = setting && !focused;
    unsafe { set_ex_flag(hwnd, WS_EX_TRANSPARENT, effective) };
    log(&format!(
        "pointer: click-through setting={setting} terminal-focus={focused} -> effective click-through={effective} (exstyle=0x{:X})",
        unsafe { ex_style(hwnd) }
    ));
}

/// Ghost preset: click-through plus a dimmed window, restored on the second press. The dimmed value
/// is never written to config.json: `cfg().opacity` keeps the user's real setting throughout.
static GHOST_ON: AtomicBool = AtomicBool::new(false);
static GHOST_PREV_CT: AtomicBool = AtomicBool::new(true);
/// True while the window is dimmed because the shell has been silent for `idle_dim_seconds`.
static IDLE_DIM: AtomicBool = AtomicBool::new(false);
static LAST_OUTPUT_MS: AtomicU64 = AtomicU64::new(0);

/// The opacity the user chose for the current state: the active value while HUDshell holds the keyboard
/// (when one is set), the normal value otherwise. Ghost and idle dimming are applied on top of this.
fn base_opacity() -> u32 {
    let c = cfg();
    if TERMINAL_FOCUS.load(Ordering::SeqCst) { c.active_opacity.unwrap_or(c.opacity) } else { c.opacity }
}

/// Opacity the window should show right now: the chosen value, dimmed by ghost mode or by idleness (never
/// brighter than the chosen value). The single place that turns state into window alpha.
fn effective_opacity() -> u32 {
    let base = base_opacity();
    if GHOST_ON.load(Ordering::SeqCst) {
        base.min(cfg().ghost_opacity)
    } else if IDLE_DIM.load(Ordering::SeqCst) && !TERMINAL_FOCUS.load(Ordering::SeqCst) {
        base.min(cfg().idle_opacity)
    } else {
        base
    }
}

unsafe fn refresh_opacity(hwnd: HWND) {
    unsafe { apply_opacity(hwnd, effective_opacity()) };
}

unsafe fn ghost_on(hwnd: HWND) {
    if GHOST_ON.swap(true, Ordering::SeqCst) {
        return;
    }
    GHOST_PREV_CT.store(CLICK_THROUGH.load(Ordering::SeqCst), Ordering::SeqCst);
    if TERMINAL_FOCUS.load(Ordering::SeqCst) {
        unsafe { release_focus(hwnd) };
    }
    CLICK_THROUGH.store(true, Ordering::SeqCst);
    unsafe {
        refresh_opacity(hwnd);
        refresh_pointer_style(hwnd);
    }
    // "Never brighten" falls out of effective_opacity: ghost only ever lowers the chosen value.
    log(&format!("ghost on: opacity {}% -> {}%, click-through on", base_opacity(), effective_opacity()));
}

/// Leave ghost mode: restore the opacity and (optionally) the click-through setting from before.
unsafe fn ghost_off(hwnd: HWND, restore_click_through: bool) {
    if !GHOST_ON.swap(false, Ordering::SeqCst) {
        return;
    }
    if restore_click_through {
        CLICK_THROUGH.store(GHOST_PREV_CT.load(Ordering::SeqCst), Ordering::SeqCst);
    }
    unsafe {
        refresh_opacity(hwnd);
        refresh_pointer_style(hwnd);
    }
    log(&format!("ghost off: opacity restored to {}%", effective_opacity()));
}

/// Once a second: dim after `idle_dim_seconds` of shell silence while HUDshell is passive.
unsafe fn idle_tick(hwnd: HWND) {
    let secs = cfg().idle_dim_seconds as u64;
    if secs == 0 {
        return;
    }
    let quiet = (now_ms() as u64).saturating_sub(LAST_OUTPUT_MS.load(Ordering::SeqCst)) >= secs * 1000;
    if quiet != IDLE_DIM.load(Ordering::SeqCst) {
        IDLE_DIM.store(quiet, Ordering::SeqCst);
        unsafe { refresh_opacity(hwnd) };
        log(&format!("idle dim {} -> opacity {}%", if quiet { "on" } else { "off" }, effective_opacity()));
    }
}

/// The shell produced output: remember when, and undo idle dimming at once.
unsafe fn note_output(hwnd: HWND) {
    LAST_OUTPUT_MS.store(now_ms() as u64, Ordering::SeqCst);
    if IDLE_DIM.swap(false, Ordering::SeqCst) {
        unsafe { refresh_opacity(hwnd) };
        log(&format!("idle dim off (output) -> opacity {}%", effective_opacity()));
    }
}

unsafe fn toggle_ghost(hwnd: HWND) {
    if GHOST_ON.load(Ordering::SeqCst) {
        unsafe { ghost_off(hwnd, true) };
    } else {
        unsafe { ghost_on(hwnd) };
    }
}

unsafe fn toggle_click_through(hwnd: HWND) {
    // An explicit click-through change leaves ghost mode (the opacity comes back; the new setting stays).
    unsafe { ghost_off(hwnd, false) };
    let on = !CLICK_THROUGH.load(Ordering::SeqCst);
    CLICK_THROUGH.store(on, Ordering::SeqCst);
    unsafe { refresh_pointer_style(hwnd) };
    log(&format!("click_through setting = {on}"));
    unsafe { foreground_note("after click_through toggle") };
}

/// A click on the terminal while it is interactive (click-through off) but not focused requests
/// keyboard focus first ("click to type"). Called from the page's mousedown message.
fn on_page_click(hwnd_i: isize) {
    if TERMINAL_FOCUS.load(Ordering::SeqCst) || CLICK_THROUGH.load(Ordering::SeqCst) {
        return;
    }
    log("click on the interactive terminal: requesting keyboard focus");
    unsafe { grab_focus(HWND(hwnd_i as *mut _)) };
}

unsafe fn grab_focus(hwnd: HWND) {
    // Typing wants a readable window: leave ghost mode first.
    unsafe { ghost_off(hwnd, true) };
    // Remember who had the keyboard so release can give it back. If HUDshell is already the
    // foreground window (the tray menu briefly makes it so), keep the window recorded before that.
    let cur = unsafe { GetForegroundWindow() };
    if cur != hwnd {
        PREV_FOREGROUND.store(cur.0 as isize, Ordering::SeqCst);
    }
    let prev = HWND(PREV_FOREGROUND.load(Ordering::SeqCst) as *mut _);
    log(&format!("GRAB requested; previous foreground = {}", unsafe { describe(prev) }));
    unsafe {
        set_ex_flag(hwnd, WS_EX_NOACTIVATE, false);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = SetFocus(Some(hwnd));
        let fg = GetForegroundWindow();
        let success = fg == hwnd;
        log(&format!(
            "GRAB: SetForegroundWindow returned {ok}; foreground==HUDshell: {success}; foreground now = {}",
            describe(fg)
        ));
        TERMINAL_FOCUS.store(success, Ordering::SeqCst);
        refresh_opacity(hwnd);
        refresh_pointer_style(hwnd);
        if success {
            wv::focus();
        }
        if !success {
            // Denied: stay a visible, non-interactive HUD.
            set_ex_flag(hwnd, WS_EX_NOACTIVATE, true);
            log("GRAB denied: restored NOACTIVATE, remaining passive HUD");
        }
    }
}

unsafe fn release_focus(hwnd: HWND) {
    unsafe {
        set_ex_flag(hwnd, WS_EX_NOACTIVATE, true);
        TERMINAL_FOCUS.store(false, Ordering::SeqCst);
        refresh_opacity(hwnd);
        refresh_pointer_style(hwnd);
        let prev = HWND(PREV_FOREGROUND.load(Ordering::SeqCst) as *mut _);
        if !prev.0.is_null() && IsWindow(Some(prev)).as_bool() {
            let ok = SetForegroundWindow(prev).as_bool();
            log(&format!("RELEASE: SetForegroundWindow(prev) returned {ok}"));
        } else {
            log("RELEASE: previous foreground window is gone; not restoring");
        }
        foreground_note("after release");
        let _ = SetTimer(Some(hwnd), 1, 250, None);
    }
}

unsafe fn toggle_show(hwnd: HWND) {
    let vis = !VISIBLE.load(Ordering::SeqCst);
    VISIBLE.store(vis, Ordering::SeqCst);
    unsafe {
        if !vis && TERMINAL_FOCUS.load(Ordering::SeqCst) {
            log("hide requested while holding focus: releasing focus first");
            release_focus(hwnd);
        }
        if vis {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            reassert_topmost(hwnd);
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        log(&format!("visible = {vis}"));
        foreground_note("after show/hide");
    }
}

// ---- configuration state -------------------------------------------------------------------

static CFG: Mutex<Option<config::Config>> = Mutex::new(None);
/// Id of the registered "TaskbarCreated" message Explorer broadcasts when it (re)starts.
static TASKBAR_MSG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Problems found before the tray icon exists (bad config, hotkey conflicts); shown once as a
/// tray notification so the user actually sees them instead of only a log line.
static NOTICES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn notice(msg: &str) {
    log(&format!("notice: {msg}"));
    if let Ok(mut n) = NOTICES.lock() {
        n.push(msg.to_string());
    }
}
/// Settings are only written to disk in normal runs; test modes use defaults and never save, so
/// the scripted tests cannot disturb a real user's config (and vice versa).
static PERSIST: AtomicBool = AtomicBool::new(true);

/// True only in a harness build started with a test switch; a normal build is never in test mode.
fn is_test_mode() -> bool {
    #[cfg(feature = "harness")]
    {
        harness::is_test_mode()
    }
    #[cfg(not(feature = "harness"))]
    {
        false
    }
}

fn cfg() -> config::Config {
    CFG.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

fn cfg_update(f: impl FnOnce(&mut config::Config)) {
    if let Ok(mut g) = CFG.lock() {
        if let Some(c) = g.as_mut() {
            f(c);
            if PERSIST.load(Ordering::SeqCst) {
                config::save(c);
            }
        }
    }
}

fn theme_json(c: &config::Config) -> serde_json::Value {
    let col = config::colours_for(c);
    serde_json::json!({
        "background": col.background,
        "foreground": col.foreground,
        "cursor": col.cursor,
        "selectionBackground": col.selection,
    })
}

/// Minimum time between attention cues, so a program ringing the bell in a loop cannot flash or spam.
const ATTENTION_GAP_MS: u64 = 3000;
static ATTENTION_LAST: AtomicU64 = AtomicU64::new(0);

/// Keep only printable characters and cap the length: terminal output must never inject control
/// characters or a wall of text into a system notification.
fn sanitize_notice(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect::<String>().trim().to_string()
}

/// A program rang the bell or sent an OSC 9 / OSC 777 notification. If HUDshell does not hold the keyboard
/// the user is probably elsewhere: pulse the window border, or (window hidden) raise a tray notification.
/// The notification text is shown to the user but never written to the log.
fn on_attention(hwnd_i: isize, kind: &str, text: &str) {
    let now = now_ms() as u64;
    let last = ATTENTION_LAST.load(Ordering::SeqCst);
    if last != 0 && now.saturating_sub(last) < ATTENTION_GAP_MS {
        log(&format!("attention: {kind} ignored (rate limit)"));
        return;
    }
    let focused = TERMINAL_FOCUS.load(Ordering::SeqCst);
    let visible = VISIBLE.load(Ordering::SeqCst);
    let c = cfg();
    if focused {
        log(&format!("attention: {kind} while HUDshell has the keyboard: no cue"));
        return;
    }
    ATTENTION_LAST.store(now, Ordering::SeqCst);
    if visible {
        if c.attention_cue {
            log(&format!("attention: {kind} -> border cue"));
            wv::post("{\"t\":\"cue\"}");
        } else {
            log(&format!("attention: {kind} (cue disabled in config)"));
        }
    } else if c.attention_toast {
        let msg = sanitize_notice(text);
        let msg = if msg.is_empty() { "A program asked for your attention".to_string() } else { msg };
        log(&format!("attention: {kind} while hidden -> tray notification"));
        if !is_test_mode() {
            unsafe { tray_notify_icon(HWND(hwnd_i as *mut std::ffi::c_void), "HUDshell", &msg, false) };
        }
    } else {
        log(&format!("attention: {kind} while hidden (toast disabled in config)"));
    }
}

/// Open the clipboard, retrying briefly: clipboard history, sync and other programs hold it open for a few
/// milliseconds at a time, and a single failed attempt must not look like "nothing to paste".
fn open_clipboard_retry() -> bool {
    use windows::Win32::System::DataExchange::OpenClipboard;
    for attempt in 0..20 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            if attempt > 0 {
                log(&format!("clipboard opened after {attempt} retr{}", if attempt == 1 { "y" } else { "ies" }));
            }
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    log("clipboard could not be opened (held by another program)");
    false
}

/// Unicode text on the clipboard, or None. Read by the host so the page never needs the
/// clipboard-read permission (which would pop a permission prompt over the game).
fn clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    const MAX_CHARS: usize = 1_000_000;
    unsafe {
        if !open_clipboard_retry() {
            return None;
        }
        let out = (|| {
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0;
            while n < MAX_CHARS && *p.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(g);
            Some(s)
        })();
        let _ = CloseClipboard();
        out
    }
}

/// Replace the clipboard text. Used for OSC 52 writes, which the host performs so they work while the
/// window is passive. Never logs the text.
fn set_clipboard_text(text: &str) -> bool {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else { return false };
        let p = GlobalLock(mem) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        let _ = GlobalUnlock(mem);
        if !open_clipboard_retry() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))).is_ok();
        let _ = CloseClipboard();
        if !ok {
            let _ = GlobalFree(Some(HGLOBAL(mem.0)));
        }
        ok
    }
}

/// OSC 52 write from a program (only when `osc52` is "write"). Decodes the base64 text and sets the clipboard.
pub fn on_osc52(b64: &str) {
    use base64::Engine;
    if cfg().osc52 != "write" {
        return;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
        log("OSC 52 write refused: not valid base64");
        return;
    };
    if bytes.len() > 100_000 {
        log("OSC 52 write refused: more than 100 KB");
        return;
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let ok = set_clipboard_text(&text);
    log(&format!("OSC 52 clipboard write from a program: {} characters, ok={ok}", text.chars().count()));
}

/// Right-click paste requested by the page (only when `right_click` is "paste").
pub fn on_right_paste() {
    if cfg().right_click != "paste" {
        return;
    }
    let started = std::time::Instant::now();
    let text = clipboard_text();
    // A read that blocks (for example behind a system consent prompt) would show up here.
    let took = started.elapsed().as_millis();
    match text {
        Some(t) if !t.is_empty() => {
            log(&format!("right-click paste: {} characters (clipboard read took {took} ms)", t.chars().count()));
            wv::post(&serde_json::json!({ "t": "paste", "text": t }).to_string());
        }
        _ => log("right-click paste: clipboard has no text"),
    }
}

/// Tell the page the current colours (live change, no restart).
fn push_theme() {
    let c = cfg();
    wv::post(&serde_json::json!({ "t": "theme", "name": c.theme, "theme": theme_json(&c) }).to_string());
}

fn cycle_theme() {
    let mut name = String::new();
    cfg_update(|c| {
        c.theme = config::next_theme(c);
        name = c.theme.clone();
    });
    log(&format!("colour theme -> {name}"));
    push_theme();
}

/// Whole-window opacity (text and background together), 20-100 percent.
unsafe fn apply_opacity(hwnd: HWND, pct: u32) {
    let pct = pct.clamp(config::MIN_OPACITY, 100);
    let alpha = (pct * 255 / 100) as u8;
    let _ = unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA) };
}

/// Change opacity and optionally save it (sliders save only when released).
unsafe fn set_opacity(hwnd: HWND, pct: u32, save: bool) {
    unsafe { ghost_off(hwnd, true) };
    let pct = pct.clamp(config::MIN_OPACITY, 100);
    // While the keyboard is held and an active opacity is configured, that is the value being edited.
    let focused = TERMINAL_FOCUS.load(Ordering::SeqCst);
    let write = move |c: &mut config::Config| {
        if focused && c.active_opacity.is_some() { c.active_opacity = Some(pct) } else { c.opacity = pct }
    };
    if save {
        cfg_update(write);
        log(&format!("opacity -> {pct}%"));
    } else if let Ok(mut g) = CFG.lock() {
        if let Some(c) = g.as_mut() {
            write(c); // keep memory current while dragging; written on release
        }
    }
    unsafe { refresh_opacity(hwnd) };
}

unsafe fn step_opacity(hwnd: HWND, delta: i32) {
    let cur = base_opacity() as i32;
    unsafe { set_opacity(hwnd, (cur + delta).clamp(config::MIN_OPACITY as i32, 100) as u32, true) };
    push_settings(false);
}

/// True while the settings panel is open because *we* took the keyboard for it.
static SETTINGS_GRABBED: AtomicBool = AtomicBool::new(false);

fn push_settings(open: bool) {
    let c = cfg();
    let themes: Vec<String> = config::presets().iter().map(|(n, _)| n.to_string()).collect();
    wv::post(
        &serde_json::json!({
            "t": "settings", "open": open, "opacity": base_opacity(), "minOpacity": config::MIN_OPACITY,
            "theme": c.theme, "themes": themes,
        })
        .to_string(),
    );
}

unsafe fn open_settings(hwnd: HWND) {
    if !TERMINAL_FOCUS.load(Ordering::SeqCst) {
        unsafe { grab_focus(hwnd) };
        SETTINGS_GRABBED.store(true, Ordering::SeqCst);
    }
    push_settings(true);
}

/// Messages from the settings panel in the page.
pub fn on_settings_message(hwnd_i: isize, v: &serde_json::Value) {
    let hwnd = HWND(hwnd_i as *mut std::ffi::c_void);
    unsafe {
        match v["a"].as_str() {
            Some("opacity") => {
                if let Some(p) = v["v"].as_u64() {
                    set_opacity(hwnd, p as u32, v["save"].as_bool().unwrap_or(false));
                }
            }
            Some("theme") => {
                if let Some(name) = v["v"].as_str() {
                    if config::presets().iter().any(|(n, _)| *n == name) {
                        cfg_update(|c| c.theme = name.to_string());
                        log(&format!("colour theme -> {name} (settings panel)"));
                        push_theme();
                    }
                }
            }
            Some("close") => {
                log("settings panel closed");
                if SETTINGS_GRABBED.swap(false, Ordering::SeqCst) {
                    release_focus(hwnd);
                }
            }
            _ => {}
        }
    }
}

// ---- placement: corners, size presets, magnetic snapping ------------------------------------------

fn to_rect(r: &RECT) -> place::Rect {
    place::Rect::new(r.left, r.top, r.right, r.bottom)
}

/// Work area (screen minus taskbar) of the monitor the window is mostly on.
unsafe fn work_area(hwnd: HWND) -> place::Rect {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
        to_rect(&mi.rcWork)
    } else {
        place::Rect::new(0, 0, 1920, 1080)
    }
}

/// Scale a 96-dpi pixel value to this window's monitor.
unsafe fn scaled(hwnd: HWND, px: u32) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    (px as i64 * dpi as i64 / 96) as i32
}

/// Dock the window in the next (+1) or previous (-1) screen corner of its monitor, keeping its size.
/// A window that is not in a corner yet first goes to the nearest one.
unsafe fn move_to_corner(hwnd: HWND, step: i32) {
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return;
    }
    let win = to_rect(&r);
    let work = unsafe { work_area(hwnd) };
    let margin = unsafe { scaled(hwnd, cfg().snap_margin) };
    let here = place::corner_of(win, work);
    let (hx, hy) = place::corner_pos(here, win.w(), win.h(), work, margin);
    let target = if (win.l - hx).abs() > 4 || (win.t - hy).abs() > 4 { here } else { place::step_corner(here, step) };
    let (x, y) = place::corner_pos(target, win.w(), win.h(), work, margin);
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
    log(&format!("corner {} -> {target} at ({x}, {y})", place::corner_of(win, work)));
    unsafe { save_geometry(hwnd) };
}

/// Switch to the next size preset, keeping the corner the window is nearest to where it is.
unsafe fn cycle_size(hwnd: HWND) {
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return;
    }
    let win = to_rect(&r);
    let work = unsafe { work_area(hwnd) };
    let sizes: Vec<(i32, i32)> =
        cfg().sizes.iter().map(|s| unsafe { (scaled(hwnd, s[0]), scaled(hwnd, s[1])) }).collect();
    let i = place::next_size(&sizes, win.w(), win.h());
    let (w, h) = sizes[i];
    let (w, h) = (w.min(work.w()), h.min(work.h()));
    let corner = place::corner_of(win, work);
    let x = if corner == 1 || corner == 2 { win.r - w } else { win.l };
    let y = if corner == 2 || corner == 3 { win.b - h } else { win.t };
    let x = x.clamp(work.l, (work.r - w).max(work.l));
    let y = y.clamp(work.t, (work.b - h).max(work.t));
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    log(&format!("size preset {i}: {w}x{h} (corner {corner})"));
    unsafe { save_geometry(hwnd) };
}

/// Magnetic edge snapping while the window is dragged (WM_MOVING). Returns true when the rectangle changed.
unsafe fn snap_moving(hwnd: HWND, r: &mut RECT) -> bool {
    let c = cfg();
    let work = unsafe { work_area(hwnd) };
    let before = to_rect(r);
    let after = place::snap_rect(before, work, unsafe { scaled(hwnd, c.snap_distance) }, unsafe { scaled(hwnd, c.snap_margin) });
    if after == before {
        return false;
    }
    *r = RECT { left: after.l, top: after.t, right: after.r, bottom: after.b };
    true
}

/// Make the terminal font bigger or smaller by `delta` points (8-48), remember it and tell the page.
fn change_font(delta: i32) {
    let mut size = 0;
    cfg_update(|c| {
        c.font_size = (c.font_size as i32 + delta).clamp(8, 48) as u32;
        size = c.font_size;
    });
    log(&format!("font size -> {size}"));
    wv::post(&serde_json::json!({ "t": "font", "size": size }).to_string());
}

fn on_screen(g: &config::Geometry) -> bool {
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    g.x + 100 > vx && g.x < vx + vw - 100 && g.y + 50 > vy && g.y < vy + vh - 50
}

/// Remember where the window is (physical pixels). Skipped when hidden or in test modes.
unsafe fn save_geometry(hwnd: HWND) {
    if !PERSIST.load(Ordering::SeqCst) || !VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_ok() {
        cfg_update(|c| {
            c.window = Some(config::Geometry { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top })
        });
    }
}

// ---- tray icon -----------------------------------------------------------------------------

// ---- shells (tabs) ---------------------------------------------------------------------------

static SHELLS: Mutex<shells::Shells> = Mutex::new(shells::Shells::new());
/// The profile each live shell was started from (used again when a finished shell is restarted).
static SPAWN_PROFILES: Mutex<Vec<(u32, config::Profile)>> = Mutex::new(Vec::new());
/// True while the profile picker is open because we took the keyboard for it.
static PICKER_GRABBED: AtomicBool = AtomicBool::new(false);

fn shells_lock() -> std::sync::MutexGuard<'static, shells::Shells> {
    SHELLS.lock().unwrap_or_else(|p| p.into_inner())
}

/// Expand `%NAME%` environment variables in a configured path.
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 => match std::env::var(&after[..j]) {
                Ok(v) => {
                    out.push_str(&v);
                    rest = &after[j + 1..];
                }
                Err(_) => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn profile_named(name: &str) -> config::Profile {
    let c = cfg();
    c.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name)).cloned().unwrap_or_else(|| config::default_profile(&c).clone())
}

/// How to start the shell of session `id` (its profile; a plain PowerShell when unknown).
pub fn spawn_opts_for(id: u32) -> pty::SpawnOpts {
    let p = SPAWN_PROFILES
        .lock()
        .ok()
        .and_then(|l| l.iter().find(|(i, _)| *i == id).map(|(_, p)| p.clone()))
        .unwrap_or_default();
    pty::SpawnOpts { dir: p.dir.map(|d| expand_env(&d)), shell: p.shell, command: p.command }
}

/// Tell the page the shell list: order, which one is active, tab style and the profile names.
pub fn push_tabs() {
    let c = cfg();
    let sh = shells_lock();
    let list: Vec<serde_json::Value> = sh
        .entries()
        .iter()
        .map(|e| serde_json::json!({ "id": e.id, "profile": e.profile, "color": profile_named(&e.profile).color }))
        .collect();
    let names: Vec<String> = c.profiles.iter().map(|p| p.name.clone()).collect();
    wv::post(
        &serde_json::json!({ "t": "tabs", "list": list, "active": sh.active(), "mode": c.tabs, "profiles": names }).to_string(),
    );
}

/// Show shell `id` (the page swaps its view) and make it the one keystrokes and harness writes go to.
pub fn switch_to(id: u32) {
    let ok = shells_lock().activate(id);
    if ok {
        pty::set_active(id);
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
        push_tabs();
        log(&format!("shell {id} is now active"));
    }
}

fn shell_step(delta: i32) {
    let id = shells_lock().step(delta);
    if let Some(id) = id {
        pty::set_active(id);
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
        push_tabs();
        log(&format!("shell {id} is now active (step {delta})"));
    }
}

fn shell_jump(n: usize) {
    let id = shells_lock().jump(n);
    match id {
        Some(id) => {
            pty::set_active(id);
            wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
            push_tabs();
            log(&format!("jumped to shell {n} (id {id})"));
        }
        None => log(&format!("no shell {n} to jump to")),
    }
}

/// Start a new shell from a profile (the default one when `profile` is None) and show it.
pub fn new_shell(profile: Option<&str>) {
    let p = match profile {
        Some(n) => profile_named(n),
        None => config::default_profile(&cfg()).clone(),
    };
    let id = shells_lock().add(&p.name);
    let Some(id) = id else {
        log("new shell refused: the window already holds the most shells");
        wv::post("{\"t\":\"full\"}");
        return;
    };
    if let Ok(mut l) = SPAWN_PROFILES.lock() {
        l.push((id, p.clone()));
    }
    log(&format!("new shell {id} from profile '{}'", p.name));
    shells_lock().activate(id);
    pty::set_active(id);
    wv::post(&serde_json::json!({ "t": "open", "s": id }).to_string());
    push_tabs();
}

/// The close hotkey: a finished shell closes at once, a running one needs a second press within 3 seconds.
fn close_active_shell() {
    let id = shells_lock().active();
    if id == 0 {
        return;
    }
    let step = if pty::has_session(id) {
        shells_lock().request_close(id, now_ms() as u64)
    } else {
        shells::CloseStep::Close
    };
    match step {
        shells::CloseStep::Confirm => {
            let pos = shells_lock().position(id).map(|p| p + 1).unwrap_or(0);
            log(&format!("close shell {id}: press again to confirm"));
            wv::post(&serde_json::json!({ "t": "confirm", "s": id, "n": pos }).to_string());
        }
        shells::CloseStep::Close => close_shell(id),
    }
}

fn close_shell(id: u32) {
    if shells_lock().len() <= 1 {
        // The last shell is never removed: it starts over (fresh shell, same tab).
        pty::close(id);
        log(&format!("shell {id} was the last one: starting it again"));
        wv::post(&serde_json::json!({ "t": "reset", "s": id }).to_string());
        return;
    }
    pty::close(id);
    let next = shells_lock().remove(id);
    if let Ok(mut l) = SPAWN_PROFILES.lock() {
        l.retain(|(i, _)| *i != id);
    }
    log(&format!("shell {id} closed"));
    wv::post(&serde_json::json!({ "t": "close", "s": id }).to_string());
    if let Some(n) = next {
        pty::set_active(n);
        wv::post(&serde_json::json!({ "t": "show", "s": n }).to_string());
    }
    push_tabs();
}

/// The picker hotkey: take the keyboard (like the settings panel) so a number key can choose a profile.
unsafe fn open_picker(hwnd: HWND) {
    if !TERMINAL_FOCUS.load(Ordering::SeqCst) {
        unsafe { grab_focus(hwnd) };
        PICKER_GRABBED.store(true, Ordering::SeqCst);
    }
    push_tabs();
    wv::post("{\"t\":\"picker\"}");
}

pub fn picker_closed(hwnd_i: isize) {
    if PICKER_GRABBED.swap(false, Ordering::SeqCst) {
        unsafe { release_focus(HWND(hwnd_i as *mut std::ffi::c_void)) };
    }
}

// ---- start with Windows ----------------------------------------------------------------------

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Registry value name. Test modes use a different name so a test can never touch the user's real entry.
fn autostart_name() -> &'static str {
    if is_test_mode() { "HUDshell-test" } else { "HUDshell" }
}

fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether this program is registered to start when the user signs in.
fn autostart_enabled() -> bool {
    use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ};
    let key = wide_z(RUN_KEY);
    let name = wide_z(autostart_name());
    unsafe {
        let mut h = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), None, KEY_READ, &mut h).is_err() {
            return false;
        }
        let found = RegQueryValueExW(h, PCWSTR(name.as_ptr()), None, None, None, None).is_ok();
        let _ = RegCloseKey(h);
        found
    }
}

/// Add or remove the per-user startup entry (HKCU Run). Only HUDshell's own value is ever touched.
fn set_autostart(on: bool) -> bool {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let key = wide_z(RUN_KEY);
    let name = wide_z(autostart_name());
    unsafe {
        let mut h = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None).is_err() {
            return false;
        }
        let ok = if on {
            let Ok(exe) = std::env::current_exe() else {
                let _ = RegCloseKey(h);
                return false;
            };
            let value = wide_z(&format!("\"{}\"", exe.display()));
            let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
            RegSetValueExW(h, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)).is_ok()
        } else {
            // A value that is already absent counts as success.
            let r = RegDeleteValueW(h, PCWSTR(name.as_ptr()));
            r.is_ok() || !autostart_enabled()
        };
        let _ = RegCloseKey(h);
        log(&format!("autostart {} -> {ok}", if on { "on" } else { "off" }));
        ok
    }
}

/// Show a balloon/toast from the tray icon (hotkey conflicts, bad config).
unsafe fn tray_notify(hwnd: HWND, title: &str, text: &str) {
    unsafe { tray_notify_icon(hwnd, title, text, true) };
}

unsafe fn tray_notify_icon(hwnd: HWND, title: &str, text: &str, warning: bool) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_INFO, NIIF_INFO, NIIF_WARNING, NIM_MODIFY, NOTIFYICONDATAW};
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
    let t: Vec<u16> = title.encode_utf16().take(63).collect();
    nid.szInfoTitle[..t.len()].copy_from_slice(&t);
    let b: Vec<u16> = text.encode_utf16().take(255).collect();
    nid.szInfo[..b.len()].copy_from_slice(&b);
    let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool();
    log(&format!("tray notification shown: {ok}"));
}

unsafe fn add_tray(hwnd: HWND) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW};
    // Delete first so this is safe to call again after Explorer restarts (no duplicate icons).
    unsafe { remove_tray(hwnd) };
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default();
    let tip: Vec<u16> = "HUDshell".encode_utf16().collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool();
    log(&format!("tray icon added: {ok}"));
}

unsafe fn remove_tray(hwnd: HWND) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

unsafe fn tray_menu(hwnd: HWND) {
    use windows::Win32::Foundation::POINT;
    let prev = unsafe { GetForegroundWindow() };
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let checked = if CLICK_THROUGH.load(Ordering::SeqCst) { MF_CHECKED } else { MF_UNCHECKED };
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Show / hide"));
        let _ = AppendMenuW(menu, MF_STRING | checked, 2, w!("Click-through"));
        let _ = AppendMenuW(menu, MF_STRING, 3, w!("Next colour theme"));
        let _ = AppendMenuW(menu, MF_STRING, 4, w!("Grab / release keyboard focus"));
        let ghost_chk = if GHOST_ON.load(Ordering::SeqCst) { MF_CHECKED } else { MF_UNCHECKED };
        let auto_chk = if autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | ghost_chk, 7, w!("Ghost (click-through + dim)"));
        let _ = AppendMenuW(menu, MF_STRING, 9, w!("New shell"));
        let _ = AppendMenuW(menu, MF_STRING, 10, w!("Next shell"));
        let _ = AppendMenuW(menu, MF_STRING, 6, w!("Settings (opacity, colours)..."));
        let _ = AppendMenuW(menu, MF_STRING | auto_chk, 8, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, 5, w!("Quit HUDshell"));
    }
    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    // A menu only dismisses correctly when its owner is the foreground window.
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let cmd = unsafe {
        TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), hwnd, None)
    };
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
    log(&format!("tray menu command {}", cmd.0));
    if prev != hwnd && !prev.0.is_null() {
        // The window that really had the keyboard before the menu took the foreground.
        PREV_FOREGROUND.store(prev.0 as isize, Ordering::SeqCst);
    }
    unsafe {
        match cmd.0 {
            1 => toggle_show(hwnd),
            2 => toggle_click_through(hwnd),
            3 => cycle_theme(),
            4 => {
                if TERMINAL_FOCUS.load(Ordering::SeqCst) {
                    release_focus(hwnd)
                } else {
                    grab_focus(hwnd)
                }
            }
            5 => {
                let _ = DestroyWindow(hwnd);
            }
            6 => open_settings(hwnd),
            7 => toggle_ghost(hwnd),
            9 => new_shell(None),
            10 => shell_step(1),
            8 => {
                let on = !autostart_enabled();
                if !set_autostart(on) {
                    tray_notify(hwnd, "HUDshell", "Could not change the Start with Windows setting");
                }
            }
            _ => {}
        }
        // Give the keyboard back to whatever had it, unless the user asked to grab it.
        if cmd.0 != 4 && cmd.0 != 5 && cmd.0 != 6 && !TERMINAL_FOCUS.load(Ordering::SeqCst) && prev != hwnd
            && IsWindow(Some(prev)).as_bool()
        {
            let _ = SetForegroundWindow(prev);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            m if m != 0 && m == ACTIVATE_MSG.load(Ordering::SeqCst) => {
                bring_forward(hwnd);
                LRESULT(0)
            }
            WM_HOTKEY => {
                match wparam.0 as i32 {
                    HK_CLICKTHROUGH => toggle_click_through(hwnd),
                    HK_FOCUS => {
                        if TERMINAL_FOCUS.load(Ordering::SeqCst) {
                            release_focus(hwnd)
                        } else {
                            grab_focus(hwnd)
                        }
                    }
                    HK_SHOWHIDE => toggle_show(hwnd),
                    HK_THEME => cycle_theme(),
                    HK_OPACITY_UP => step_opacity(hwnd, 10),
                    HK_OPACITY_DOWN => step_opacity(hwnd, -10),
                    HK_SETTINGS => open_settings(hwnd),
                    HK_GHOST => toggle_ghost(hwnd),
                    HK_CORNER_NEXT => move_to_corner(hwnd, 1),
                    HK_CORNER_PREV => move_to_corner(hwnd, -1),
                    HK_SIZE => cycle_size(hwnd),
                    HK_FONT_UP => change_font(1),
                    HK_FONT_DOWN => change_font(-1),
                    HK_SHELL_NEXT => shell_step(1),
                    HK_SHELL_PREV => shell_step(-1),
                    HK_SHELL_NEW => new_shell(None),
                    HK_SHELL_CLOSE => close_active_shell(),
                    HK_SHELL_LIST => wv::post("{\"t\":\"overlay\"}"),
                    HK_SHELL_PICKER => open_picker(hwnd),
                    HK_SHELL_ATTN => wv::post("{\"t\":\"attentionnext\"}"),
                    n if (HK_SHELL_JUMP..HK_SHELL_JUMP + 9).contains(&n) => shell_jump((n - HK_SHELL_JUMP + 1) as usize),
                    HK_QUIT => {
                        log("quit requested");
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => {
                let hit = (lparam.0 & 0xFFFF) as u32;
                let passive = !TERMINAL_FOCUS.load(Ordering::SeqCst);
                log(&format!("WM_MOUSEACTIVATE hit-test={hit} passive={passive}"));
                if passive {
                    return LRESULT(MA_NOACTIVATE as isize);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ACTIVATE => {
                let state = wparam.0 & 0xFFFF;
                log(&format!("WM_ACTIVATE state={state}"));
                if state == 0 && TERMINAL_FOCUS.load(Ordering::SeqCst) {
                    // Focus was taken away by something else (user clicked the game,
                    // or the game re-asserted itself). Never stay activatable.
                    TERMINAL_FOCUS.store(false, Ordering::SeqCst);
                    refresh_opacity(hwnd);
                    set_ex_flag(hwnd, WS_EX_NOACTIVATE, true);
                    refresh_pointer_style(hwnd);
                    log("focus lost externally: back to passive HUD (NOACTIVATE restored)");
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCCALCSIZE => {
                // No native frame at all: the client area is the whole window (this removes the
                // white sizing bar Windows draws along the top). Resizing and dragging are done by
                // WM_NCHITTEST below.
                if wparam.0 != 0 {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCHITTEST => {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut wr = RECT::default();
                let _ = GetWindowRect(hwnd, &mut wr);
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let b = RESIZE_BORDER_PX * dpi / 96;
                let strip = DRAG_STRIP_PX * dpi / 96;
                let left = x < wr.left + b;
                let right = x >= wr.right - b;
                let top = y < wr.top + b;
                let bottom = y >= wr.bottom - b;
                let hit = if top && left {
                    HTTOPLEFT
                } else if top && right {
                    HTTOPRIGHT
                } else if bottom && left {
                    HTBOTTOMLEFT
                } else if bottom && right {
                    HTBOTTOMRIGHT
                } else if left {
                    HTLEFT
                } else if right {
                    HTRIGHT
                } else if top {
                    HTTOP
                } else if bottom {
                    HTBOTTOM
                } else if y < wr.top + strip {
                    HTCAPTION
                } else {
                    HTCLIENT
                };
                LRESULT(hit as isize)
            }
            WM_TRAY => {
                match (lparam.0 & 0xFFFF) as u32 {
                    WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(hwnd),
                    WM_LBUTTONUP => toggle_show(hwnd),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_NCLBUTTONDOWN => {
                log(&format!("WM_NCLBUTTONDOWN hit-test={} passive={}", wparam.0, !TERMINAL_FOCUS.load(Ordering::SeqCst)));
                foreground_note("at non-client click");
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SIZE => {
                wv::resize(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            pty::WM_APP_PTY => {
                pty::clear_wake();
                note_output(hwnd);
                wv::pump_output();
                LRESULT(0)
            }
            pty::WM_APP_PTY_EXIT => {
                wv::pump_output();
                wv::notify_exit(wparam.0 as pty::SessionId);
                LRESULT(0)
            }
            #[cfg(feature = "harness")]
            WM_TIMER if harness::handles_timer(wparam.0) => {
                harness::on_timer(hwnd, wparam.0);
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == 4 {
                    idle_tick(hwnd);
                } else if wparam.0 == 3 {
                    let _ = KillTimer(Some(hwnd), 3);
                    wv::log_children(hwnd);
                } else if wparam.0 == 1 {
                    let _ = KillTimer(Some(hwnd), 1);
                    foreground_note("250ms after release");
                } else if wparam.0 == 2 {
                    // During drag/resize: who is foreground, active and focused right now?
                    let mut gi = GUITHREADINFO {
                        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                        ..Default::default()
                    };
                    let _ = GetGUIThreadInfo(0, &mut gi);
                    log(&format!(
                        "drag-poll: fg={} active=0x{:X} focus=0x{:X} capture=0x{:X}",
                        describe(GetForegroundWindow()),
                        gi.hwndActive.0 as usize,
                        gi.hwndFocus.0 as usize,
                        gi.hwndCapture.0 as usize
                    ));
                }
                LRESULT(0)
            }
            WM_ACTIVATEAPP => {
                log(&format!("WM_ACTIVATEAPP active={}", wparam.0 != 0));
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ENTERSIZEMOVE => {
                log("WM_ENTERSIZEMOVE (drag/resize began)");
                foreground_note("at size/move start");
                let _ = SetTimer(Some(hwnd), 2, 100, None);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOVING => {
                // lparam points at the rectangle the window is about to occupy; snapping edits it in place.
                let r = unsafe { &mut *(lparam.0 as *mut RECT) };
                if snap_moving(hwnd, r) { LRESULT(1) } else { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_EXITSIZEMOVE => {
                let _ = KillTimer(Some(hwnd), 2);
                log("WM_EXITSIZEMOVE");
                foreground_note("at size/move end");
                save_geometry(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => LRESULT(1),
            m if m != 0 && m == TASKBAR_MSG.load(Ordering::SeqCst) => {
                // Explorer restarted: its tray is empty, so put our icon back.
                log("TaskbarCreated received: re-adding the tray icon");
                if PERSIST.load(Ordering::SeqCst) {
                    add_tray(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                save_geometry(hwnd);
                remove_tray(hwnd);
                pty::shutdown();
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Registered message a second launch posts to the running window.
static ACTIVATE_MSG: AtomicU32 = AtomicU32::new(0);

/// A second launch asked the running window to show itself: unhide, leave ghost mode, stay on top.
unsafe fn bring_forward(hwnd: HWND) {
    unsafe { ghost_off(hwnd, true) };
    let was_hidden = !VISIBLE.load(Ordering::SeqCst);
    if was_hidden {
        unsafe { toggle_show(hwnd) };
    }
    unsafe { reassert_topmost(hwnd) };
    log(&format!("second launch: brought the running window forward (was hidden: {was_hidden})"));
}

fn main() -> windows::core::Result<()> {
    unsafe {
        ACTIVATE_MSG.store(RegisterWindowMessageW(w!("HUDshell.Activate")), Ordering::SeqCst);
        // Single instance: a stale copy would silently hold the hotkeys.
        let _guard = windows::Win32::System::Threading::CreateMutexW(
            None,
            true,
            w!("Local\\HUDshell"),
        );
        if windows::Win32::Foundation::GetLastError()
            == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
        {
            log("ANOTHER INSTANCE IS ALREADY RUNNING - asking it to show itself, then exiting.");
            if let Ok(other) = FindWindowW(w!("HUDshell"), PCWSTR::null()) {
                let _ = PostMessageW(Some(other), ACTIVATE_MSG.load(Ordering::SeqCst), WPARAM(0), LPARAM(0));
            }
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Settings: the real config file in normal runs; fixed defaults (never saved) in test modes.
        let test_mode = is_test_mode();
        PERSIST.store(!test_mode, Ordering::SeqCst);
        let loaded = if test_mode { config::Config::default() } else { config::load() };
        #[cfg(feature = "harness")]
        let loaded = harness::override_config(loaded);
        if let Ok(mut g) = CFG.lock() {
            *g = Some(loaded.clone());
        }
        CLICK_THROUGH.store(loaded.click_through, Ordering::SeqCst);
        pty::set_shell(loaded.shell.clone());
        let (gx, gy, gw, gh) = match &loaded.window {
            Some(g) if on_screen(g) => (g.x, g.y, g.w, g.h),
            _ => (200, 200, 560, 300),
        };
        let hmod = GetModuleHandleW(PCWSTR::null())?;
        let hinstance = hmod.into();
        let class_name = w!("HUDshell");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            class_name,
            w!("HUDshell"),
            WS_POPUP | WS_THICKFRAME,
            gx,
            gy,
            gw,
            gh,
            None,
            None,
            Some(hinstance),
            None,
        )?;
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
        apply_opacity(hwnd, loaded.opacity);

        // Clean look: no DWM border line, rounded corners (Windows 11).
        {
            use windows::Win32::Graphics::Dwm::{
                DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            };
            let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
            let round: i32 = DWMWCP_ROUND.0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &none as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &round as *const i32 as *const std::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }

        let hk = loaded.hotkeys.clone();
        for (id, spec, what) in [
            (HK_CLICKTHROUGH, &hk.click_through, "click-through"),
            (HK_FOCUS, &hk.grab_focus, "grab/release focus"),
            (HK_SHOWHIDE, &hk.show_hide, "show/hide"),
            (HK_THEME, &hk.cycle_colours, "cycle colours"),
            (HK_OPACITY_UP, &hk.opacity_up, "opacity up"),
            (HK_OPACITY_DOWN, &hk.opacity_down, "opacity down"),
            (HK_SETTINGS, &hk.settings, "settings panel"),
            (HK_QUIT, &hk.quit, "quit"),
            (HK_GHOST, &hk.ghost, "ghost (click-through + dim)"),
            (HK_CORNER_NEXT, &hk.corner_next, "next corner"),
            (HK_CORNER_PREV, &hk.corner_prev, "previous corner"),
            (HK_SIZE, &hk.size_cycle, "size presets"),
            (HK_FONT_UP, &hk.font_bigger, "bigger font"),
            (HK_FONT_DOWN, &hk.font_smaller, "smaller font"),
            (HK_SHELL_NEXT, &hk.shell_next, "next shell"),
            (HK_SHELL_PREV, &hk.shell_prev, "previous shell"),
            (HK_SHELL_NEW, &hk.shell_new, "new shell"),
            (HK_SHELL_CLOSE, &hk.shell_close, "close shell"),
            (HK_SHELL_LIST, &hk.shell_list, "shell list"),
            (HK_SHELL_PICKER, &hk.shell_picker, "shell picker"),
            (HK_SHELL_ATTN, &hk.shell_attention, "next shell needing attention"),
        ] {
            match config::parse_hotkey(spec) {
                Some((mods, vk)) => match RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(mods), vk) {
                    Ok(()) => log(&format!("hotkey {spec} ({what}) registered")),
                    Err(e) => {
                        log(&format!("hotkey {spec} ({what}) FAILED: {e}"));
                        notice(&format!(
                            "Hotkey {spec} ({what}) is already used by another program. Change it in {}",
                            config::path().display()
                        ));
                    }
                },
                None => notice(&format!("Hotkey '{spec}' ({what}) is not valid")),
            }
        }
        for n in 1..=9 {
            let spec = format!("{}+{n}", hk.shell_jump);
            match config::parse_hotkey(&spec) {
                Some((mods, vk)) => {
                    if RegisterHotKey(Some(hwnd), HK_SHELL_JUMP + n - 1, HOT_KEY_MODIFIERS(mods), vk).is_err() {
                        log(&format!("hotkey {spec} (shell {n}) FAILED"));
                    }
                }
                None => {
                    notice(&format!("shell_jump '{}' is not valid", hk.shell_jump));
                    break;
                }
            }
        }
        {
            let c0 = cfg();
            shells_lock().adopt(1, &config::default_profile(&c0).name);
            if let Ok(mut l) = SPAWN_PROFILES.lock() {
                l.push((1, config::default_profile(&c0).clone()));
            }
        }
        TASKBAR_MSG.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::SeqCst);
        if !test_mode {
            add_tray(hwnd);
            let msgs: Vec<String> = NOTICES.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default();
            if !msgs.is_empty() {
                tray_notify(hwnd, "HUDshell", &msgs.join("\n"));
            }
        }

        if loaded.start_hidden {
            VISIBLE.store(false, Ordering::SeqCst);
            log("start: window stays hidden (start_hidden); use the show/hide hotkey or the tray menu");
        } else {
            log("start: showing window without activation");
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        reassert_topmost(hwnd);
        foreground_note("after initial show");
        if std::env::args().any(|a| a == "--interactive") {
            CLICK_THROUGH.store(false, Ordering::SeqCst);
        }
        refresh_pointer_style(hwnd);
        LAST_OUTPUT_MS.store(now_ms() as u64, Ordering::SeqCst);
        if loaded.start_ghost {
            ghost_on(hwnd);
        }
        if loaded.idle_dim_seconds > 0 {
            let _ = SetTimer(Some(hwnd), 4, 1000, None);
        }

        wv::set_options(&loaded.unicode, &loaded.shift_enter);
        wv::set_config_json(
            &serde_json::json!({
                "font": loaded.font_family,
                "fontSize": loaded.font_size,
                "scrollback": loaded.scrollback,
                "theme": theme_json(&loaded),
                "copyOnSelect": loaded.copy_on_select,
                "rightClick": loaded.right_click,
                "osc52": loaded.osc52,
            })
            .to_string(),
        );
        #[cfg(feature = "harness")]
        harness::apply_cli_options(&loaded);
        {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
            log("initializing WebView2 (windowed hosting)");
            match wv::init(hwnd) {
                Ok(()) => {
                    log("WebView2 ready");
                    let _ = SetTimer(Some(hwnd), 3, 1500, None);
                    #[cfg(feature = "harness")]
                    harness::arm(hwnd);
                }
                Err(e) => log(&format!("WebView2 init FAILED: {e}")),
            }
            foreground_note("after WebView2 init");
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
