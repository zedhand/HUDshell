//! HUDshell: a compact always-on-top PowerShell 7 terminal for Windows 11 (Win32 window +
//! WebView2/xterm.js renderer + ConPTY host).
//!
//! Global hotkeys (RegisterHotKey only - no hooks, no injection; all configurable in config.json):
//!   Ctrl+Alt+T  toggle click-through      Ctrl+Alt+G  grab / release keyboard focus
//!   Ctrl+Alt+V  show / hide               Ctrl+Alt+C  next colour theme
//!   Ctrl+Alt+Q  quit
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
mod pty;
mod wv;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
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

unsafe fn toggle_click_through(hwnd: HWND) {
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
    let pct = pct.clamp(config::MIN_OPACITY, 100);
    unsafe { apply_opacity(hwnd, pct) };
    if save {
        cfg_update(|c| c.opacity = pct);
        log(&format!("opacity -> {pct}%"));
    } else if let Ok(mut g) = CFG.lock() {
        if let Some(c) = g.as_mut() {
            c.opacity = pct; // keep memory current while dragging; written on release
        }
    }
}

unsafe fn step_opacity(hwnd: HWND, delta: i32) {
    let cur = cfg().opacity as i32;
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
            "t": "settings", "open": open, "opacity": c.opacity, "minOpacity": config::MIN_OPACITY,
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

/// Show a balloon/toast from the tray icon (hotkey conflicts, bad config).
unsafe fn tray_notify(hwnd: HWND, title: &str, text: &str) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_INFO, NIIF_WARNING, NIM_MODIFY, NOTIFYICONDATAW};
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_WARNING;
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
        let _ = AppendMenuW(menu, MF_STRING, 6, w!("Settings (opacity, colours)..."));
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
                wv::pump_output();
                LRESULT(0)
            }
            pty::WM_APP_PTY_EXIT => {
                wv::pump_output();
                wv::notify_exit();
                LRESULT(0)
            }
            #[cfg(feature = "harness")]
            WM_TIMER if harness::handles_timer(wparam.0) => {
                harness::on_timer(hwnd, wparam.0);
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == 3 {
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

fn main() -> windows::core::Result<()> {
    unsafe {
        // Single instance: a stale copy would silently hold the hotkeys.
        let _guard = windows::Win32::System::Threading::CreateMutexW(
            None,
            true,
            w!("Local\\HUDshell"),
        );
        if windows::Win32::Foundation::GetLastError()
            == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
        {
            log("ANOTHER INSTANCE IS ALREADY RUNNING - exiting (quit it from its tray icon or its quit hotkey first).");
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Settings: the real config file in normal runs; fixed defaults (never saved) in test modes.
        let test_mode = is_test_mode();
        PERSIST.store(!test_mode, Ordering::SeqCst);
        let loaded = if test_mode { config::Config::default() } else { config::load() };
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
        TASKBAR_MSG.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::SeqCst);
        if !test_mode {
            add_tray(hwnd);
            let msgs: Vec<String> = NOTICES.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default();
            if !msgs.is_empty() {
                tray_notify(hwnd, "HUDshell", &msgs.join("\n"));
            }
        }

        log("start: showing window without activation");
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        reassert_topmost(hwnd);
        foreground_note("after initial show");
        if std::env::args().any(|a| a == "--interactive") {
            CLICK_THROUGH.store(false, Ordering::SeqCst);
        }
        refresh_pointer_style(hwnd);

        wv::set_options(&loaded.unicode, &loaded.shift_enter);
        wv::set_config_json(
            &serde_json::json!({
                "font": loaded.font_family,
                "fontSize": loaded.font_size,
                "scrollback": loaded.scrollback,
                "theme": theme_json(&loaded),
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
