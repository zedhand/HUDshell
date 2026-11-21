//! WebView2 hosted as an ordinary (windowed) child of the overlay HWND.
//! xterm.js page served from memory under a private host name, bridged to the ConPTY host with
//! byte-safe base64 JSON messages.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc;

use base64::Engine;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Settings3, ICoreWebView2Settings5, ICoreWebView2Settings6,
    COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC, COREWEBVIEW2_PERMISSION_KIND,
    COREWEBVIEW2_PERMISSION_STATE_DENY, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
};
use webview2_com::{
    take_pwstr, AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler,
    FocusChangedEventHandler, NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, ProcessFailedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::core::{Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{E_POINTER, HWND, LPARAM, RECT};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetClientRect, GetWindowLongPtrW, PostMessageW, GWL_EXSTYLE,
};
use windows::Win32::Foundation::WPARAM;

/// Posted to the main window when a WebView2 process failed (wparam = the kind of process, see `on_process_failed`).
pub const WM_APP_WEBVIEW_FAILED: u32 = 0x8000 + 9;

use crate::{log, pty};

const HOST_ORIGIN: &str = "https://hudshell.local/";
/// Max unacknowledged bytes in flight to the page (backpressure toward the pty reader).
const MAX_INFLIGHT: usize = 256 * 1024;

static OPT_UNICODE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static OPT_SHIFT_ENTER: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

static OPT_CONFIG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// JSON object (font, fontSize, scrollback, theme) merged into `window.__hudshell` before load.
pub fn set_config_json(s: &str) {
    if let Ok(mut c) = OPT_CONFIG.lock() {
        *c = s.to_string();
    }
}

/// Front-end options injected into the page before it loads.
pub fn set_options(unicode: &str, shift_enter: &str) {
    if let Ok(mut u) = OPT_UNICODE.lock() {
        *u = unicode.to_string();
    }
    if let Ok(mut s) = OPT_SHIFT_ENTER.lock() {
        *s = shift_enter.to_string();
    }
}

/// Text of the most recent screen dump (used by the scripted test's skip-if checks).
#[cfg(feature = "harness")]
static LAST_DUMP: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

#[cfg(feature = "harness")]
pub fn last_dump() -> String {
    LAST_DUMP.lock().map(|s| s.clone()).unwrap_or_default()
}

// One web view per window, keyed by the window's serial number (see perwin.rs); the functions below act on the current window's.
thread_local! {
    static CONTROLLER: RefCell<HashMap<u32, ICoreWebView2Controller>> = RefCell::new(HashMap::new());
    static WEBVIEW: RefCell<HashMap<u32, ICoreWebView2>> = RefCell::new(HashMap::new());
    static INFLIGHT: RefCell<HashMap<u32, usize>> = RefCell::new(HashMap::new());
}

fn webview() -> Option<ICoreWebView2> {
    let me = crate::perwin::current();
    WEBVIEW.with(|w| w.borrow().get(&me).cloned())
}

fn controller() -> Option<ICoreWebView2Controller> {
    let me = crate::perwin::current();
    CONTROLLER.with(|c| c.borrow().get(&me).cloned())
}

fn inflight() -> usize {
    let me = crate::perwin::current();
    INFLIGHT.with(|i| i.borrow().get(&me).copied().unwrap_or(0))
}

fn set_inflight(n: usize) {
    let me = crate::perwin::current();
    INFLIGHT.with(|i| {
        i.borrow_mut().insert(me, n);
    });
}

fn windows_build() -> u32 {
    let mut vi = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    let _ = unsafe { RtlGetVersion(&mut vi) };
    if vi.dwBuildNumber == 0 { 26100 } else { vi.dwBuildNumber }
}

/// Create the web view of the current window. `first` is the id of the shell the page starts with (shell 1 in the first window).
pub unsafe fn init(hwnd: HWND, first: pty::SessionId) -> Result<(), String> {
    let data_dir =
        format!("{}\\HUDshell\\WebView2", std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()));
    let data_dir_w = HSTRING::from(data_dir);

    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &data_dir_w, None, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error_code, environment| {
            error_code?;
            tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                .expect("send env");
            Ok(())
        }),
    )
    .map_err(|e| format!("environment: {e:?}"))?;
    let env = rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel();
    let env2 = env.clone();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            env2.CreateCoreWebView2Controller(hwnd, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error_code, controller| {
            error_code?;
            tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                .expect("send controller");
            Ok(())
        }),
    )
    .map_err(|e| format!("controller: {e:?}"))?;
    let controller = rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    let hwnd_i = hwnd.0 as isize;
    let webview = unsafe { controller.CoreWebView2() }.map_err(|e| e.to_string())?;

    // This is a terminal, not a browser: no DevTools window, browser shortcuts (Ctrl+Shift+C/I,
    // F12, Ctrl+P, F5, zoom...), right-click menu, zoom, status bar or swipe navigation. Ctrl+C/V
    // editing keys are not affected by AreBrowserAcceleratorKeysEnabled.
    unsafe {
        if let Ok(s) = webview.Settings() {
            let _ = s.SetAreDevToolsEnabled(false);
            let _ = s.SetAreDefaultContextMenusEnabled(false);
            let _ = s.SetIsZoomControlEnabled(false);
            let _ = s.SetIsStatusBarEnabled(false);
            if let Ok(s3) = s.cast::<ICoreWebView2Settings3>() {
                let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
            if let Ok(s5) = s.cast::<ICoreWebView2Settings5>() {
                let _ = s5.SetIsPinchZoomEnabled(false);
            }
            if let Ok(s6) = s.cast::<ICoreWebView2Settings6>() {
                let _ = s6.SetIsSwipeNavigationEnabled(false);
            }
            log("webview: DevTools, browser shortcuts, context menu, zoom and swipe disabled");
        }
    }
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut rc) };
    unsafe {
        controller.SetBounds(rc).map_err(|e| e.to_string())?;
        controller.SetIsVisible(true).map_err(|e| e.to_string())?;
    }

    let mut token = Default::default();
    unsafe {
        controller
            .add_GotFocus(
                &FocusChangedEventHandler::create(Box::new(|_, _| {
                    log("webview: controller GotFocus");
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;
        controller
            .add_LostFocus(
                &FocusChangedEventHandler::create(Box::new(|_, _| {
                    log("webview: controller LostFocus");
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;

        // A WebView2 process died. Only post a message here: rebuilding blocks and must not run inside this callback.
        webview
            .add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let mut kind = Default::default();
                    if let Some(a) = args {
                        let _ = a.ProcessFailedKind(&mut kind);
                    }
                    log(&format!("webview: a process failed (kind {})", kind.0));
                    let _ = PostMessageW(
                        Some(HWND(hwnd_i as *mut std::ffi::c_void)),
                        WM_APP_WEBVIEW_FAILED,
                        WPARAM(kind.0 as usize),
                        LPARAM(0),
                    );
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;

        // Messages from the page; the source origin is checked first.
        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut src = PWSTR::null();
                    let ok = unsafe { args.Source(&mut src) }.is_ok()
                        && take_pwstr(src).starts_with(HOST_ORIGIN);
                    if !ok {
                        log("bridge: message from unexpected origin dropped");
                        return Ok(());
                    }
                    let mut msg = PWSTR::null();
                    if unsafe { args.TryGetWebMessageAsString(&mut msg) }.is_err() {
                        return Ok(());
                    }
                    let _window = crate::perwin::enter_hwnd(hwnd_i);
                    handle_term_message(hwnd_i, &take_pwstr(msg));
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;
    }

    {
        unsafe {
            // Never let terminal content navigate the host somewhere else or open windows.
            webview
                .add_NavigationStarting(
                    &NavigationStartingEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            let mut uri = PWSTR::null();
                            if unsafe { args.Uri(&mut uri) }.is_ok() {
                                let u = take_pwstr(uri);
                                if !u.starts_with(HOST_ORIGIN) {
                                    let _ = unsafe { args.SetCancel(true) };
                                    log(&format!("bridge: navigation blocked: {u}"));
                                }
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())?;
            webview
                .add_NewWindowRequested(
                    &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            let _ = unsafe { args.SetHandled(true) };
                            log("bridge: new-window request blocked");
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())?;

            // The page never needs a browser permission (the host does clipboard reads and writes), so
            // every request is denied without showing WebView2's own prompt over the user's game.
            webview
                .add_PermissionRequested(
                    &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                            let _ = unsafe { args.PermissionKind(&mut kind) };
                            let _ = unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
                            log(&format!("bridge: browser permission request denied (kind {})", kind.0));
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())?;

            // Serve the UI that is compiled into this executable, from memory, for the private
            // host name. Unknown paths get a 404; nothing is read from disk or the network.
            let env_res = env.clone();
            webview
                .AddWebResourceRequestedFilter(
                    &HSTRING::from("https://hudshell.local/*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )
                .map_err(|e| e.to_string())?;
            webview
                .add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let request = unsafe { args.Request()? };
                        let mut uri = PWSTR::null();
                        unsafe { request.Uri(&mut uri)? };
                        let u = take_pwstr(uri);
                        let path = u
                            .strip_prefix("https://hudshell.local")
                            .unwrap_or("/")
                            .split(['?', '#'])
                            .next()
                            .unwrap_or("/");
                        let path = if path.is_empty() || path == "/" { "/index.html" } else { path };
                        let (status, reason, body, mime): (i32, &str, &[u8], &str) =
                            match crate::assets::find(path) {
                                Some(a) => (200, "OK", a.data, a.mime),
                                None => (404, "Not Found", b"not found", "text/plain"),
                            };
                        log(&format!("asset served from memory: {path} -> {status} ({} bytes)", body.len()));
                        let stream = unsafe { SHCreateMemStream(Some(body)) };
                        let headers = format!(
                            "Content-Type: {mime}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff"
                        );
                        let resp = unsafe {
                            env_res.CreateWebResourceResponse(
                                stream.as_ref(),
                                status,
                                &HSTRING::from(reason),
                                &HSTRING::from(headers),
                            )?
                        };
                        unsafe { args.SetResponse(&resp)? };
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())?;

            // Tell xterm.js the real Windows build so its ConPTY reflow handling is right.
            let build = windows_build();
            log(&format!("terminal: windows build {build}"));
            let uni = OPT_UNICODE.lock().map(|s| s.clone()).unwrap_or_default();
            let uni = if uni.is_empty() { "11".to_string() } else { uni };
            let se = OPT_SHIFT_ENTER.lock().map(|s| s.clone()).unwrap_or_default();
            let se = if se.is_empty() { "kitty".to_string() } else { se };
            log(&format!("terminal: options unicode={uni} shiftEnter={se}"));
            let extra = OPT_CONFIG.lock().map(|s| s.clone()).unwrap_or_default();
            let extra = if extra.is_empty() { "{}".to_string() } else { extra };
            let script = HSTRING::from(format!(
                "window.__hudshell=Object.assign({{build:{build},unicode:\"{uni}\",shiftEnter:\"{se}\",first:{first}}},{extra});"
            ));
            webview
                .AddScriptToExecuteOnDocumentCreated(
                    &script,
                    &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
                        |_, _| Ok(()),
                    )),
                )
                .map_err(|e| e.to_string())?;
            webview
                .Navigate(&HSTRING::from(format!("{HOST_ORIGIN}index.html")))
                .map_err(|e| e.to_string())?;
        }
    }

    let me = crate::perwin::current();
    WEBVIEW.with(|w| w.borrow_mut().insert(me, webview));
    CONTROLLER.with(|c| c.borrow_mut().insert(me, controller));
    Ok(())
}

fn handle_term_message(hwnd: isize, m: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(m) else { return };
    match v["t"].as_str() {
        // "ready": a view was created and sized (start its shell). "restart": the user asked for a new
        // shell in a tab whose shell had exited.
        Some(kind @ ("ready" | "restart")) => {
            let id = session_of(&v);
            let cols = v["cols"].as_i64().unwrap_or(100);
            let rows = v["rows"].as_i64().unwrap_or(30);
            if pty::has_session(id) {
                // a shell that moved into this window from another one: the new view is blank, so it repaints
                crate::redraw_moved_shell(id, cols, rows);
            }
            if !pty::has_session(id) {
                log(&format!("bridge: session {id} {kind} at {cols}x{rows}, starting shell"));
                match pty::spawn(hwnd, id, cols, rows, &crate::spawn_opts_for(id)) {
                    Ok(path) => log(&format!("pty: started {path} for session {id} at {cols}x{rows}")),
                    Err(e) => {
                        log(&format!("pty: FAILED to start session {id}: {e}"));
                        // say so in the tab (Enter there tries again), instead of a silent empty window
                        post(&serde_json::json!({ "t": "startfail", "s": id, "msg": e }).to_string());
                    }
                }
            }
        }
        Some("switch") => crate::switch_to(session_of(&v)),
        // Shift+click on a tab: move that shell into a window of its own.
        // (posted: a window cannot be created from inside this callback)
        Some("detach") => {
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut std::ffi::c_void)), crate::WM_APP_DETACH, WPARAM(session_of(&v) as usize), LPARAM(0)) };
        }
        Some("newshell") => {
            crate::new_shell(v["profile"].as_str());
            crate::picker_closed(hwnd);
        }
        Some("pickerclose") => crate::picker_closed(hwnd),
        Some("tabsneeded") => {
            crate::reopen_shells_after_recovery();
            crate::layout_page_ready();
            crate::push_tabs();
        }
        Some("area") => {
            let cols = v["cols"].as_u64().unwrap_or(0);
            let rows = v["rows"].as_u64().unwrap_or(0);
            if (1..=2000).contains(&cols) && (1..=1000).contains(&rows) {
                crate::on_area(cols as u32, rows as u32);
            }
        }
        Some("split") => {
            let c = v["c"].as_u64().unwrap_or(50).min(100) as u32;
            let r = v["r"].as_u64().unwrap_or(50).min(100) as u32;
            crate::on_split(c, r);
        }
        Some("panefocus") => crate::on_pane_focus(v["i"].as_u64().unwrap_or(99) as usize),
        Some("in") => {
            if let Some(d) = v["d"].as_str() {
                if d.len() <= 2_000_000 {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(d) {
                        pty::write_to(session_of(&v), bytes);
                    }
                }
            }
        }
        Some("click") => crate::on_page_click(hwnd),
        Some("nc") => {
            // A press on the drag strip or an edge: only these hit-test codes can be asked for.
            let ht = match v["hit"].as_str() {
                Some("caption") => 2,
                Some("left") => 10,
                Some("right") => 11,
                Some("top") => 12,
                Some("topleft") => 13,
                Some("topright") => 14,
                Some("bottom") => 15,
                Some("bottomleft") => 16,
                Some("bottomright") => 17,
                Some("toggle") => 99, // double click on the top strip: maximize / restore
                _ => 0,
            };
            if ht == 99 {
                crate::toggle_maximize(hwnd);
            } else if ht != 0 {
                crate::start_native_drag(hwnd, ht);
            }
        }
        Some("settings") => crate::on_settings_message(hwnd, &v),
        Some("applied") => log(&format!("terminal applied settings: {v}")),
        Some("copied") => log(&format!("copy to clipboard: {v}")),
        Some("rightpaste") => crate::on_right_paste(),
        Some("attention") => crate::on_attention(hwnd, v["kind"].as_str().unwrap_or("?"), v["text"].as_str().unwrap_or("")),
        Some("osc52") => crate::on_osc52(v["b64"].as_str().unwrap_or("")),
        Some("note") => log(&format!("page: {v}")),
        Some("resize") => {
            let cols = v["cols"].as_i64().unwrap_or(0);
            let rows = v["rows"].as_i64().unwrap_or(0);
            if (1..=500).contains(&cols) && (1..=300).contains(&rows) {
                pty::resize(session_of(&v), cols, rows);
            }
        }
        Some("ack") => {
            let n = v["n"].as_u64().unwrap_or(0) as usize;
            set_inflight(inflight().saturating_sub(n));
            pump_output();
        }
        #[cfg(feature = "harness")]
        Some("dump") => {
            log(&format!(
                "terminal dump: {}x{} buffer={} cursor={} unicode={} view={} cue={} fs={} modes={} s={} ui={}",
                v["cols"], v["rows"], v["buf"], v["cursor"], v["unicode"], v["view"], v["cue"], v["fs"], v["modes"], v["s"], v["ui"]
            ));
            if let Some(lines) = v["lines"].as_array() {
                if let Ok(mut d) = LAST_DUMP.lock() {
                    *d = lines
                        .iter()
                        .map(|l| l.as_str().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                let mut last = 0;
                for (i, l) in lines.iter().enumerate() {
                    if l.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false) {
                        last = i + 1;
                    }
                }
                for l in lines.iter().take(last) {
                    log(&format!("  | {}", l.as_str().unwrap_or("")));
                }
            }
        }
        _ => {}
    }
}

pub fn post(json: &str) {
    if let Some(w) = webview() {
        let _ = unsafe { w.PostWebMessageAsString(&HSTRING::from(json)) };
    }
}

/// A WebView2 process failed (message `WM_APP_WEBVIEW_FAILED`, `kind` = COREWEBVIEW2_PROCESS_FAILED_KIND). The shells keep running; only the page
/// needs a new life. A render process (1), an unresponsive one (2) or a frame's (3) is cured by loading the page again; when the browser process
/// itself (0) is gone the whole web view is created again. Returns true when a new page is on its way and must be told about the open shells.
pub unsafe fn on_process_failed(hwnd: HWND, kind: i32) -> bool {
    match kind {
        1..=3 => {
            log("webview: reloading the page after a render process failure");
            set_inflight(0);
            if let Some(w) = webview() {
                let _ = unsafe { w.Reload() };
            }
            true
        }
        0 => {
            log("webview: the browser process exited; creating the web view again");
            let me = crate::perwin::current();
            WEBVIEW.with(|w| w.borrow_mut().remove(&me));
            CONTROLLER.with(|c| c.borrow_mut().remove(&me));
            set_inflight(0); // the old page will never acknowledge what was in flight
            let first = crate::first_shell_of_window();
            match unsafe { init(hwnd, first) } {
                Ok(()) => {
                    log("webview: created again");
                    true
                }
                Err(e) => {
                    log(&format!("webview: could not create it again: {e}"));
                    false
                }
            }
        }
        _ => {
            log("webview: the failed process needs no action");
            false
        }
    }
}

/// Forward buffered pty output of this window's shells to its page while the in-flight byte budget allows.
/// Returns how many bytes were sent.
pub fn pump_output() -> usize {
    let mine = crate::window_shell_ids();
    let mut sent = 0;
    loop {
        if inflight() >= MAX_INFLIGHT {
            break;
        }
        let Some((id, chunk)) = pty::take_chunk_for(&mine) else { break };
        set_inflight(inflight() + chunk.len());
        sent += chunk.len();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&chunk);
        post(&format!("{{\"t\":\"out\",\"s\":{id},\"d\":\"{b64}\"}}"));
    }
    sent
}

// ---- Test-only input path: Chrome DevTools Protocol key events (no OS focus needed). ----
// These produce real browser keydown/keypress/input events inside the page, so xterm.js's own
// key handling and encoding are exercised, without sending any OS-level input to other apps.

#[cfg(feature = "harness")]
pub fn cdp(method: &str, params: &str) {
    if let Some(w) = webview() {
        let h = webview2_com::CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
        let _ = unsafe { w.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params), &h) };
    }
}

#[cfg(feature = "harness")]
fn key_event(kind: &str, key: &str, code: &str, vk: u32, mods: u32, text: &str) {
    let mut o = serde_json::json!({
        "type": kind, "key": key, "code": code,
        "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk, "modifiers": mods,
    });
    if !text.is_empty() && kind == "keyDown" {
        o["text"] = serde_json::Value::String(text.to_string());
    }
    cdp("Input.dispatchKeyEvent", &o.to_string());
}

/// "Enter", "Shift+Enter", "Ctrl+C", "Backspace", "Up", "Esc", "Tab", ...
#[cfg(feature = "harness")]
pub fn send_key(spec: &str) {
    let mut mods = 0u32;
    let mut name = "";
    for part in spec.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "alt" => mods |= 1,
            "ctrl" | "control" => mods |= 2,
            "meta" | "win" => mods |= 4,
            "shift" => mods |= 8,
            _ => name = part,
        }
    }
    let (key, code, vk, text): (String, String, u32, &str) = match name.to_ascii_lowercase().as_str()
    {
        "enter" => ("Enter".into(), "Enter".into(), 13, "\r"),
        "backspace" => ("Backspace".into(), "Backspace".into(), 8, ""),
        "tab" => ("Tab".into(), "Tab".into(), 9, ""),
        "esc" | "escape" => ("Escape".into(), "Escape".into(), 27, ""),
        "home" => ("Home".into(), "Home".into(), 36, ""),
        "end" => ("End".into(), "End".into(), 35, ""),
        "delete" => ("Delete".into(), "Delete".into(), 46, ""),
        "pageup" => ("PageUp".into(), "PageUp".into(), 33, ""),
        "pagedown" => ("PageDown".into(), "PageDown".into(), 34, ""),
        "up" => ("ArrowUp".into(), "ArrowUp".into(), 38, ""),
        "down" => ("ArrowDown".into(), "ArrowDown".into(), 40, ""),
        "left" => ("ArrowLeft".into(), "ArrowLeft".into(), 37, ""),
        "right" => ("ArrowRight".into(), "ArrowRight".into(), 39, ""),
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() => {
            (n.to_string(), format!("Digit{n}"), n.as_bytes()[0] as u32, "")
        }
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_alphabetic() => {
            let u = n.to_ascii_uppercase();
            (n.to_string(), format!("Key{u}"), u.as_bytes()[0] as u32, "")
        }
        _ => return,
    };
    let printable_text = if mods & 0b11 == 0 { text } else { "" };
    let kind = if printable_text.is_empty() { "rawKeyDown" } else { "keyDown" };
    key_event(kind, &key, &code, vk, mods, printable_text);
    key_event("keyUp", &key, &code, vk, mods, "");
}

#[cfg(feature = "harness")]
pub fn type_text(s: &str) {
    for c in s.chars() {
        let (key, code, vk, mods): (String, String, u32, u32) = if c.is_ascii_alphabetic() {
            let u = c.to_ascii_uppercase();
            (c.to_string(), format!("Key{u}"), u as u32, if c.is_ascii_uppercase() { 8 } else { 0 })
        } else if c.is_ascii_digit() {
            (c.to_string(), format!("Digit{c}"), c as u32, 0)
        } else if c == ' ' {
            (" ".into(), "Space".into(), 32, 0)
        } else {
            (c.to_string(), String::new(), 0, 0)
        };
        let text = c.to_string();
        key_event("keyDown", &key, &code, vk, mods, &text);
        key_event("keyUp", &key, &code, vk, mods, "");
    }
}

/// Goes through xterm.js's own paste handling (bracketed paste, newline normalization).
#[cfg(feature = "harness")]
pub fn paste_text(s: &str) {
    post(&serde_json::json!({ "t": "paste", "text": s }).to_string());
}

/// Tell the page that shows shell `id` (whichever window that is) that its shell ended.
pub fn notify_exit(id: pty::SessionId) {
    crate::in_window_of(id, || post(&format!("{{\"t\":\"exit\",\"s\":{id}}}")));
}

/// Session a page message belongs to (the page puts it in `s`; messages without one mean session 1).
fn session_of(v: &serde_json::Value) -> pty::SessionId {
    v["s"].as_u64().unwrap_or(1) as pty::SessionId
}

pub unsafe fn resize(hwnd: HWND) {
    if let Some(ctrl) = controller() {
        let mut rc = RECT::default();
        let _ = unsafe { GetClientRect(hwnd, &mut rc) };
        let _ = unsafe { ctrl.SetBounds(rc) };
    }
}

pub unsafe fn focus() {
    if let Some(ctrl) = controller() {
        let r = unsafe { ctrl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
        log(&format!("webview: MoveFocus(programmatic) -> {:?}", r.is_ok()));
    }
    post("{\"t\":\"focus\"}");
}

unsafe extern "system" fn enum_cb(h: HWND, _l: LPARAM) -> BOOL {
    let mut class = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut class) } as usize;
    let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
    log(&format!(
        "child window 0x{:X} class='{}' exstyle=0x{:X}",
        h.0 as usize,
        String::from_utf16_lossy(&class[..n]),
        ex
    ));
    BOOL(1)
}

pub unsafe fn log_children(hwnd: HWND) {
    log("--- child window tree under the overlay ---");
    let _ = unsafe { EnumChildWindows(Some(hwnd), Some(enum_cb), LPARAM(0)) };
}
