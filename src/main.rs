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
// The Win32 structs (NOTIFYICONDATAW and the like) are filled in field by field after `Default::default()`, which is how their sizes and flags are set.
#![allow(clippy::field_reassign_with_default)]

mod assets;
mod config;
mod gamepos;
#[cfg(feature = "harness")]
mod harness;
mod layout;
mod perwin;
mod place;
mod progs;
mod pty;
mod session;
mod shells;
mod wv;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use perwin::PerWin;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, SetFocus, UnregisterHotKey, HOT_KEY_MODIFIERS};
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
const HK_FONT_RESET: i32 = 18;
const HK_LAYOUT: i32 = 15;
const HK_PANE_ZOOM: i32 = 16;
const HK_PANE_NEXT: i32 = 17;
const HK_SHELL_NEXT: i32 = 20;
const HK_SHELL_PREV: i32 = 21;
const HK_SHELL_NEW: i32 = 22;
const HK_SHELL_CLOSE: i32 = 23;
const HK_SHELL_LIST: i32 = 24;
const HK_SHELL_PICKER: i32 = 25;
const HK_SHELL_ATTN: i32 = 26;
const HK_WINDOW_DETACH: i32 = 27;
const HK_WINDOW_NEXT: i32 = 28;
const HK_SHELL_MOVE: i32 = 19;
const HK_GHOST_ALL: i32 = 29;
const HK_SHOWHIDE_ALL: i32 = 39;
/// Jump hotkeys use ids 30..=38 for shells 1..=9.
const HK_SHELL_JUMP: i32 = 30;
const WM_TRAY: u32 = 0x8000 + 3;
/// Posted to a window to open the windows of a saved session. A web view cannot be created from inside a web view's own message callback
/// (it waits for itself), so everything that opens a window from a page message goes through a posted message like this.
const WM_APP_RESTORE_WINDOWS: u32 = 0x8000 + 10;
/// Posted to a window: move shell `wparam` of that window into a window of its own (the page asked, with Shift+click on its tab).
pub const WM_APP_DETACH: u32 = 0x8000 + 11;
/// Posted to a window when the page has handed over the snapshot of a shell that is about to move (wparam = the shell).
const WM_APP_MOVE_READY: u32 = 0x8000 + 12;
/// Height of the slim drag strip along the top edge, in 96-dpi pixels (scaled per monitor).
const DRAG_STRIP_PX: i32 = 22;
/// Width of the invisible resize border on every edge, in 96-dpi pixels.
const RESIZE_BORDER_PX: i32 = 6;

/// The user's pointer-mode setting. Default: click-through, so HUDshell never
/// catches mouse input meant for the game. While the terminal holds keyboard focus the window is
/// temporarily clickable regardless; see refresh_pointer_style.
static CLICK_THROUGH: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(true));
static TERMINAL_FOCUS: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
static VISIBLE: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(true));
static PREV_FOREGROUND: PerWin<AtomicIsize> = PerWin::new(|| AtomicIsize::new(0));


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
            let dir = config::local_app_data().join("HUDshell");
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
    if WRITES.fetch_add(1, Ordering::Relaxed).is_multiple_of(500) {
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
static GHOST_ON: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
static GHOST_PREV_CT: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(true));
/// True while the window is dimmed because the shell has been silent for `idle_dim_seconds`.
static IDLE_DIM: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
static LAST_OUTPUT_MS: PerWin<AtomicU64> = PerWin::new(|| AtomicU64::new(0));

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
    note_active_window();
    if TERMINAL_FOCUS.load(Ordering::SeqCst) || CLICK_THROUGH.load(Ordering::SeqCst) {
        return;
    }
    log("click on the interactive terminal: requesting keyboard focus");
    unsafe { grab_focus(HWND(hwnd_i as *mut _)) };
}

/// Does the window cover its whole monitor (a game in fullscreen or borderless)?
fn covers_monitor(win: HWND) -> bool {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    unsafe {
        let mut r = RECT::default();
        if win.0.is_null() || GetWindowRect(win, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(win, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        (r.left - m.left).abs() <= 2 && (r.top - m.top).abs() <= 2 && (r.right - m.right).abs() <= 2 && (r.bottom - m.bottom).abs() <= 2
    }
}

/// The window that had the keyboard when HUDshell took it, if it covered its monitor: if a grab minimizes it, the program runs in exclusive fullscreen.
static GRAB_FULLSCREEN_PREV: PerWin<AtomicIsize> = PerWin::new(|| AtomicIsize::new(0));

/// How many times timer 13 has looked since the last grab.
static GRAB_WATCH_TRIES: PerWin<AtomicU32> = PerWin::new(|| AtomicU32::new(0));
/// Timer 13 looks every 300 ms for this many tries (a game takes a moment to minimize itself).
const GRAB_WATCH_MAX: u32 = 14;

/// Timer 13, after a grab: was the full-screen program that had the keyboard minimized by it? Then it runs in exclusive fullscreen: give
/// the keyboard back, bring it back up, remember it and say why. Looks for about 4 seconds, then gives up.
unsafe fn check_exclusive_after_grab(hwnd: HWND) {
    let prev = HWND(GRAB_FULLSCREEN_PREV.load(Ordering::SeqCst) as *mut _);
    let tries = GRAB_WATCH_TRIES.fetch_add(1, Ordering::SeqCst) + 1;
    let stop = || {
        GRAB_FULLSCREEN_PREV.store(0, Ordering::SeqCst);
        let _ = unsafe { KillTimer(Some(hwnd), 13) };
    };
    if prev.0.is_null() || !TERMINAL_FOCUS.load(Ordering::SeqCst) || !cfg().exclusive_guard || !unsafe { IsWindow(Some(prev)) }.as_bool() {
        stop();
        return;
    }
    if !unsafe { IsIconic(prev) }.as_bool() {
        if tries >= GRAB_WATCH_MAX {
            stop();
        }
        return;
    }
    stop();
    // Only a minimize while HUDshell still holds the keyboard counts: Win+D, Alt+Tab or a click on the program's own minimize
    // button take the keyboard away first (WM_ACTIVATE clears TERMINAL_FOCUS), and those are the user's doing, not the grab's.
    if unsafe { GetForegroundWindow() } != hwnd {
        log("GRAB watch: the program was minimized while something else had the keyboard; not learned");
        return;
    }
    if let Some(app) = app_of_window(prev) {
        if gamepos::shared_host(&app) {
            log("GRAB watch: the minimized window belongs to a host of other programs; not learned");
        } else {
            cfg_update(|c| gamepos::remember_exclusive(&mut c.exclusive_programs, &app));
        }
    }
    log("GRAB minimized the full-screen program that was in front (exclusive fullscreen): keyboard given back, program restored, remembered");
    unsafe {
        release_focus(hwnd);
        let _ = ShowWindowAsync(prev, SW_RESTORE);
        tray_notify(
            hwnd,
            "HUDshell",
            "That program runs in exclusive fullscreen and was minimized when HUDshell took the keyboard. HUDshell gave it back and will not take the keyboard from it again (the tray menu can clear this list). Use the game's borderless or windowed mode to use HUDshell over it.",
        );
    }
}

unsafe fn grab_focus(hwnd: HWND) {
    // A program in exclusive fullscreen would be minimized by the grab and cannot show HUDshell anyway: leave it alone.
    if cfg().exclusive_guard
        && let Some(app) = foreground_app()
        && gamepos::is_exclusive(&cfg().exclusive_programs, &app)
    {
        log("GRAB refused: the program in front is listed as exclusive fullscreen (exclusive_programs)");
        unsafe {
            tray_notify(
                hwnd,
                "HUDshell",
                "The program in front runs in exclusive fullscreen: taking the keyboard would minimize it, so HUDshell did not. Use its borderless or windowed mode, or turn exclusive_guard off in config.json.",
            );
        }
        return;
    }
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
    // measured now: after the grab an exclusive-fullscreen game may already have shrunk or minimized itself
    let was_full_screen = cur != hwnd && covers_monitor(prev);
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
            note_active_window();
            wv::focus();
            // watch whether a full-screen program that had the keyboard gets minimized by this (exclusive fullscreen)
            log(&format!("GRAB watch: guard={} previous program covered its monitor={}", cfg().exclusive_guard, was_full_screen));
            if cfg().exclusive_guard && was_full_screen {
                GRAB_FULLSCREEN_PREV.store(prev.0 as isize, Ordering::SeqCst);
                GRAB_WATCH_TRIES.store(0, Ordering::SeqCst);
                let _ = SetTimer(Some(hwnd), 13, 300, None);
            }
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
    // change under the lock, write the file outside it (a slow disk must not block every reader of the settings)
    let snapshot = match CFG.lock() {
        Ok(mut g) => g.as_mut().map(|c| {
            f(c);
            c.clone()
        }),
        Err(_) => None,
    };
    if let Some(c) = snapshot
        && PERSIST.load(Ordering::SeqCst)
    {
        config::save(&c);
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
/// When each shell last got a cue or notification: one noisy shell must not use up another's.
static ATTENTION_LAST: Mutex<Vec<(u32, u64)>> = Mutex::new(Vec::new());

/// Invisible formatting characters (bidi overrides and isolates, zero-width characters, tags): a program could use them to make
/// a notification read differently from what it says.
pub fn is_invisible_format(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x061C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB
        | 0xE0000..=0xE007F)
}

/// How a tab is called in a notification: its position in its window, and its name when it has one.
fn tab_label(id: u32) -> String {
    let sh = shells_lock();
    match (sh.position(id), sh.entries().iter().find(|e| e.id == id).map(|e| e.name.clone()).filter(|n| !n.is_empty())) {
        (Some(p), Some(n)) => format!("tab {} ({n})", p + 1),
        (Some(p), None) => format!("tab {}", p + 1),
        _ => "a tab".to_string(),
    }
}

/// Keep only printable characters and cap the length: terminal output must never inject control
/// characters or a wall of text into a system notification.
fn sanitize_notice(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() && !is_invisible_format(*c)).take(120).collect::<String>().trim().to_string()
}

/// A program rang the bell or sent an OSC 9 / OSC 777 notification. If HUDshell does not hold the keyboard
/// the user is probably elsewhere: pulse the window border, or (window hidden) raise a tray notification.
/// The notification text is shown to the user but never written to the log.
fn on_attention(hwnd_i: isize, id: u32, kind: &str, text: &str) {
    let now = now_ms() as u64;
    let last = ATTENTION_LAST.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(i, _)| *i == id).map(|(_, t)| *t).unwrap_or(0);
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
    {
        let mut l = ATTENTION_LAST.lock().unwrap_or_else(|p| p.into_inner());
        l.retain(|(i, t)| *i != id && now.saturating_sub(*t) < 60_000);
        l.push((id, now));
    }
    if visible {
        if c.attention_cue {
            log(&format!("attention: {kind} -> border cue"));
            wv::post("{\"t\":\"cue\"}");
        } else {
            log(&format!("attention: {kind} (cue disabled in config)"));
        }
    } else if c.attention_toast {
        // The text comes from a program: it is named as such, with the tab it came from, so it cannot pass for HUDshell's own words.
        let msg = sanitize_notice(text);
        let from = tab_label(id);
        let msg = if msg.is_empty() { format!("A program in {from} asked for your attention") } else { format!("From {from}: {msg}") };
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
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
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
            // never read past the block, even if the program that wrote it left out the terminating zero
            let limit = MAX_CHARS.min(GlobalSize(g) / 2);
            let mut n = 0;
            while n < limit && *p.add(n) != 0 {
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
/// When a program last set the clipboard with OSC 52 (one write per second at most).
static OSC52_LAST: AtomicU64 = AtomicU64::new(0);

pub fn on_osc52(id: u32, b64: &str) {
    use base64::Engine;
    if cfg().osc52 != "write" {
        return;
    }
    let now = now_ms() as u64;
    let last = OSC52_LAST.load(Ordering::SeqCst);
    if last != 0 && now.saturating_sub(last) < 1000 {
        log("OSC 52 write refused: more than one a second");
        return;
    }
    OSC52_LAST.store(now, Ordering::SeqCst);
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
    if ok {
        // say so on screen: a clipboard that changes by itself is how a pasted command gets swapped
        wv::post(&serde_json::json!({ "t": "flash", "text": format!("a program in {} set the clipboard", tab_label(id)) }).to_string());
    }
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

/// Send a message to the page of every window (settings that are shared by all of them).
fn post_all(json: &str) {
    for (serial, _) in perwin::all() {
        let _w = perwin::enter(serial);
        wv::post(json);
    }
}

/// Tell every page the current colours (live change, no restart).
fn push_theme() {
    let c = cfg();
    post_all(&serde_json::json!({ "t": "theme", "name": c.theme, "theme": theme_json(&c) }).to_string());
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
    } else if let Ok(mut g) = CFG.lock()
        && let Some(c) = g.as_mut() {
            write(c); // keep memory current while dragging; written on release
        }
    unsafe { refresh_opacity(hwnd) };
}

unsafe fn step_opacity(hwnd: HWND, delta: i32) {
    let cur = base_opacity() as i32;
    unsafe { set_opacity(hwnd, (cur + delta).clamp(config::MIN_OPACITY as i32, 100) as u32, true) };
    push_settings(false);
}

/// True while the settings panel is open because *we* took the keyboard for it.
static SETTINGS_GRABBED: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));

fn push_settings(open: bool) {
    let c = cfg();
    let themes: Vec<String> = config::presets().iter().map(|(n, _)| n.to_string()).collect();
    wv::post(
        &serde_json::json!({
            "t": "settings", "open": open, "opacity": base_opacity(), "minOpacity": config::MIN_OPACITY,
            "theme": c.theme, "themes": themes, "textScale": c.text_scale, "uiScale": c.ui_scale,
            "ghost": c.ghost_opacity, "tabs": c.tabs, "lineHeight": c.line_height, "letterSpacing": c.letter_spacing,
        })
        .to_string(),
    );
}

/// Take the keyboard for a panel that needs it. False when the grab was refused (exclusive guard) or denied by Windows: the
/// panel would then sit on a window that cannot be typed into. True with `grabbed` set when this call took the keyboard.
unsafe fn grab_for_panel(hwnd: HWND, grabbed: &AtomicBool, what: &str) -> bool {
    if TERMINAL_FOCUS.load(Ordering::SeqCst) {
        return true;
    }
    unsafe { grab_focus(hwnd) };
    if TERMINAL_FOCUS.load(Ordering::SeqCst) {
        grabbed.store(true, Ordering::SeqCst);
        return true;
    }
    log(&format!("{what} not opened: the keyboard could not be taken"));
    false
}

unsafe fn open_settings(hwnd: HWND) {
    if !unsafe { grab_for_panel(hwnd, &SETTINGS_GRABBED, "settings panel") } {
        return;
    }
    push_settings(true);
}

/// Messages from the settings panel in the page.
pub fn on_settings_message(hwnd_i: isize, v: &serde_json::Value) {
    let hwnd = HWND(hwnd_i as *mut std::ffi::c_void);
    unsafe {
        match v["a"].as_str() {
            Some("opacity") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    set_opacity(hwnd, p as u32, v["save"].as_bool().unwrap_or(false));
                }
            }
            Some("theme") => {
                if let Some(name) = v["v"].as_str()
                    && config::presets().iter().any(|(n, _)| *n == name) {
                        cfg_update(|c| c.theme = name.to_string());
                        log(&format!("colour theme -> {name} (settings panel)"));
                        push_theme();
                    }
            }
            Some("textscale") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    let pct = (p as u32).clamp(50, 300);
                    cfg_update(|c| c.text_scale = pct);
                    log(&format!("text scale -> {pct}% (settings panel)"));
                    post_all(&serde_json::json!({ "t": "textscale", "pct": pct }).to_string());
                }
            }
            Some("uiscale") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    let pct = (p as u32).clamp(50, 300);
                    cfg_update(|c| c.ui_scale = pct);
                    log(&format!("ui scale -> {pct}% (settings panel)"));
                    post_all(&serde_json::json!({ "t": "uiscale", "pct": pct }).to_string());
                }
            }
            Some("ghost") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    let pct = (p as u32).clamp(config::MIN_OPACITY, 100);
                    cfg_update(|c| c.ghost_opacity = pct);
                    log(&format!("ghost opacity -> {pct}% (settings panel)"));
                    for (serial, h) in perwin::all() {
                        let _w = perwin::enter(serial);
                        refresh_opacity(HWND(h as *mut std::ffi::c_void));
                    }
                }
            }
            Some("tabs") => {
                if let Some(m) = v["v"].as_str().filter(|m| matches!(*m, "auto" | "always" | "overlay")) {
                    let m = m.to_string();
                    cfg_update(|c| c.tabs = m.clone());
                    log(&format!("tabs -> {m} (settings panel)"));
                    for (serial, _) in perwin::all() {
                        let _w = perwin::enter(serial);
                        push_tabs();
                    }
                }
            }
            Some("lineheight") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    let pct = (p as u32).clamp(100, 200);
                    cfg_update(|c| c.line_height = pct);
                    log(&format!("line height -> {pct}% (settings panel)"));
                    push_page_cfg();
                }
            }
            Some("spacing") => {
                if let Some(p) = v["v"].as_u64().map(|p| p.min(u32::MAX as u64)) {
                    let px = (p as u32).min(10);
                    cfg_update(|c| c.letter_spacing = px);
                    log(&format!("letter spacing -> {px} px (settings panel)"));
                    push_page_cfg();
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

/// Work area of the monitor a rectangle is mostly on (the nearest one when it is on none).
unsafe fn work_area_of(r: place::Rect) -> place::Rect {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    let rc = RECT { left: r.l, top: r.t, right: r.r, bottom: r.b };
    let mon = unsafe { MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST) };
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
    unsafe { restore_window(hwnd, None) };
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return;
    }
    let win = to_rect(&r);
    let work = unsafe { work_area(hwnd) };
    let margin = unsafe { scaled(hwnd, cfg().snap_margin) };
    let here = place::corner_of(win, work);
    // a window bigger than the work area (a short monitor) is cut to fit; otherwise the size is kept
    let (w, h) = place::fit_for_docking(win.w(), win.h(), work, margin);
    let resized = (w, h) != (win.w(), win.h());
    let (hx, hy) = place::corner_pos(here, w, h, work, margin);
    let mut target = if resized || (win.l - hx).abs() > 4 || (win.t - hy).abs() > 4 { here } else { place::step_corner(here, step) };
    // a window as tall (or wide) as the work area has corners that are the same place: go on to the next one that really is somewhere else
    for _ in 1..place::CORNERS {
        let (px, py) = place::corner_pos(target, w, h, work, margin);
        if !resized && (px - win.l).abs() <= 4 && (py - win.t).abs() <= 4 {
            target = place::step_corner(target, step);
        } else {
            break;
        }
    }
    let (x, y) = place::corner_pos(target, w, h, work, margin);
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, if resized { SWP_NOZORDER | SWP_NOACTIVATE } else { SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE }) };
    log(&format!("corner {} -> {target} at ({x}, {y}){}", place::corner_of(win, work), if resized { format!(", cut to {w}x{h} to fit") } else { String::new() }));
    unsafe { save_geometry(hwnd) };
}

/// Switch to the next size preset, keeping the corner the window is nearest to where it is.
unsafe fn cycle_size(hwnd: HWND) {
    unsafe { restore_window(hwnd, None) };
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return;
    }
    let win = to_rect(&r);
    let work = unsafe { work_area(hwnd) };
    // the presets as they fit the work area (a preset bigger than it is cut to it, so the window that fills it is that preset)
    let sizes = place::fit_sizes(&cfg().sizes.iter().map(|s| unsafe { (scaled(hwnd, s[0]), scaled(hwnd, s[1])) }).collect::<Vec<_>>(), work);
    if sizes.is_empty() {
        return;
    }
    let i = place::next_size(&sizes, win.w(), win.h());
    let (w, h) = sizes[i];
    let corner = place::corner_of(win, work);
    let x = if corner == 1 || corner == 2 { win.r - w } else { win.l };
    let y = if corner == 2 || corner == 3 { win.b - h } else { win.t };
    let x = x.clamp(work.l, (work.r - w).max(work.l));
    let y = y.clamp(work.t, (work.b - h).max(work.t));
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    log(&format!("size preset {i}: {w}x{h} (corner {corner})"));
    unsafe { save_geometry(hwnd) };
}

/// Drag-time state: the position the window would have without snapping (see place::SnapDrag), the rectangle it had
/// before the drag, and whether the drag is a move (not a resize) started on the top strip.
static SNAP_DRAG: PerWin<Mutex<Option<place::SnapDrag>>> = PerWin::new(|| Mutex::new(None));
static DRAG_START: PerWin<Mutex<Option<RECT>>> = PerWin::new(|| Mutex::new(None));
/// After a drag that began on a maximized window: (the cursor's proportional place in the window, the window's width, the sideways
/// correction). Windows keeps the grab offset it took at the start of the drag, measured in the maximized window, so the rectangles it
/// proposes afterwards put the restored window at the wrong place (flush left instead of under the cursor); the correction (worked out
/// at the first proposal after the restore) puts it back under the cursor. Empty outside such a drag.
static RESTORE_ANCHOR: PerWin<Mutex<Option<RestoreAnchor>>> = PerWin::new(|| Mutex::new(None));
struct RestoreAnchor {
    rel: f64,
    w: i32,
    shift: Option<i32>,
}
static DRAG_IS_MOVE: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
/// The window really moved during this drag (a plain click on the strip must not maximize it at the top edge).
static DRAG_MOVED: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
/// The windows that move along with the one being dragged (they touch it, directly or through each other) and where each started. Only one
/// drag happens at a time, so this is not per window. Empty when Shift was held at the start, or the window was maximized.
static DRAG_GROUP: Mutex<Vec<(isize, RECT)>> = Mutex::new(Vec::new());
/// The other windows' rectangles that are not in the group, for snapping to them.
static DRAG_OTHERS: Mutex<Vec<place::Rect>> = Mutex::new(Vec::new());
/// The window fills the work area of a monitor (dragged to the top edge, or the top strip double-clicked); the rectangle and
/// scaling it had before are kept to restore it.
static MAXIMIZED: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));
static RESTORE_RECT: PerWin<Mutex<Option<RECT>>> = PerWin::new(|| Mutex::new(None));
static RESTORE_DPI: PerWin<AtomicU32> = PerWin::new(|| AtomicU32::new(96));

fn lock_or_recover<T>(m: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Whether the window currently has square corners (Windows 11 rounds them by default).
static CORNERS_SQUARE: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));

/// Square corners while the window sits in a corner of the work area or fills it (maximized); rounded otherwise.
/// Windows rounds all four corners or none, so "touches a corner" decides for the whole window.
unsafe fn update_corner_style(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND};
    if hwnd.0.is_null() {
        return;
    }
    let mut wr = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut wr) }.is_err() {
        return;
    }
    let square = place::touches_corner(to_rect(&wr), unsafe { work_area(hwnd) });
    if CORNERS_SQUARE.swap(square, Ordering::SeqCst) == square {
        return;
    }
    let pref: i32 = if square { DWMWCP_DONOTROUND.0 } else { DWMWCP_ROUND.0 };
    let _ = unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const i32 as *const std::ffi::c_void, std::mem::size_of::<i32>() as u32)
    };
    log(&format!("window corners: {}", if square { "square" } else { "rounded" }));
}

/// A drag of `hwnd` begins: find the windows that touch it (they will move along, unless Shift is held) and the rectangles of the others
/// (the dragged window may snap to those).
unsafe fn begin_group_drag(hwnd: HWND, wr: &RECT) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
    let me = hwnd.0 as isize;
    let others: Vec<(isize, RECT)> = perwin::all()
        .into_iter()
        .filter(|(_, h)| *h != me)
        .filter_map(|(_, h)| {
            let w = HWND(h as *mut std::ffi::c_void);
            let mut r = RECT::default();
            (unsafe { IsWindowVisible(w) }.as_bool() && unsafe { GetWindowRect(w, &mut r) }.is_ok()).then_some((h, r))
        })
        .collect();
    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    let moving = DRAG_IS_MOVE.load(Ordering::SeqCst) && !MAXIMIZED.load(Ordering::SeqCst);
    let mut group: Vec<(isize, RECT)> = Vec::new();
    if moving && !shift && !others.is_empty() {
        let mut rects = vec![to_rect(wr)];
        rects.extend(others.iter().map(|(_, r)| to_rect(r)));
        for i in place::group_of(&rects, 0, unsafe { scaled(hwnd, 2) }.max(2)) {
            group.push(others[i - 1]);
        }
    }
    let rest: Vec<place::Rect> =
        if moving { others.iter().filter(|(h, _)| !group.iter().any(|(g, _)| g == h)).map(|(_, r)| to_rect(r)).collect() } else { Vec::new() };
    if !group.is_empty() {
        log(&format!("drag: {} touching window(s) move along", group.len()));
    }
    *lock_or_recover(&DRAG_GROUP) = group;
    *lock_or_recover(&DRAG_OTHERS) = rest;
}

/// Magnetic edge snapping while the window is dragged (WM_MOVING). Returns true when the rectangle changed.
unsafe fn snap_moving(hwnd: HWND, r: &mut RECT) -> bool {
    DRAG_MOVED.store(true, Ordering::SeqCst);
    if MAXIMIZED.swap(false, Ordering::SeqCst) {
        // The first movement of a drag that began on a maximized window: it leaves the maximized state, at the size it had
        // before, staying under the cursor in the same proportion as before (like a normal title bar).
        if let Some(rr) = lock_or_recover(&RESTORE_RECT).take() {
            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = unsafe { GetCursorPos(&mut pt) };
            let mut cur = RECT::default();
            let _ = unsafe { GetWindowRect(hwnd, &mut cur) };
            let k_num = unsafe { GetDpiForWindow(hwnd) }.max(96) as i64;
            let k_den = RESTORE_DPI.load(Ordering::SeqCst).max(96) as i64;
            let w = ((rr.right - rr.left) as i64 * k_num / k_den) as i32;
            let h = ((rr.bottom - rr.top) as i64 * k_num / k_den) as i32;
            let rel = (pt.x - cur.left) as f64 / ((cur.right - cur.left).max(1)) as f64;
            let nl = (pt.x as f64 - rel * w as f64) as i32;
            let nt = r.top;
            *r = RECT { left: nl, top: nt, right: nl + w, bottom: nt + h };
            *lock_or_recover(&SNAP_DRAG) = Some(place::SnapDrag::new(to_rect(r)));
            *lock_or_recover(&DRAG_START) = Some(*r);
            *lock_or_recover(&RESTORE_ANCHOR) = Some(RestoreAnchor { rel, w, shift: None });
            log(&format!("restored from maximized to {w}x{h} at {nl},{nt}: the drag left the maximized state"));
            return true;
        }
    }
    if let Some(a) = lock_or_recover(&RESTORE_ANCHOR).as_mut() {
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = unsafe { GetCursorPos(&mut pt) };
        let dx = *a.shift.get_or_insert_with(|| (pt.x as f64 - a.rel * a.w as f64) as i32 - r.left);
        r.left += dx;
        r.right += dx;
    }
    let c = cfg();
    let proposed = to_rect(r);
    // the screen the drag is heading for: the one the window would be on without snapping (not the one it was last put on)
    let ahead = lock_or_recover(&SNAP_DRAG).as_ref().map(|d| d.peek(proposed)).unwrap_or(proposed);
    let work = unsafe { work_area_of(ahead) };
    let (dist, margin) = unsafe { (scaled(hwnd, c.snap_distance), scaled(hwnd, c.snap_margin)) };
    let others = lock_or_recover(&DRAG_OTHERS).clone();
    let after = lock_or_recover(&SNAP_DRAG)
        .get_or_insert_with(|| place::SnapDrag::new(proposed))
        .step(proposed, work, dist, margin, &others);
    // the windows touching this one follow it by the same amount
    if let Some(start) = *lock_or_recover(&DRAG_START) {
        let (dx, dy) = (after.l - start.left, after.t - start.top);
        for (h, r) in lock_or_recover(&DRAG_GROUP).iter() {
            let _ = unsafe {
                SetWindowPos(HWND(*h as *mut std::ffi::c_void), None, r.left + dx, r.top + dy, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
            };
        }
    }
    #[cfg(feature = "harness")]
    {
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = unsafe { GetCursorPos(&mut pt) };
        log(&format!(
            "SNAPDRAG cursor={},{} in={},{},{},{} out={},{},{},{} work={},{},{},{} dpi={}",
            pt.x, pt.y, proposed.l, proposed.t, proposed.r, proposed.b, after.l, after.t, after.r, after.b, work.l, work.t, work.r, work.b,
            unsafe { GetDpiForWindow(hwnd) }
        ));
    }
    if after == proposed {
        return false;
    }
    *r = RECT { left: after.l, top: after.t, right: after.r, bottom: after.b };
    true
}

/// Fill the work area `work` (of the monitor the cursor or the window is on). `before` is the rectangle to restore later.
unsafe fn maximize_window(hwnd: HWND, work: RECT, before: Option<RECT>) {
    let mut cur = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut cur) };
    if !MAXIMIZED.load(Ordering::SeqCst) {
        *lock_or_recover(&RESTORE_RECT) = Some(before.unwrap_or(cur));
        RESTORE_DPI.store(unsafe { GetDpiForWindow(hwnd) }.max(96), Ordering::SeqCst);
    }
    MAXIMIZED.store(true, Ordering::SeqCst);
    // over the whole monitor the window must also stay above the taskbar (which is itself topmost)
    let over_taskbar = cfg().maximize_area != "work";
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            if over_taskbar { Some(HWND_TOPMOST) } else { None },
            work.left,
            work.top,
            work.right - work.left,
            work.bottom - work.top,
            if over_taskbar { SWP_NOACTIVATE } else { SWP_NOZORDER | SWP_NOACTIVATE },
        )
    };
    log(&format!("maximized to the work area {},{},{},{}", work.left, work.top, work.right, work.bottom));
}

/// Undo `maximize_window`. With `anchor_x` (a drag from the top strip) the window keeps the same proportional place under the cursor.
unsafe fn restore_window(hwnd: HWND, anchor_x: Option<i32>) {
    if !MAXIMIZED.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(rr) = lock_or_recover(&RESTORE_RECT).take() else { return };
    let mut cur = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut cur) };
    // the size it had, in the scaling of the monitor it is on now
    let k_num = unsafe { GetDpiForWindow(hwnd) }.max(96) as i64;
    let k_den = RESTORE_DPI.load(Ordering::SeqCst).max(96) as i64;
    let (w, h) = (((rr.right - rr.left) as i64 * k_num / k_den) as i32, ((rr.bottom - rr.top) as i64 * k_num / k_den) as i32);
    let (x, y) = match anchor_x {
        Some(cx) => {
            let rel = (cx - cur.left) as f64 / ((cur.right - cur.left).max(1)) as f64;
            ((cx as f64 - rel * w as f64) as i32, cur.top)
        }
        None => (rr.left, rr.top),
    };
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    log(&format!("restored from maximized to {w}x{h} at {x},{y}"));
    unsafe { save_geometry(hwnd) };
}

/// On a keyboard layout with an AltGr key (right Alt, which Windows treats as Ctrl+Alt), a Ctrl+Alt+<key> hotkey also swallows
/// the character AltGr+<key> types. The registered hotkeys (as written in the config) that do, with that character.
fn altgr_clashes(specs: &[String]) -> Vec<(String, char)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, MAPVK_VK_TO_VSC};
    let hkl = unsafe { GetKeyboardLayout(0) };
    let mut out = Vec::new();
    for spec in specs {
        let Some((mods, vk)) = config::parse_hotkey(spec) else { continue };
        // Ctrl and Alt both held, no Windows key
        if mods & 0x2 == 0 || mods & 0x1 == 0 || mods & 0x8 != 0 {
            continue;
        }
        // AltGr is reported as Ctrl (left) + Alt (right)
        let mut state = [0u8; 256];
        state[0x11] = 0x80;
        state[0x12] = 0x80;
        state[0xA2] = 0x80;
        state[0xA5] = 0x80;
        if mods & 0x4 != 0 {
            state[0x10] = 0x80;
        }
        let sc = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, Some(hkl)) };
        let mut buf = [0u16; 8];
        // flag 4: do not change the keyboard state (a dead key stays pending as it was)
        let n = unsafe { ToUnicodeEx(vk, sc, &state, &mut buf, 4, Some(hkl)) };
        // exactly one printable character (a layout without AltGr gives nothing or a control character)
        if n == 1 && buf[0] >= 0x20 && buf[0] != 0x7f
            && let Some(ch) = char::from_u32(buf[0] as u32) {
                out.push((spec.clone(), ch));
            }
    }
    out
}

/// Tell the user (once, at start) about hotkeys that take characters typed with AltGr on the current layout.
fn notice_altgr_clashes(registered: &[String]) {
    let clashes = altgr_clashes(registered);
    if clashes.is_empty() {
        return;
    }
    let shown: Vec<String> = clashes.iter().take(5).map(|(s, c)| format!("{s} ({c})")).collect();
    let more = if clashes.len() > 5 { format!(" and {} more", clashes.len() - 5) } else { String::new() };
    log(&format!("AltGr clash: {} hotkey(s) also type a character with AltGr on this keyboard layout", clashes.len()));
    notice(&format!(
        "Your keyboard layout has an AltGr key (right Alt). These HUDshell hotkeys use the same keys, so HUDshell takes the character instead of typing it: {}{}. Change those hotkeys in {} if you need the characters.",
        shown.join(", "),
        more,
        config::path().display()
    ));
}

/// Register every global hotkey of `hk` (a problem becomes a notice); returns the hotkey texts that were registered.
unsafe fn register_hotkeys(hwnd: HWND, hk: &config::Hotkeys) -> Vec<String> {
    unsafe {
        let mut registered: Vec<String> = Vec::new();
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
            (HK_FONT_RESET, &hk.font_reset, "reset font size"),
            (HK_SHELL_NEXT, &hk.shell_next, "next shell"),
            (HK_SHELL_PREV, &hk.shell_prev, "previous shell"),
            (HK_SHELL_NEW, &hk.shell_new, "new shell"),
            (HK_SHELL_CLOSE, &hk.shell_close, "close shell"),
            (HK_SHELL_LIST, &hk.shell_list, "shell list"),
            (HK_SHELL_PICKER, &hk.shell_picker, "shell picker"),
            (HK_SHELL_ATTN, &hk.shell_attention, "next shell needing attention"),
            (HK_LAYOUT, &hk.layout_cycle, "next layout"),
            (HK_PANE_ZOOM, &hk.pane_zoom, "zoom pane"),
            (HK_PANE_NEXT, &hk.pane_next, "next pane"),
            (HK_WINDOW_DETACH, &hk.window_detach, "detach shell into its own window"),
            (HK_WINDOW_NEXT, &hk.window_next, "next window"),
            (HK_SHELL_MOVE, &hk.shell_move, "move shell to the next window"),
            (HK_GHOST_ALL, &hk.ghost_all, "ghost all windows"),
            (HK_SHOWHIDE_ALL, &hk.show_hide_all, "show / hide all windows"),
        ] {
            match config::parse_hotkey(spec) {
                Some((mods, vk)) => match RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(mods), vk) {
                    Ok(()) => {
                        log(&format!("hotkey {spec} ({what}) registered"));
                        registered.push(spec.to_string());
                    }
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
                        notice(&format!("Hotkey {spec} (jump to shell {n}) is already used by another program. Change shell_jump in {}", config::path().display()));
                    } else {
                        registered.push(spec.clone());
                    }
                }
                None => {
                    notice(&format!("shell_jump '{}' is not valid", hk.shell_jump));
                    break;
                }
            }
        }
        registered
    }
}

/// Give back every hotkey id HUDshell uses (before registering a changed set).
unsafe fn unregister_hotkeys(hwnd: HWND) {
    for id in 1..=39 {
        let _ = unsafe { UnregisterHotKey(Some(hwnd), id) };
    }
}

/// Modified time of config.json when HUDshell last looked at it (normal runs only; tests never read or write the real file).
static CFG_STAMP: Mutex<Option<std::time::SystemTime>> = Mutex::new(None);
/// How many polls in a row found the file unreadable or invalid (an editor may be halfway through writing it).
static CFG_ERRS: AtomicU32 = AtomicU32::new(0);

fn cfg_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(config::path()).and_then(|m| m.modified()).ok()
}

/// Show the notices that piled up (tray notification).
unsafe fn flush_notices(hwnd: HWND) {
    let msgs: Vec<String> = NOTICES.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default();
    if !msgs.is_empty() {
        unsafe { tray_notify(hwnd, "HUDshell", &msgs.join("\n")) };
    }
}

/// Every 1.5 s in a normal run: reload the settings when config.json changed on disk.
unsafe fn poll_config(hwnd: HWND) {
    if !PERSIST.load(Ordering::SeqCst) {
        return;
    }
    let Some(m) = cfg_mtime() else { return };
    {
        let mut stamp = lock_or_recover(&CFG_STAMP);
        match *stamp {
            None => {
                *stamp = Some(m);
                return;
            }
            Some(prev) if prev == m => return,
            _ => {}
        }
    }
    match config::try_load() {
        Ok(c) => {
            CFG_ERRS.store(0, Ordering::SeqCst);
            *lock_or_recover(&CFG_STAMP) = Some(m);
            unsafe { reload_with(hwnd, c, false) };
        }
        Err(e) => {
            // an editor may still be writing: only the second poll in a row that finds it bad says so
            if CFG_ERRS.fetch_add(1, Ordering::SeqCst) + 1 >= 2 {
                *lock_or_recover(&CFG_STAMP) = Some(m);
                CFG_ERRS.store(0, Ordering::SeqCst);
                log(&format!("config reload: the settings file is not valid ({e}); keeping the settings in use"));
                unsafe { tray_notify(hwnd, "HUDshell", "config.json is not valid, so the settings in use were kept. Fix the file and save it again.") };
            }
        }
    }
}

/// The tray menu's "Reload settings": read the file now.
unsafe fn reload_now(hwnd: HWND) {
    match config::try_load() {
        Ok(c) => {
            *lock_or_recover(&CFG_STAMP) = cfg_mtime();
            unsafe { reload_with(hwnd, c, true) };
        }
        Err(e) => {
            log(&format!("config reload: the settings file is not valid ({e}); keeping the settings in use"));
            unsafe { tray_notify(hwnd, "HUDshell", "config.json is not valid, so the settings in use were kept.") };
        }
    }
}

/// Make `new` the settings in use and apply what can change while running; a notice names what needs a restart.
unsafe fn reload_with(hwnd: HWND, mut new: config::Config, manual: bool) {
    let old = cfg();
    new.window = old.window.clone();
    let changed = config::changed_keys(&old, &new);
    if changed.is_empty() {
        if manual {
            unsafe { tray_notify(hwnd, "HUDshell", "Settings reloaded: nothing had changed.") };
        }
        return;
    }
    if let Ok(mut g) = CFG.lock() {
        *g = Some(new.clone());
    }
    unsafe { apply_config_change(hwnd, &new, &changed) };
    unsafe { flush_notices(hwnd) };
}

/// Send the settings the pages apply themselves (text, spacing, scrollback, clipboard policy) to every window.
fn push_page_cfg() {
    let new = cfg();
    post_all(
        &serde_json::json!({
            "t": "cfg", "fontSize": new.font_size, "fontFamily": new.font_family, "textScale": new.text_scale, "uiScale": new.ui_scale,
            "lineHeight": new.line_height as f64 / 100.0, "letterSpacing": new.letter_spacing, "scrollback": new.scrollback,
            "copyOnSelect": new.copy_on_select, "rightClick": new.right_click, "osc52": new.osc52,
        })
        .to_string(),
    );
}

unsafe fn apply_config_change(hwnd: HWND, new: &config::Config, changed: &[String]) {
    let has = |k: &str| changed.iter().any(|c| c == k);
    log(&format!("config reload: changed {}", changed.join(", ")));
    // window-level settings go to every window, not only the one that noticed the change
    for (serial, h) in perwin::all() {
        let _w = perwin::enter(serial);
        let wh = HWND(h as *mut std::ffi::c_void);
        if has("idle_dim_seconds") {
            if new.idle_dim_seconds > 0 {
                let _ = unsafe { SetTimer(Some(wh), 4, 1000, None) };
            } else {
                let _ = unsafe { KillTimer(Some(wh), 4) };
                IDLE_DIM.store(false, Ordering::SeqCst); // a window dimmed for idleness must not stay dimmed
            }
        }
        if has("click_through") {
            CLICK_THROUGH.store(new.click_through, Ordering::SeqCst);
            unsafe { refresh_pointer_style(wh) };
        }
        if has("opacity") || has("active_opacity") || has("idle_dim_seconds") || has("idle_opacity") || has("ghost_opacity") {
            unsafe { refresh_opacity(wh) };
        }
    }
    if has("game_positions")
        && let Some((_, first)) = perwin::all().first().copied()
    {
        let h = HWND(first as *mut std::ffi::c_void);
        if new.game_positions {
            let _ = unsafe { SetTimer(Some(h), 7, 1000, None) };
        } else {
            let _ = unsafe { KillTimer(Some(h), 7) };
        }
    }
    if has("theme") || has("custom_colours") {
        push_theme();
    }
    if has("hotkeys") {
        unsafe {
            unregister_hotkeys(hwnd);
            let registered = register_hotkeys(hwnd, &new.hotkeys);
            notice_altgr_clashes(&registered);
        }
    }
    if has("tab_program") {
        let _ = unsafe { SetTimer(Some(hwnd), 5, 2000, None) };
    }
    if has("tabs") || has("profiles") || has("default_profile") || has("tab_program") {
        push_tabs();
    }
    if has("pane_min") || has("split") || has("pane_dividers") || has("layout_fill") {
        {
            let mut l = layout_lock();
            l.set_min(new.pane_min[0], new.pane_min[1]);
            l.set_split(new.split[0], new.split[1]);
        }
        push_layout();
    }
    if has("font_size") {
        FONT_START.store(new.font_size, Ordering::SeqCst);
    }
    if has("restore_session") && new.restore_session == "shells" {
        SESSION_LIVE.store(true, Ordering::SeqCst); // start saving now (it was off at start)
        save_session();
    }
    // everything the page itself applies
    let page_keys = [
        "font_size", "font_family", "text_scale", "ui_scale", "line_height", "letter_spacing", "scrollback", "copy_on_select",
        "right_click", "osc52",
    ];
    if page_keys.iter().any(|k| has(k)) {
        push_page_cfg();
    }
    // what is only read at start
    let restart: Vec<&str> = ["shell", "unicode", "shift_enter", "pane_shells", "layout", "start_hidden", "start_ghost"]
        .into_iter()
        .filter(|k| has(k))
        .collect();
    if !restart.is_empty() {
        notice(&format!("These settings apply after HUDshell is restarted: {}", restart.join(", ")));
    }
}

// ---- session restore (config restore_session = "shells") ----------------------------------------------------------

/// The saved session read at start, until the page is ready and the shells can be started again.
static SESSION_PENDING: Mutex<Option<session::Session>> = Mutex::new(None);
/// Saving is switched on once the start (and a restore) is finished, so the start itself never overwrites the saved session.
static SESSION_LIVE: AtomicBool = AtomicBool::new(false);
/// Where the session is kept in a test run (tests never touch the real settings folder).
static SESSION_FILE: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

fn session_enabled() -> bool {
    cfg().restore_session == "shells" && (PERSIST.load(Ordering::SeqCst) || lock_or_recover(&SESSION_FILE).is_some())
}

fn session_path() -> std::path::PathBuf {
    lock_or_recover(&SESSION_FILE).clone().unwrap_or_else(|| config::path().with_file_name("session.json"))
}

fn load_session() -> Option<session::Session> {
    if !session_enabled() {
        return None;
    }
    let text = std::fs::read_to_string(session_path()).ok()?;
    let s = session::Session::parse(&text);
    if s.is_none() {
        log("session: the saved session is damaged or from another version; starting with one shell");
    }
    s
}

/// Write the open shells and the layout (called whenever the shell list or the layout changes, and at quit).
fn save_session() {
    if !SESSION_LIVE.load(Ordering::SeqCst) || !session_enabled() {
        return;
    }
    let mut windows: Vec<session::Session> = Vec::new();
    for (serial, h) in perwin::all() {
        let _w = perwin::enter(serial);
        let (entries, active): (Vec<(u32, String)>, u32) = {
            let sh = shells_lock();
            (sh.entries().iter().map(|e| (e.id, e.profile.clone())).collect(), sh.active())
        };
        let mut s = session::Session::capture(&entries, active, &layout_lock());
        let names: Vec<String> = shells_lock().entries().iter().map(|e| e.name.clone()).collect();
        if names.iter().any(|n| !n.is_empty()) {
            s.names = names;
        }
        let mut r = RECT::default();
        if unsafe { GetWindowRect(HWND(h as *mut std::ffi::c_void), &mut r) }.is_ok() {
            if MAXIMIZED.load(Ordering::SeqCst)
                && let Some(rr) = *lock_or_recover(&RESTORE_RECT) {
                    r = rr;
                }
            s.rect = Some([r.left, r.top, r.right - r.left, r.bottom - r.top]);
        }
        if !s.shells.is_empty() {
            windows.push(s);
        }
    }
    if windows.is_empty() {
        return;
    }
    let mut s = windows.remove(0);
    s.windows = windows;
    let p = session_path();
    let ok = serde_json::to_string_pretty(&s)
        .map_err(|e| e.to_string())
        .and_then(|text| config::write_durable(&p, &text).map_err(|e| e.to_string()));
    if let Err(e) = ok {
        log(&format!("session: could not save ({e})"));
    }
}

/// Start the saved shells again (shell 1 already runs the first profile) and put the layout back.
fn restore_session(sess: session::Session) {
    for name in sess.shells.iter().skip(1) {
        new_shell(Some(name));
    }
    let ids = shells_lock().ids();
    if sess.names.len() == ids.len() {
        for (id, name) in ids.iter().zip(&sess.names) {
            shells_lock().rename(*id, name);
        }
    }
    match sess.resolve(&ids) {
        Some(r) => {
            layout_lock().restore(r.preset, r.split, r.slots, r.groups, r.focus, &ids);
            shells_lock().activate(r.active);
            pty::set_active(r.active);
            wv::post(&serde_json::json!({ "t": "show", "s": r.active }).to_string());
            log(&format!("session restored: {} shell(s), layout {}", ids.len(), r.preset.name()));
            push_tabs();
        }
        None => log("session: the shells that started do not match the saved session; its layout is ignored"),
    }
}

/// Set while a new page after a WebView2 failure has not yet been told about the open shells.
static WV_RECOVERING: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));

/// The new page (after a WebView2 failure) starts with shell 1 only: open the others in it. Their programs kept running; what the old page had
/// on screen is gone.
pub fn reopen_shells_after_recovery() {
    if !WV_RECOVERING.swap(false, Ordering::SeqCst) {
        return;
    }
    let ids = shells_lock().ids();
    for id in ids.into_iter().filter(|&i| i != 1) {
        wv::post(&serde_json::json!({ "t": "open", "s": id }).to_string());
    }
    log("webview: the open shells were opened again in the new page");
    push_tabs();
}

/// The top strip was double-clicked: maximize, or restore.
pub fn toggle_maximize(hwnd_i: isize) {
    let hwnd = HWND(hwnd_i as *mut std::ffi::c_void);
    unsafe {
        if MAXIMIZED.load(Ordering::SeqCst) {
            restore_window(hwnd, None);
        } else {
            use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(mon, &mut mi).as_bool() {
                maximize_window(hwnd, if cfg().maximize_area == "work" { mi.rcWork } else { mi.rcMonitor }, None);
            }
        }
    }
}

/// The window being dragged becomes a maximized or tiled window: the windows that followed it during the drag go back where they were.
fn restore_drag_group() {
    for (h, r) in lock_or_recover(&DRAG_GROUP).iter() {
        let _ = unsafe {
            SetWindowPos(HWND(*h as *mut std::ffi::c_void), None, r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
        };
    }
}

/// A move drag ended: if the cursor is on the top edge of a monitor, the window fills that monitor's work area, like a normal window.
unsafe fn after_move_drag(hwnd: HWND) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL};
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    let mon = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
        return;
    }
    let slack = unsafe { scaled(hwnd, 2) };
    if MAXIMIZED.load(Ordering::SeqCst) {
        return;
    }
    let c = cfg();
    let area = if c.maximize_area == "work" { mi.rcWork } else { mi.rcMonitor };
    let before = *lock_or_recover(&DRAG_START);
    // an edge where another monitor continues is not a place to tile: the cursor simply crosses over to it
    let neighbour = |x: i32| unsafe { !MonitorFromPoint(POINT { x, y: pt.y }, MONITOR_DEFAULTTONULL).0.is_null() };
    if pt.y <= mi.rcMonitor.top + slack {
        restore_drag_group();
        unsafe { maximize_window(hwnd, area, before) };
    } else if c.edge_tiling && pt.x <= mi.rcMonitor.left + slack && !neighbour(mi.rcMonitor.left - 1) {
        // left half of the screen
        let half = RECT { left: area.left, top: area.top, right: area.left + (area.right - area.left) / 2, bottom: area.bottom };
        log("tiled to the left half of the screen");
        restore_drag_group();
        unsafe { maximize_window(hwnd, half, before) };
    } else if c.edge_tiling && pt.x >= mi.rcMonitor.right - 1 - slack && !neighbour(mi.rcMonitor.right) {
        let half = RECT { left: area.right - (area.right - area.left) / 2, top: area.top, right: area.right, bottom: area.bottom };
        log("tiled to the right half of the screen");
        restore_drag_group();
        unsafe { maximize_window(hwnd, half, before) };
    }
}

/// Text size (points) of each terminal that has its own, by session id, for this run. Others use `font_size`.
static FONT_SIZES: PerWin<Mutex<Vec<(u32, u32)>>> = PerWin::new(|| Mutex::new(Vec::new()));
/// `font_size` as it was at start: what the reset hotkey goes back to (in "all" mode `font_size` itself changes).
static FONT_START: AtomicU32 = AtomicU32::new(14);

/// The text size (points, before `text_scale`) of terminal `id`.
fn font_size_of(id: u32) -> u32 {
    FONT_SIZES
        .lock()
        .ok()
        .and_then(|l| l.iter().find(|(i, _)| *i == id).map(|(_, s)| *s))
        .unwrap_or_else(|| cfg().font_size)
}

/// Make the focused terminal's text bigger or smaller by `delta` points (8-48) and tell the page; `text_scale_hotkeys =
/// "all"` changes every terminal together instead and saves `font_size`.
fn change_font(delta: i32) {
    apply_font(Some(delta));
}

/// Put the text size back to the size it had at start (the focused terminal, or all with `text_scale_hotkeys = "all"`).
fn reset_font() {
    apply_font(None);
}

fn apply_font(delta: Option<i32>) {
    let start = FONT_START.load(Ordering::SeqCst);
    if cfg().text_scale_hotkeys == "all" {
        let mut size = 0;
        cfg_update(|c| {
            c.font_size = match delta {
                Some(d) => (c.font_size as i32 + d).clamp(8, 48) as u32,
                None => start,
            };
            size = c.font_size;
        });
        for (serial, _) in perwin::all() {
            if let Ok(mut l) = FONT_SIZES.of(serial).lock() {
                l.clear(); // every terminal, in every window, follows the common size again
            }
        }
        log(&format!("font size -> {size} (all terminals)"));
        post_all(&serde_json::json!({ "t": "font", "size": size, "all": true }).to_string());
    } else {
        let id = shells_lock().active();
        let size = match delta {
            Some(d) => (font_size_of(id) as i32 + d).clamp(8, 48) as u32,
            None => start,
        };
        if let Ok(mut l) = FONT_SIZES.lock() {
            l.retain(|(i, _)| *i != id);
            l.push((id, size));
        }
        log(&format!("font size -> {size} (shell {id})"));
        wv::post(&serde_json::json!({ "t": "font", "s": id, "size": size }).to_string());
    }
}

/// The rectangle shows at least 100 x 50 pixels on some real monitor (not merely inside the bounding box of all of them: with monitors
/// in an L shape the gap is not a screen).
fn on_screen(g: &config::Geometry) -> bool {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONULL};
    if g.w <= 0 || g.h <= 0 {
        return false;
    }
    let r = RECT { left: g.x, top: g.y, right: g.x.saturating_add(g.w), bottom: g.y.saturating_add(g.h) };
    let mon = unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONULL) };
    if mon.0.is_null() {
        return false;
    }
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
        return false;
    }
    let w = r.right.min(mi.rcMonitor.right) - r.left.max(mi.rcMonitor.left);
    let h = r.bottom.min(mi.rcMonitor.bottom) - r.top.max(mi.rcMonitor.top);
    w >= 100.min(g.w) && h >= 50.min(g.h)
}

/// Remember where the window is (physical pixels). Skipped when hidden or in test modes.
unsafe fn save_geometry(hwnd: HWND) {
    // `window` in the settings is the first window's place; the other windows' places are kept in the session file
    if perwin::all().first().map(|(_, h)| *h) != Some(hwnd.0 as isize) {
        return;
    }
    if !VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_ok() {
        if MAXIMIZED.load(Ordering::SeqCst)
            && let Some(rr) = *lock_or_recover(&RESTORE_RECT) {
                r = rr;
            }
        remember_game_place(&r);
        if !PERSIST.load(Ordering::SeqCst) {
            return;
        }
        cfg_update(|c| {
            c.window = Some(config::Geometry { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top })
        });
    }
}

// ---- window places per program (config game_positions) ----------------------------------------------------------

/// The program in front (the file name of its executable, lower case), or None when it is HUDshell itself, the desktop shell or unknown.
/// Window titles are never read, and the name is never written to the log.
fn foreground_app() -> Option<String> {
    #[cfg(feature = "harness")]
    if let Some(f) = lock_or_recover(&FAKE_FOREGROUND).clone() {
        return if gamepos::ignored(&f) { None } else { Some(f) };
    }
    app_of_window(unsafe { GetForegroundWindow() })
}

/// The program that owns `win` (see `foreground_app`).
fn app_of_window(win: HWND) -> Option<String> {
    use windows::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let fg = win;
        if fg.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(h);
        if !ok {
            return None;
        }
        let app = gamepos::app_key(&String::from_utf16_lossy(&buf[..n as usize]));
        if gamepos::ignored(&app) { None } else { Some(app) }
    }
}

#[cfg(feature = "harness")]
static FAKE_FOREGROUND: Mutex<Option<String>> = Mutex::new(None);
/// The program whose saved place was applied last (so a place is applied when the program comes to the front, not every second).
static LAST_GAME: Mutex<Option<String>> = Mutex::new(None);

/// The first window was placed (moved, docked, resized): with `game_positions` on, remember it for the program in front.
fn remember_game_place(r: &RECT) {
    if !cfg().game_positions {
        return;
    }
    let Some(app) = foreground_app() else { return };
    cfg_update(|c| gamepos::remember(&mut c.game_places, &app, [r.left, r.top, r.right - r.left, r.bottom - r.top]));
    *lock_or_recover(&LAST_GAME) = Some(app); // it is where the user put it now
    log("game positions: the window's place was remembered for the program in front");
}

/// Every second (timer 7, only with `game_positions` on): when a program with a remembered place comes to the front, the first window goes there.
unsafe fn poll_game_places() {
    let c = cfg();
    if !c.game_positions {
        return;
    }
    let Some(app) = foreground_app() else { return };
    if lock_or_recover(&LAST_GAME).as_deref() == Some(app.as_str()) {
        return;
    }
    *lock_or_recover(&LAST_GAME) = Some(app.clone());
    let Some([x, y, w, h]) = gamepos::lookup(&c.game_places, &app) else { return };
    let Some((_, first)) = perwin::all().first().copied() else { return };
    if lock_or_recover(&SNAP_DRAG).is_some() || !on_screen(&config::Geometry { x, y, w, h }) {
        return;
    }
    let hwnd = HWND(first as *mut std::ffi::c_void);
    let _w = perwin::enter(perwin::all()[0].0);
    unsafe { restore_window(hwnd, None) };
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    log("game positions: the window went to the place remembered for the program in front");
}

// ---- tray icon -----------------------------------------------------------------------------

// ---- shells (tabs) ---------------------------------------------------------------------------

static SHELLS: PerWin<Mutex<shells::Shells>> = PerWin::new(|| Mutex::new(shells::Shells::new()));
/// The profile each live shell was started from (used again when a finished shell is restarted).
static SPAWN_PROFILES: Mutex<Vec<(u32, config::Profile)>> = Mutex::new(Vec::new());
/// True while the profile picker is open because we took the keyboard for it.
static PICKER_GRABBED: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));

fn shells_lock() -> std::sync::MutexGuard<'static, shells::Shells> {
    SHELLS.lock().unwrap_or_else(|p| p.into_inner())
}

/// Split panes: which shell each pane shows, the focused pane, zoom (see layout.rs).
static LAYOUT: PerWin<Mutex<layout::Layout>> = PerWin::new(|| Mutex::new(layout::Layout::new()));

fn layout_lock() -> std::sync::MutexGuard<'static, layout::Layout> {
    LAYOUT.lock().unwrap_or_else(|p| p.into_inner())
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
        .map(|e| serde_json::json!({ "id": e.id, "profile": e.profile, "name": e.name, "color": profile_named(&e.profile).color }))
        .collect();
    let names: Vec<String> = c.profiles.iter().map(|p| p.name.clone()).collect();
    wv::post(
        &serde_json::json!({ "t": "tabs", "list": list, "active": sh.active(), "mode": c.tabs, "profiles": names }).to_string(),
    );
    drop(sh);
    push_layout();
    save_session();
}

/// Tell the page what to draw: the panes on screen (shape, which shell in each), focus, zoom and split.
pub fn push_layout() {
    let c = cfg();
    let l = layout_lock();
    let shown = l.shown();
    let (sc, sr) = shown.shape();
    let cells: Vec<serde_json::Value> =
        l.cells().iter().map(|(i, id)| serde_json::json!({ "slot": i, "id": id })).collect();
    let (spc, spr) = l.split();
    let groups: Vec<Vec<u32>> = if l.own() { (0..l.slots().len()).map(|i| l.group(i).to_vec()).collect() } else { Vec::new() };
    wv::post(
        &serde_json::json!({
            "t": "layout", "preset": l.preset().name(), "shown": shown.name(), "shape": [sc, sr], "cells": cells,
            "focus": l.focus(), "zoom": l.zoomed(), "split": [spc, spr], "dividers": c.pane_dividers,
            "hint": c.hotkeys.shell_new, "own": l.own(), "groups": groups, "strip": l.tab_order().unwrap_or_default(),
        })
        .to_string(),
    );
}

/// The user named a tab by hand (double-click on it in the tab strip, then type); an empty name goes back to the automatic one.
pub fn on_tab_rename(id: u32, name: &str) {
    if shells_lock().rename(id, name) {
        log(&format!("tab {id} renamed by hand"));
        push_tabs();
    }
}

/// A tab was dragged to another place in the tab strip. With per-pane lists the strip shows one pane's own shells, so it keeps its order.
pub fn on_tab_reorder(id: u32, to: usize) {
    if layout_lock().own() {
        wv::post("{\"t\":\"flash\",\"text\":\"tabs keep their order with per-pane shell lists\"}");
        return;
    }
    if shells_lock().move_to(id, to) {
        log(&format!("tab {id} moved to place {}", to + 1));
        push_tabs();
    }
}

/// The focused pane changed: its shell (if any) becomes the active one.
fn pane_focus_changed() {
    let (i, id) = {
        let l = layout_lock();
        (l.focus(), l.focused_id())
    };
    log(&format!("pane focus -> pane {} (shell {id})", i + 1));
    if id != 0 {
        shells_lock().activate(id);
        pty::set_active(id);
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
    }
    push_tabs();
}

/// After a new layout was chosen: remember it if asked to, start shells for empty panes (layout_fill =
/// "new"), and keep the focus on the shell that had it.
fn after_layout_change(empty: usize, active: u32) {
    let c = cfg();
    if c.remember_layout {
        let name = layout_lock().preset().name().to_string();
        cfg_update(|c| c.layout = name);
    }
    if empty > 0 && c.layout_fill == "new" {
        let mut started = 0;
        let (own, panes) = {
            let l = layout_lock();
            (l.own(), l.empty_panes())
        };
        for i in panes.into_iter().take(empty) {
            if shells_lock().len() >= shells::MAX_SHELLS {
                break;
            }
            if own {
                layout_lock().set_focus(i); // a new shell joins the focused pane's own list
            }
            new_shell(None);
            started += 1;
        }
        log(&format!("layout: started {started} shell(s) for empty panes"));
        if started > 0 && active != 0 {
            switch_to(active);
            return;
        }
    }
    pane_focus_changed();
}

/// The layout hotkey: single -> side by side -> stacked -> 2 x 2 -> single.
fn layout_cycle() {
    let (order, active) = {
        let sh = shells_lock();
        (sh.ids(), sh.active())
    };
    let (p, empty) = {
        let mut l = layout_lock();
        let e = l.cycle(&order, active);
        (l.preset(), e)
    };
    log(&format!("layout -> {} ({} panes, {} empty)", p.name(), p.panes(), empty));
    after_layout_change(empty, active);
}

/// The saved session of a window that a restore opened, until its page is ready.
static WINDOW_PENDING: PerWin<Mutex<Option<session::Session>>> = PerWin::new(|| Mutex::new(None));
/// How many restored windows have not yet got their shells back; saving starts when it reaches 0.
static PENDING_RESTORES: AtomicU32 = AtomicU32::new(0);

/// The saved sessions of the other windows, until the posted message opens them.
static PENDING_WINDOW_SESSIONS: Mutex<Vec<session::Session>> = Mutex::new(Vec::new());

/// One restored window is done (or could not be opened); when none is left, saving starts.
fn finish_one_restore() {
    if PENDING_RESTORES.fetch_sub(1, Ordering::SeqCst) == 1 {
        SESSION_LIVE.store(true, Ordering::SeqCst);
        save_session();
    }
}

/// Open a window for a saved one (at its saved place, with its first shell). Its other shells and its layout follow when its page is ready.
unsafe fn restore_extra_window(s: session::Session) -> bool {
    let p = profile_named(&s.shells[0]);
    let id = NEXT_SHELL_ID.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut l) = SPAWN_PROFILES.lock() {
        l.push((id, p.clone()));
    }
    let at = s.rect.filter(|r| on_screen(&config::Geometry { x: r[0], y: r[1], w: r[2], h: r[3] }));
    let Some(serial) = (unsafe { create_window(id, &p.name, at) }) else {
        log("session: a saved window could not be opened");
        finish_one_restore();
        return false;
    };
    let _w = perwin::enter(serial);
    *lock_or_recover(&WINDOW_PENDING) = Some(s);
    true
}

/// Set once the page of a window has been told its start layout (a window made by detaching a shell never gets the configured one).
static LAYOUT_STARTED: PerWin<AtomicBool> = PerWin::new(|| AtomicBool::new(false));

/// Apply the configured start layout once the page is up (it needs the first shell's view).
pub fn layout_page_ready() {
    if LAYOUT_STARTED.swap(true, Ordering::SeqCst) {
        // a window opened by a restore: its page is up now, so its saved shells and layout come back
        let pending = lock_or_recover(&WINDOW_PENDING).take();
        if let Some(sess) = pending {
            restore_session(sess);
            finish_one_restore();
        }
        return;
    }
    let pending = lock_or_recover(&SESSION_PENDING).take();
    if let Some(sess) = pending {
        let others = sess.windows.clone();
        restore_session(sess);
        // the other saved windows are opened from a posted message (see WM_APP_RESTORE_WINDOWS); each finishes its session when its own
        // page is ready, and saving starts when the last one has
        if others.is_empty() {
            SESSION_LIVE.store(true, Ordering::SeqCst);
            save_session();
        } else {
            PENDING_RESTORES.store(others.len() as u32, Ordering::SeqCst);
            *lock_or_recover(&PENDING_WINDOW_SESSIONS) = others;
            let me = perwin::current_hwnd();
            let _ = unsafe { PostMessageW(Some(HWND(me as *mut std::ffi::c_void)), WM_APP_RESTORE_WINDOWS, WPARAM(0), LPARAM(0)) };
        }
        return; // the saved layout wins over the `layout` setting
    }
    if session_enabled() {
        SESSION_LIVE.store(true, Ordering::SeqCst); // nothing to restore: record from now on
    }
    let p = layout::Preset::parse(&cfg().layout).unwrap_or(layout::Preset::Single);
    if p == layout::Preset::Single {
        return;
    }
    let (order, active) = {
        let sh = shells_lock();
        (sh.ids(), sh.active())
    };
    let empty = layout_lock().set_preset(p, &order, active);
    log(&format!("layout at start: {} ({} empty)", p.name(), empty));
    after_layout_change(empty, active);
}

fn pane_step(delta: i32) {
    layout_lock().focus_step(delta);
    pane_focus_changed();
}

/// A click on a pane (the page sends its index; used for empty panes, which have no shell to switch to).
pub fn on_pane_focus(i: usize) {
    if layout_lock().set_focus(i) {
        pane_focus_changed();
    }
}

fn pane_zoom() {
    let (n, on) = {
        let mut l = layout_lock();
        (l.slots().len(), l.toggle_zoom())
    };
    if n <= 1 {
        log("zoom: there is only one pane");
    } else {
        log(&format!("zoom {}", if on { "on" } else { "off" }));
    }
    push_layout();
}

/// The page measured the terminal area in cells; a window too small for the layout shows fewer panes.
pub fn on_area(cols: u32, rows: u32) {
    let changed = layout_lock().set_area(cols, rows);
    if changed {
        log(&format!("layout: area {cols}x{rows} cells, showing {}", layout_lock().shown().name()));
        push_layout();
    }
}

/// The program label of each shell (session id, name), as last sent to the page.
static PROGS_SENT: PerWin<Mutex<Vec<(u32, String)>>> = PerWin::new(|| Mutex::new(Vec::new()));

/// Every couple of seconds: look at what runs below each shell and tell the page when a label changed.
fn poll_programs() {
    let mode = cfg().tab_program;
    let pids = pty::shell_pids();
    let mut now: Vec<(u32, String)> = Vec::new();
    if mode != "off" && !pids.is_empty() {
        let all = progs::snapshot();
        for (id, pid) in pids {
            let name = progs::label_for(&all, pid, &mode);
            if !name.is_empty() {
                now.push((id, name));
            }
        }
    }
    now.sort();
    let mut last = PROGS_SENT.lock().unwrap_or_else(|p| p.into_inner());
    if *last != now {
        let list: Vec<serde_json::Value> = now.iter().map(|(s, p)| serde_json::json!({ "s": s, "p": p })).collect();
        // Not logged: what a shell runs says what the user is doing.
        wv::post(&serde_json::json!({ "t": "prog", "list": list }).to_string());
        *last = now;
    }
}

/// The page saw a press on the top strip or on an edge (the web view covers the whole window, so the host's own
/// hit-test never sees it): start Windows' move / resize loop for that hit-test code, as if the press had landed
/// on a title bar or border. Posted, so the loop does not run inside the bridge callback.
pub fn start_native_drag(hwnd_i: isize, ht: u32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    let hwnd = HWND(hwnd_i as *mut std::ffi::c_void);
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // A maximized window is not resized from its edges. Dragging its strip takes it out of the maximized state, but
        // only once it really moves (see snap_moving): a plain click on the strip must not restore it.
        if MAXIMIZED.load(Ordering::SeqCst) && ht != 2 {
            return;
        }
        let _ = ReleaseCapture();
        let lp = ((pt.y as isize & 0xFFFF) << 16) | (pt.x as isize & 0xFFFF);
        let _ = PostMessageW(Some(hwnd), WM_NCLBUTTONDOWN, WPARAM(ht as usize), LPARAM(lp));
    }
}

/// A divider was dragged (pane_dividers = true).
pub fn on_split(cols_pct: u32, rows_pct: u32) {
    let (c, r) = {
        let mut l = layout_lock();
        l.set_split(cols_pct, rows_pct);
        l.split()
    };
    log(&format!("split -> {c},{r}"));
    if cfg().remember_layout {
        cfg_update(|cf| cf.split = [c, r]);
    }
    push_layout();
}

/// Show shell `id` (the page swaps its view) and make it the one keystrokes and harness writes go to.
pub fn switch_to(id: u32) {
    let ok = shells_lock().activate(id);
    if ok {
        layout_lock().show(id);
        pty::set_active(id);
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
        push_tabs();
        log(&format!("shell {id} is now active"));
    }
}

fn shell_step(delta: i32) {
    // pane_shells = "own": step through the focused pane's own shells; otherwise through the whole tab list.
    let own = layout_lock().own();
    let id = if own { layout_lock().group_step(delta) } else { shells_lock().step(delta) };
    if let Some(id) = id {
        if own {
            shells_lock().activate(id);
        } else {
            layout_lock().show(id);
        }
        pty::set_active(id);
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
        push_tabs();
        log(&format!("shell {id} is now active (step {delta})"));
    }
}

fn shell_jump(n: usize) {
    let own = layout_lock().own();
    let id = if own { layout_lock().group_jump(n) } else { shells_lock().jump(n) };
    match id {
        Some(id) => {
            if own {
                shells_lock().activate(id);
            } else {
                layout_lock().show(id);
            }
            pty::set_active(id);
            wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
            push_tabs();
            log(&format!("jumped to shell {n} (id {id})"));
        }
        None => log(&format!("no shell {n} to jump to")),
    }
}

/// Shell (session) ids are unique across windows; shell 1 is the first window's first shell.
static NEXT_SHELL_ID: AtomicU32 = AtomicU32::new(2);

/// The ids of the shells the current window holds, plus (in the first window) sessions no window lists, such as the ones a test script
/// opens by hand: their output goes to the first window's page as it always did.
pub fn window_shell_ids() -> Vec<u32> {
    let mut ids = shells_lock().ids();
    if perwin::all().first().map(|(s, _)| *s) == Some(perwin::current()) {
        let owned = all_listed_shells();
        ids.extend(pty::session_ids().into_iter().filter(|i| !owned.contains(i)));
    }
    ids
}

/// Every shell id some window lists.
fn all_listed_shells() -> Vec<u32> {
    let mut v = Vec::new();
    for (serial, _) in perwin::all() {
        let _w = perwin::enter(serial);
        v.extend(shells_lock().ids());
    }
    v
}

/// The shell a new page of the current window starts with (shell 1 in the first window).
pub fn first_shell_of_window() -> u32 {
    shells_lock().ids().first().copied().unwrap_or(1)
}

/// Run `f` with the window that holds shell `id` as the current one (nothing happens when no window holds it).
pub fn in_window_of(id: u32, f: impl FnOnce()) {
    for (serial, _) in perwin::all() {
        let _w = perwin::enter(serial);
        if shells_lock().position(id).is_some() {
            f();
            return;
        }
    }
    // a session no window lists belongs to the first window's page
    if let Some((serial, _)) = perwin::all().first().copied() {
        let _w = perwin::enter(serial);
        f();
    }
}

/// Send the pending shell output to every window's page; returns the windows that received some or still hold some back.
fn pump_all_windows() {
    for (serial, hwnd) in perwin::all() {
        let _w = perwin::enter(serial);
        let sent = wv::pump_output();
        if sent > 0 {
            unsafe { note_output(HWND(hwnd as *mut std::ffi::c_void)) };
        }
    }
}

/// Start a new shell from a profile (the default one when `profile` is None) and show it.
pub fn new_shell(profile: Option<&str>) {
    let p = match profile {
        Some(n) => profile_named(n),
        None => config::default_profile(&cfg()).clone(),
    };
    let id = if shells_lock().len() >= shells::MAX_SHELLS {
        None
    } else {
        let id = NEXT_SHELL_ID.fetch_add(1, Ordering::SeqCst);
        shells_lock().adopt(id, &p.name);
        Some(id)
    };
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
    layout_lock().place_new(id);
    pty::set_active(id);
    // The layout goes first, so the new view is created in a pane that is already on screen and starts
    // its shell at that pane's size.
    push_layout();
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
        if perwin::count() > 1 {
            // Another window is open: closing the last shell of this one closes the window.
            pty::close(id);
            log(&format!("shell {id} was the last one of its window: closing the window"));
            let hwnd = perwin::current_hwnd();
            if hwnd != 0 {
                let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut std::ffi::c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
            return;
        }
        // The last shell is never removed: it starts over (fresh shell, same tab).
        pty::close(id);
        log(&format!("shell {id} was the last one: starting it again"));
        wv::post(&serde_json::json!({ "t": "reset", "s": id }).to_string());
        return;
    }
    pty::close(id);
    remove_shell_from_window(id);
    if let Ok(mut l) = SPAWN_PROFILES.lock() {
        l.retain(|(i, _)| *i != id);
    }
    log(&format!("shell {id} closed"));
}

/// Take shell `id` out of the current window (its tab, its pane and its page view). The shell itself is not touched: closing it is the
/// caller's business, and detaching it leaves it running for another window.
fn remove_shell_from_window(id: u32) {
    let next = shells_lock().remove(id);
    // (the shell's start profile stays: a shell that only changes window must still restart from it)
    if let Ok(mut l) = FONT_SIZES.lock() {
        l.retain(|(i, _)| *i != id);
    }
    // Its pane shows a shell that was not on screen, if there is one; otherwise the focus goes to the
    // neighbour the tab list picked (and the pane stays empty when every shell is already showing).
    let order = shells_lock().ids();
    let refill = {
        let mut l = layout_lock();
        l.remove(id, &order, next.unwrap_or(0));
        l.focused_id()
    };
    let next = if refill != 0 {
        shells_lock().activate(refill);
        Some(refill)
    } else if layout_lock().own() {
        // The pane's own list is empty: it stays empty and the keyboard goes to a pane that still has a shell.
        let n = layout_lock().focus_nonempty();
        if n != 0 {
            shells_lock().activate(n);
            Some(n)
        } else {
            next
        }
    } else {
        if let Some(n) = next {
            layout_lock().show(n);
        }
        next
    };
    log(&format!("shell {id} left this window"));
    wv::post(&serde_json::json!({ "t": "close", "s": id }).to_string());
    if let Some(n) = next {
        pty::set_active(n);
        wv::post(&serde_json::json!({ "t": "show", "s": n }).to_string());
    }
    push_tabs();
}

/// The picker hotkey: take the keyboard (like the settings panel) so a number key can choose a profile.
unsafe fn open_picker(hwnd: HWND) {
    if !unsafe { grab_for_panel(hwnd, &PICKER_GRABBED, "profile picker") } {
        return;
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
    // The tray icon belongs to the first window: a notification from any other window goes through it.
    let owner = perwin::all().first().map(|(_, h)| HWND(*h as *mut std::ffi::c_void)).unwrap_or(hwnd);
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = owner;
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

/// Ghost mode for every window: all go ghost unless every one already is (then all leave it).
unsafe fn ghost_all() {
    let all = perwin::all();
    let any_off = all.iter().any(|(s, _)| !GHOST_ON.of(*s).load(Ordering::SeqCst));
    for (s, h) in all {
        let _w = perwin::enter(s);
        let h = HWND(h as *mut std::ffi::c_void);
        if any_off {
            unsafe { ghost_on(h) };
        } else {
            unsafe { ghost_off(h, true) };
        }
    }
    log(&format!("ghost {} for all windows", if any_off { "on" } else { "off" }));
}

/// Show or hide every window: all hide when any is visible, otherwise all show.
unsafe fn show_hide_all() {
    let all = perwin::all();
    let any_visible = all.iter().any(|(s, _)| VISIBLE.of(*s).load(Ordering::SeqCst));
    for (s, h) in all {
        let _w = perwin::enter(s);
        if VISIBLE.load(Ordering::SeqCst) == any_visible {
            unsafe { toggle_show(HWND(h as *mut std::ffi::c_void)) };
        }
    }
    log(&format!("{} all windows", if any_visible { "hid" } else { "showed" }));
}

unsafe fn tray_menu(hwnd: HWND) {
    use windows::Win32::Foundation::POINT;
    // The tray icon lives on the first window; the menu acts on the window that was last active (the "all windows" items on every one).
    let (active, active_hwnd) = active_window(hwnd.0 as isize);
    let _active = perwin::enter(active);
    let ahwnd = HWND(active_hwnd as *mut std::ffi::c_void);
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
        let _ = AppendMenuW(menu, MF_STRING, 11, w!("Next layout (split panes)"));
        let zoom_chk = if layout_lock().zoomed() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | zoom_chk, 12, w!("Zoom pane"));
        let _ = AppendMenuW(menu, MF_STRING, 14, w!("Detach this shell into its own window"));
        let _ = AppendMenuW(menu, MF_STRING, 15, w!("Next window"));
        let _ = AppendMenuW(menu, MF_STRING, 16, w!("Ghost all windows"));
        let _ = AppendMenuW(menu, MF_STRING, 17, w!("Show / hide all windows"));
        let _ = AppendMenuW(menu, MF_STRING, 13, w!("Reload settings (config.json)"));
        let _ = AppendMenuW(menu, MF_STRING, 6, w!("Settings (opacity, colours)..."));
        let _ = AppendMenuW(menu, MF_STRING | auto_chk, 8, w!("Start with Windows"));
        let learned = cfg().exclusive_programs.len();
        if learned > 0 {
            let label = windows::core::HSTRING::from(format!("Clear the exclusive-fullscreen list ({learned})"));
            let _ = AppendMenuW(menu, MF_STRING, 18, PCWSTR(label.as_ptr()));
        }
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
            1 => toggle_show(ahwnd),
            2 => toggle_click_through(ahwnd),
            3 => cycle_theme(),
            4 => {
                if TERMINAL_FOCUS.load(Ordering::SeqCst) {
                    release_focus(ahwnd)
                } else {
                    grab_focus(ahwnd)
                }
            }
            5 => quit_all(),
            6 => open_settings(ahwnd),
            7 => toggle_ghost(ahwnd),
            14 => detach_active_shell(),
            15 => next_window(),
            16 => ghost_all(),
            17 => show_hide_all(),
            9 => new_shell(None),
            10 => shell_step(1),
            11 => layout_cycle(),
            12 => pane_zoom(),
            13 => reload_now(hwnd),
            18 => {
                cfg_update(|c| c.exclusive_programs.clear());
                log("exclusive-fullscreen list cleared from the tray menu");
                tray_notify(hwnd, "HUDshell", "The exclusive-fullscreen list is empty: HUDshell takes the keyboard over every program again.");
            }
            8 => {
                let on = !autostart_enabled();
                if !set_autostart(on) {
                    tray_notify(hwnd, "HUDshell", "Could not change the Start with Windows setting");
                }
            }
            _ => {}
        }
        // Give the keyboard back to whatever had it, unless the user asked to grab it.
        if cmd.0 != 4 && cmd.0 != 5 && cmd.0 != 6 && !TERMINAL_FOCUS.load(Ordering::SeqCst) && prev != hwnd && prev != ahwnd
            && IsWindow(Some(prev)).as_bool()
        {
            let _ = SetForegroundWindow(prev);
        }
    }
}

// ---- several windows -----------------------------------------------------------------------------

/// The window the hotkeys and tray actions act on: the one that last had the keyboard, was clicked, or was made.
static ACTIVE_WINDOW: AtomicU32 = AtomicU32::new(0);

fn note_active_window() {
    ACTIVE_WINDOW.store(perwin::current(), Ordering::SeqCst);
}

/// (serial, handle) of the window the hotkeys act on; the first window when the active one is gone.
fn active_window(fallback: isize) -> (u32, isize) {
    let want = ACTIVE_WINDOW.load(Ordering::SeqCst);
    if let Some(h) = perwin::hwnd_of(want) {
        return (want, h);
    }
    perwin::all().first().copied().unwrap_or((perwin::current(), fallback))
}

/// The look every window shares: no DWM border line, rounded corners (Windows 11; `update_corner_style` squares them in a corner).
unsafe fn apply_window_look(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
    let round: i32 = DWMWCP_ROUND.0;
    let _ = unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &none as *const u32 as *const std::ffi::c_void, std::mem::size_of::<u32>() as u32)
    };
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
}

/// Open another window that starts with shell `id` (already running or about to be). It is placed next to the current one, takes over
/// its pointer mode, and gets its own web view. Returns its serial number.
unsafe fn create_window(id: u32, profile: &str, at: Option<[i32; 4]>) -> Option<u32> {
    if perwin::count() >= perwin::MAX_WINDOWS {
        return None;
    }
    let src = HWND(perwin::current_hwnd() as *mut std::ffi::c_void);
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(src, &mut r) };
    let off = unsafe { scaled(src, 36) };
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let work = unsafe { work_area(src) };
    let x = (r.left + off).min((work.r - w).max(work.l)).max(work.l);
    let y = (r.top + off).min((work.b - h).max(work.t)).max(work.t);
    // a restored window goes where it was
    let (x, y, w, h) = match at {
        Some([ax, ay, aw, ah]) => (ax, ay, aw, ah),
        None => (x, y, w, h),
    };
    let click_through = CLICK_THROUGH.load(Ordering::SeqCst);
    let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?.into();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            w!("HUDshell"),
            w!("HUDshell"),
            WS_POPUP | WS_THICKFRAME,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinstance),
            None,
        )
    }
    .ok()?;
    let serial = perwin::register_once(hwnd.0 as isize);
    let _w = perwin::enter(serial);
    let c = cfg();
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        apply_opacity(hwnd, c.opacity);
        apply_window_look(hwnd);
    }
    CLICK_THROUGH.store(click_through, Ordering::SeqCst);
    shells_lock().adopt(id, profile);
    {
        let mut l = layout_lock();
        l.set_own(c.pane_shells == "own");
        l.start(id);
        l.set_min(c.pane_min[0], c.pane_min[1]);
        l.set_split(c.split[0], c.split[1]);
    }
    LAYOUT_STARTED.store(true, Ordering::SeqCst); // the configured start layout belongs to the first window
    LAST_OUTPUT_MS.store(now_ms() as u64, Ordering::SeqCst);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        reassert_topmost(hwnd);
        refresh_pointer_style(hwnd);
        if c.idle_dim_seconds > 0 {
            let _ = SetTimer(Some(hwnd), 4, 1000, None);
        }
        if c.tab_program != "off" {
            let _ = SetTimer(Some(hwnd), 5, 2000, None);
        }
    }
    match unsafe { wv::init(hwnd, id) } {
        Ok(()) => {
            let _ = unsafe { SetTimer(Some(hwnd), 9, 2000, None) };
            log(&format!("window {serial} created (hwnd 0x{:X}) with shell {id}", hwnd.0 as usize))
        }
        Err(e) => {
            log(&format!("window {serial}: WebView2 init FAILED: {e}"));
            // The shell still belongs to the window it was coming from (a detach) or has not started (a restored window): take it off this
            // window's list first, or destroying the window would close it.
            shells_lock().remove(id);
            let _ = unsafe { DestroyWindow(hwnd) };
            return None;
        }
    }
    unsafe { update_corner_style(hwnd) };
    note_active_window();
    Some(serial)
}

#[derive(Clone, Copy, PartialEq)]
enum MoveKind {
    Detach,
    ToNext,
}

/// Moves waiting for the page's snapshot of the shell: (shell, kind, window it leaves, when asked).
static PENDING_MOVES: Mutex<Vec<(u32, MoveKind, u32, std::time::Instant)>> = Mutex::new(Vec::new());
/// The terminal text (screen and scrollback) a page handed over for a shell that is moving, until the shell's new page is ready.
static SNAPSHOTS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());
const SNAPSHOT_WAIT_MS: u64 = 1500;

/// A shell is about to move into another window: ask its page for a snapshot of the screen and scrollback first (the new page has no history
/// of it). The move itself happens when the snapshot arrives (`on_snapshot`), or after `SNAPSHOT_WAIT_MS` without it.
fn begin_move(id: u32, kind: MoveKind) {
    if id == 0 || PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner()).iter().any(|(i, ..)| *i == id) {
        return;
    }
    // a snapshot left from an earlier move that never finished must not stand in for this one
    SNAPSHOTS.lock().unwrap_or_else(|p| p.into_inner()).retain(|(i, _)| *i != id);
    PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner()).push((id, kind, perwin::current(), std::time::Instant::now()));
    wv::post(&serde_json::json!({ "t": "snapshot", "s": id }).to_string());
    let hwnd = perwin::current_hwnd();
    if hwnd != 0 {
        let _ = unsafe { SetTimer(Some(HWND(hwnd as *mut std::ffi::c_void)), 8, SNAPSHOT_WAIT_MS as u32, None) };
    }
}

/// The page handed over the snapshot of shell `id`: keep it and let the window that is waiting carry the move out.
pub fn on_snapshot(id: u32, data: &str) {
    let from = PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(i, ..)| *i == id).map(|(_, _, f, _)| *f);
    let Some(from) = from else { return };
    if !data.is_empty() {
        let mut s = SNAPSHOTS.lock().unwrap_or_else(|p| p.into_inner());
        s.retain(|(i, _)| *i != id);
        s.push((id, data.to_string()));
    }
    if let Some(h) = perwin::hwnd_of(from) {
        // posted: a window cannot be created from inside the page's message callback
        let _ = unsafe { PostMessageW(Some(HWND(h as *mut std::ffi::c_void)), WM_APP_MOVE_READY, WPARAM(id as usize), LPARAM(0)) };
    }
}

/// Carry out a move that was waiting for its snapshot (the current window is the one the shell leaves).
fn finish_move(id: u32) {
    let kind = {
        let mut p = PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner());
        let Some(i) = p.iter().position(|(s, ..)| *s == id) else { return };
        p.remove(i).1
    };
    match kind {
        MoveKind::Detach => detach_shell_now(id),
        MoveKind::ToNext => move_shell_now(id),
    }
    // a refused move (window limit, the shell went) leaves no new page to take the snapshot: drop it (it can be the whole scrollback)
    if !PENDING_REDRAW.lock().unwrap_or_else(|p| p.into_inner()).contains(&id) {
        SNAPSHOTS.lock().unwrap_or_else(|p| p.into_inner()).retain(|(i, _)| *i != id);
    }
}

/// A shell is gone with its window: forget what was kept for it.
fn forget_shell_state(id: u32) {
    SNAPSHOTS.lock().unwrap_or_else(|p| p.into_inner()).retain(|(i, _)| *i != id);
    PENDING_REDRAW.lock().unwrap_or_else(|p| p.into_inner()).retain(|i| *i != id);
    PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner()).retain(|(i, ..)| *i != id);
    if let Ok(mut l) = SPAWN_PROFILES.lock() {
        l.retain(|(i, _)| *i != id);
    }
}

/// Timer 8 of a window: moves whose snapshot never came go ahead without it.
fn finish_overdue_moves() {
    let me = perwin::current();
    let due: Vec<u32> = PENDING_MOVES
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|(_, _, from, at)| *from == me && at.elapsed().as_millis() as u64 >= SNAPSHOT_WAIT_MS - 100)
        .map(|(i, ..)| *i)
        .collect();
    for id in due {
        log(&format!("move of shell {id}: no snapshot came, going ahead without its scrollback"));
        finish_move(id);
    }
}

/// Move shell `id` of the current window into a window of its own (it keeps its screen and scrollback: see `begin_move`).
pub fn detach_shell(id: u32) {
    begin_move(id, MoveKind::Detach);
}

/// The detach itself. The shell keeps running; the new window starts with a blank view, which the snapshot fills, and the shell is asked to
/// repaint (see `pty::redraw`).
fn detach_shell_now(id: u32) {
    if shells_lock().len() <= 1 {
        log("detach refused: a window's only shell stays where it is");
        wv::post("{\"t\":\"flash\",\"text\":\"a window needs at least one shell\"}");
        return;
    }
    if perwin::count() >= perwin::MAX_WINDOWS {
        log(&format!("detach refused: at most {} windows", perwin::MAX_WINDOWS));
        wv::post(&serde_json::json!({ "t": "flash", "text": format!("at most {} windows", perwin::MAX_WINDOWS) }).to_string());
        return;
    }
    let Some(profile) = shells_lock().profile_of(id).map(|p| p.to_string()) else { return };
    let size = font_size_of(id);
    let had_own_size = FONT_SIZES.lock().map(|l| l.iter().any(|(i, _)| *i == id)).unwrap_or(false);
    let from = perwin::current();
    let Some(new_serial) = (unsafe { create_window(id, &profile, None) }) else {
        log("detach failed: the new window could not be created");
        return;
    };
    // the new window exists with the shell: now it leaves the old one
    {
        let _w = perwin::enter(from);
        remove_shell_from_window(id);
    }
    {
        let _w = perwin::enter(new_serial);
        if had_own_size
            && let Ok(mut l) = FONT_SIZES.lock() {
                l.push((id, size));
            }
        pty::set_active(id);
    }
    PENDING_REDRAW.lock().unwrap_or_else(|p| p.into_inner()).push(id);
    log(&format!("shell {id} detached into window {new_serial}"));
}

/// Shells moved into a new window whose page has not yet asked for them: they are redrawn once it does.
static PENDING_REDRAW: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// The page of a window made by detaching is ready: have the moved shell repaint its screen into the new view.
pub fn redraw_moved_shell(id: u32, cols: i64, rows: i64) {
    let mut p = PENDING_REDRAW.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(i) = p.iter().position(|s| *s == id) {
        p.remove(i);
        drop(p);
        // the scrollback and screen the old page handed over come first, then the shell repaints itself over the screen part
        let snap = {
            let mut s = SNAPSHOTS.lock().unwrap_or_else(|p| p.into_inner());
            s.iter().position(|(i, _)| *i == id).map(|k| s.remove(k).1)
        };
        if let Some(data) = snap {
            wv::post(&serde_json::json!({ "t": "restore", "s": id, "data": data }).to_string());
        }
        pty::redraw(id, cols, rows);
        log(&format!("shell {id}: asked to repaint in its new window ({cols}x{rows})"));
    }
}

fn detach_active_shell() {
    let id = shells_lock().active();
    if id != 0 {
        detach_shell(id);
    }
}

/// Move the active shell into the next window. It keeps running; the other window's page has no history of it, so it repaints (see
/// `pty::redraw`). When it is its window's only shell, that window closes with it.
fn move_active_shell_to_next_window() {
    let all = perwin::all();
    if all.len() < 2 {
        log("move refused: there is only one window");
        wv::post("{\"t\":\"flash\",\"text\":\"there is only one window\"}");
        return;
    }
    begin_move(shells_lock().active(), MoveKind::ToNext);
}

/// The move into the next window itself (after the snapshot, see `begin_move`).
fn move_shell_now(id: u32) {
    let all = perwin::all();
    if all.len() < 2 {
        return;
    }
    let from = perwin::current();
    let Some(i) = all.iter().position(|(s, _)| *s == from) else { return };
    let (to, _) = all[(i + 1) % all.len()];
    if id == 0 || shells_lock().position(id).is_none() {
        return;
    }
    {
        let _w = perwin::enter(to);
        if shells_lock().len() >= shells::MAX_SHELLS {
            log("move refused: the other window already holds the most shells");
            return;
        }
    }
    let Some(profile) = shells_lock().profile_of(id).map(|p| p.to_string()) else { return };
    let size = font_size_of(id);
    let had_own_size = FONT_SIZES.lock().map(|l| l.iter().any(|(i, _)| *i == id)).unwrap_or(false);
    let only = shells_lock().len() <= 1;
    if only {
        shells_lock().remove(id); // its window closes below; the shell is not closed with it
    } else {
        remove_shell_from_window(id);
    }
    {
        let _w = perwin::enter(to);
        shells_lock().adopt(id, &profile);
        shells_lock().activate(id);
        layout_lock().place_new(id);
        pty::set_active(id);
        if had_own_size
            && let Ok(mut l) = FONT_SIZES.lock() {
                l.push((id, size));
            }
        push_layout();
        wv::post(&serde_json::json!({ "t": "open", "s": id }).to_string());
        push_tabs();
        note_active_window();
    }
    PENDING_REDRAW.lock().unwrap_or_else(|p| p.into_inner()).push(id);
    log(&format!("shell {id} moved from window {from} to window {to}"));
    if only {
        let hwnd = perwin::hwnd_of(from).unwrap_or(0);
        if hwnd != 0 {
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut std::ffi::c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
    }
}

/// When the last rescue happened and how many windows it moved (see `keep_on_screen`).
static RESCUE_AT: AtomicU64 = AtomicU64::new(0);
static RESCUE_COUNT: AtomicU32 = AtomicU32::new(0);

/// After a display change (a monitor unplugged, the resolution or arrangement changed): a window that is no longer on any screen goes back
/// into the work area of the nearest monitor.
unsafe fn keep_on_screen(hwnd: HWND) {
    // a minimized window sits at about (-32000, -32000) on purpose; it is put back where it was when restored
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return;
    }
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return;
    }
    let g = config::Geometry { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
    if on_screen(&g) {
        return;
    }
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let work = unsafe { work_area(hwnd) };
    let margin = unsafe { scaled(hwnd, cfg().snap_margin) };
    // the scaling of the monitor it was on (the window still has it) and of the one it lands on: it keeps its logical size
    let old_dpi = unsafe { GetDpiForWindow(hwnd) } as i32;
    let (mut dx, mut dy) = (0u32, 0u32);
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let new_dpi = if unsafe { GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) }.is_ok() { dx as i32 } else { old_dpi };
    // windows rescued by the same display change sit along a diagonal instead of on top of each other
    let now = now_ms() as u64;
    let index = if now.saturating_sub(RESCUE_AT.swap(now, Ordering::SeqCst)) < 3000 { RESCUE_COUNT.fetch_add(1, Ordering::SeqCst) + 1 } else { RESCUE_COUNT.store(0, Ordering::SeqCst); 0 };
    let step = unsafe { scaled(hwnd, 36) };
    let r = place::rescue_rect((g.w, g.h), (old_dpi, new_dpi), work, margin, index as i32, step);
    let _ = unsafe { SetWindowPos(hwnd, None, r.l, r.t, r.w(), r.h(), SWP_NOZORDER | SWP_NOACTIVATE) };
    log(&format!("display changed: the window was off every screen; moved to {},{} ({}x{}, scaling {} -> {})", r.l, r.t, r.w(), r.h(), old_dpi, new_dpi));
    unsafe { save_geometry(hwnd) };
}

/// Make the next window the one the hotkeys act on, and say which it is.
fn next_window() {
    let all = perwin::all();
    if all.len() < 2 {
        return;
    }
    let me = ACTIVE_WINDOW.load(Ordering::SeqCst);
    let i = all.iter().position(|(s, _)| *s == me).unwrap_or(0);
    let (serial, hwnd) = all[(i + 1) % all.len()];
    ACTIVE_WINDOW.store(serial, Ordering::SeqCst);
    log(&format!("active window -> {serial} (of {})", all.len()));
    let _w = perwin::enter(serial);
    wv::post("{\"t\":\"cue\"}");
    unsafe { reassert_topmost(HWND(hwnd as *mut std::ffi::c_void)) };
}

/// Quit the program: the session is saved with every window still open, then all the windows go.
unsafe fn quit_all() {
    save_session();
    SESSION_LIVE.store(false, Ordering::SeqCst); // what is left while they close must not overwrite it
    for (_, h) in perwin::all() {
        let _ = unsafe { DestroyWindow(HWND(h as *mut std::ffi::c_void)) };
    }
}

/// A window was destroyed: forget it, hand the hotkeys, the tray icon and the wake target to the window that is first now, and end the
/// program when it was the last one. Returns true when the program ends.
unsafe fn window_gone(hwnd: HWND) -> bool {
    let was_first = perwin::all().first().map(|(_, h)| *h) == Some(hwnd.0 as isize);
    let ids = window_shell_ids();
    for id in ids {
        pty::close(id); // a window that goes takes its shells with it (a detached shell that was already moved is in its new window's list)
        forget_shell_state(id);
    }
    wv::forget_current();
    let gone_serial = perwin::current();
    // moves that were waiting for this window can never finish: drop them so those shells can move again
    PENDING_MOVES.lock().unwrap_or_else(|p| p.into_inner()).retain(|(_, _, from, _)| *from != gone_serial);
    perwin::unregister(hwnd.0 as isize);
    unsafe { remove_tray(hwnd) };
    unsafe { unregister_hotkeys(hwnd) };
    let rest = perwin::all();
    let Some((serial, next)) = rest.first().copied() else { return true };
    save_session(); // the window that went is no longer in the saved session (does nothing while quitting)
    if was_first {
        let next_hwnd = HWND(next as *mut std::ffi::c_void);
        let c = cfg();
        pty::set_wake_target(next);
        let _w = perwin::enter(serial);
        let registered = unsafe { register_hotkeys(next_hwnd, &c.hotkeys) };
        log(&format!("window {serial} is now the first window: {} hotkeys registered there", registered.len()));
        if !is_test_mode() {
            unsafe { add_tray(next_hwnd) };
            let _ = unsafe { SetTimer(Some(next_hwnd), 6, 1500, None) };
        }
        if c.game_positions {
            let _ = unsafe { SetTimer(Some(next_hwnd), 7, 1000, None) };
        }
        // a hotkey another program took in the meantime is reported, not lost silently
        unsafe { flush_notices(next_hwnd) };
    }
    // A wake posted to the window that went was discarded with it: clear the flag and drain what is waiting, or output would stop.
    pty::clear_wake();
    pump_all_windows();
    if ACTIVE_WINDOW.load(Ordering::SeqCst) == 0 || perwin::hwnd_of(ACTIVE_WINDOW.load(Ordering::SeqCst)).is_none() {
        ACTIVE_WINDOW.store(rest[0].0, Ordering::SeqCst);
    }
    false
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // A window belongs to the table from its first message: the resize and frame messages Windows sends while CreateWindowEx is still running must
    // not be handled as if they came for the window that happens to be the current one (that shrank the first window's web view by the new window's
    // frame, leaving black strips along two of its edges).
    if msg == WM_NCCREATE {
        perwin::register_once(hwnd.0 as isize);
    }
    let _window = perwin::enter_hwnd(hwnd.0 as isize);
    unsafe {
        match msg {
            m if m != 0 && m == ACTIVATE_MSG.load(Ordering::SeqCst) => {
                bring_forward(hwnd);
                LRESULT(0)
            }
            WM_HOTKEY => {
                // The hotkeys are registered on the first window; they act on the window that was last active.
                let (target, target_hwnd) = active_window(hwnd.0 as isize);
                let _target = perwin::enter(target);
                let hwnd = HWND(target_hwnd as *mut std::ffi::c_void);
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
                    HK_FONT_RESET => reset_font(),
                    HK_LAYOUT => layout_cycle(),
                    HK_PANE_ZOOM => pane_zoom(),
                    HK_PANE_NEXT => pane_step(1),
                    HK_SHELL_NEXT => shell_step(1),
                    HK_SHELL_PREV => shell_step(-1),
                    HK_SHELL_NEW => new_shell(None),
                    HK_SHELL_CLOSE => close_active_shell(),
                    HK_SHELL_LIST => wv::post("{\"t\":\"overlay\"}"),
                    HK_SHELL_PICKER => open_picker(hwnd),
                    HK_SHELL_ATTN => wv::post("{\"t\":\"attentionnext\"}"),
                    HK_WINDOW_DETACH => detach_active_shell(),
                    HK_WINDOW_NEXT => next_window(),
                    HK_SHELL_MOVE => move_active_shell_to_next_window(),
                    HK_GHOST_ALL => ghost_all(),
                    HK_SHOWHIDE_ALL => show_hide_all(),
                    n if (HK_SHELL_JUMP..HK_SHELL_JUMP + 9).contains(&n) => shell_jump((n - HK_SHELL_JUMP + 1) as usize),
                    HK_QUIT => {
                        log("quit requested");
                        quit_all();
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
                    // a panel opened by a grab no longer owns the keyboard: closing it later must not hand the keyboard back
                    SETTINGS_GRABBED.store(false, Ordering::SeqCst);
                    PICKER_GRABBED.store(false, Ordering::SeqCst);
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
            WM_DISPLAYCHANGE => {
                keep_on_screen(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DPICHANGED => {
                // The window was moved to a monitor with another scaling. Keep its size in logical (scaled) terms:
                // take the rectangle Windows suggests, so it neither shrinks nor grows relative to the text.
                let r = unsafe { &*(lparam.0 as *const RECT) };
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                log(&format!("dpi changed to {}: window now {}x{}", (wparam.0 & 0xFFFF) as u32, r.right - r.left, r.bottom - r.top));
                // a drag in progress carries on from the new size and scaling
                *lock_or_recover(&SNAP_DRAG) = None;
                if let Some(s) = lock_or_recover(&DRAG_START).as_mut() {
                    *s = *r;
                }
                LRESULT(0)
            }
            WM_NCLBUTTONDOWN => {
                note_active_window();
                DRAG_IS_MOVE.store(wparam.0 as u32 == HTCAPTION, Ordering::SeqCst);
                log(&format!("WM_NCLBUTTONDOWN hit-test={} passive={}", wparam.0, !TERMINAL_FOCUS.load(Ordering::SeqCst)));
                foreground_note("at non-client click");
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SIZE => {
                wv::resize(hwnd);
                update_corner_style(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOVE => {
                update_corner_style(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            pty::WM_APP_PTY => {
                pty::clear_wake();
                // The shell produced output: undo idle dimming even when the page is still busy with earlier output.
                note_output(hwnd);
                pump_all_windows();
                LRESULT(0)
            }
            wv::WM_APP_WEBVIEW_FAILED => {
                if wv::on_process_failed(hwnd, wparam.0 as i32) {
                    WV_RECOVERING.store(true, Ordering::SeqCst);
                }
                LRESULT(0)
            }
            WM_APP_RESTORE_WINDOWS => {
                let list = std::mem::take(&mut *lock_or_recover(&PENDING_WINDOW_SESSIONS));
                for s in list {
                    unsafe { restore_extra_window(s) };
                }
                LRESULT(0)
            }
            WM_APP_DETACH => {
                detach_shell(wparam.0 as u32);
                LRESULT(0)
            }
            WM_APP_MOVE_READY => {
                finish_move(wparam.0 as u32);
                LRESULT(0)
            }
            pty::WM_APP_PTY_EXIT => {
                pump_all_windows();
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
                } else if wparam.0 == 13 {
                    check_exclusive_after_grab(hwnd);
                } else if wparam.0 == 9 {
                    // every 2 s: the web view must cover the window (see wv::ensure_bounds)
                    wv::ensure_bounds(hwnd);
                } else if wparam.0 == 7 {
                    poll_game_places();
                } else if wparam.0 == 8 {
                    let _ = KillTimer(Some(hwnd), 8);
                    finish_overdue_moves();
                } else if wparam.0 == 5 {
                    poll_programs();
                } else if wparam.0 == 6 {
                    poll_config(hwnd);
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
                let mut wr = RECT::default();
                let _ = GetWindowRect(hwnd, &mut wr);
                *lock_or_recover(&SNAP_DRAG) = Some(place::SnapDrag::new(to_rect(&wr)));
                *lock_or_recover(&DRAG_START) = Some(wr);
                *lock_or_recover(&RESTORE_ANCHOR) = None;
                DRAG_MOVED.store(false, Ordering::SeqCst);
                begin_group_drag(hwnd, &wr);
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
                *lock_or_recover(&SNAP_DRAG) = None;
                lock_or_recover(&DRAG_OTHERS).clear();
                if DRAG_IS_MOVE.swap(false, Ordering::SeqCst) && DRAG_MOVED.load(Ordering::SeqCst) {
                    after_move_drag(hwnd);
                }
                lock_or_recover(&DRAG_GROUP).clear();
                *lock_or_recover(&DRAG_START) = None;
                *lock_or_recover(&RESTORE_ANCHOR) = None;
                save_geometry(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => LRESULT(1),
            m if m != 0 && m == TASKBAR_MSG.load(Ordering::SeqCst) => {
                // Explorer restarted: its tray is empty, so put our icon back. Every top-level window gets this message; the icon belongs
                // to the first window only.
                let is_first = perwin::all().first().map(|(_, h)| *h) == Some(hwnd.0 as isize);
                if is_first && PERSIST.load(Ordering::SeqCst) {
                    log("TaskbarCreated received: re-adding the tray icon");
                    add_tray(hwnd);
                }
                LRESULT(0)
            }
            WM_NCDESTROY => {
                // a window that was registered at its first message but never finished being created (or is gone now) leaves the table
                perwin::unregister(hwnd.0 as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                if perwin::count() <= 1 && SESSION_LIVE.load(Ordering::SeqCst) {
                    save_session();
                }
                save_geometry(hwnd);
                if window_gone(hwnd) {
                    pty::shutdown();
                    PostQuitMessage(0);
                }
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
        let last = windows::Win32::Foundation::GetLastError();
        // ALREADY_EXISTS: another copy runs. ACCESS_DENIED: the name exists but belongs to someone else (it cannot be opened), so it is not
        // safe to run a second copy that would fight over the hotkeys either.
        let taken = match &_guard {
            Ok(_) => last == windows::Win32::Foundation::ERROR_ALREADY_EXISTS,
            Err(e) => e.code() == windows::Win32::Foundation::ERROR_ACCESS_DENIED.to_hresult(),
        };
        if taken {
            log("ANOTHER INSTANCE IS ALREADY RUNNING - asking it to show itself, then exiting.");
            // the running copy may still be creating its window: look for it for a moment
            let mut asked = false;
            for _ in 0..20 {
                if let Ok(other) = FindWindowW(w!("HUDshell"), PCWSTR::null()) {
                    asked = PostMessageW(Some(other), ACTIVATE_MSG.load(Ordering::SeqCst), WPARAM(0), LPARAM(0)).is_ok();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if !asked {
                log("the running copy has no window to show (yet); exiting");
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
        perwin::register_once(hwnd.0 as isize);
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

        let registered = register_hotkeys(hwnd, &loaded.hotkeys);
        notice_altgr_clashes(&registered);
        {
            let c0 = cfg();
            let saved = load_session();
            let first = saved.as_ref().map(|s| profile_named(&s.shells[0])).unwrap_or_else(|| config::default_profile(&c0).clone());
            *lock_or_recover(&SESSION_PENDING) = saved;
            shells_lock().adopt(1, &first.name);
            let mut l = layout_lock();
            FONT_START.store(c0.font_size, Ordering::SeqCst);
            l.set_own(c0.pane_shells == "own");
            l.start(1);
            l.set_min(c0.pane_min[0], c0.pane_min[1]);
            l.set_split(c0.split[0], c0.split[1]);
            if let Ok(mut l) = SPAWN_PROFILES.lock() {
                l.push((1, first.clone()));
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
                "textScale": loaded.text_scale,
                "uiScale": loaded.ui_scale,
                "lineHeight": loaded.line_height as f64 / 100.0,
                "letterSpacing": loaded.letter_spacing,
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
            pty::set_wake_target(hwnd.0 as isize);
            match wv::init(hwnd, 1) {
                Ok(()) => {
                    log("WebView2 ready");
                    let _ = SetTimer(Some(hwnd), 3, 1500, None);
                    let _ = SetTimer(Some(hwnd), 9, 2000, None);
                    if loaded.tab_program != "off" {
                        let _ = SetTimer(Some(hwnd), 5, 2000, None);
                    }
                    if PERSIST.load(Ordering::SeqCst) {
                        let _ = SetTimer(Some(hwnd), 6, 1500, None); // watch config.json for edits
                    }
                    if loaded.game_positions {
                        let _ = SetTimer(Some(hwnd), 7, 1000, None);
                    }
                    #[cfg(feature = "harness")]
                    harness::arm(hwnd);
                }
                Err(e) => {
                    log(&format!("WebView2 init FAILED: {e}"));
                    tray_notify(
                        hwnd,
                        "HUDshell",
                        "The terminal view could not be created, so no shell can run. Install or repair the Microsoft Edge WebView2 Runtime, then start HUDshell again (quit from the tray menu).",
                    );
                }
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
