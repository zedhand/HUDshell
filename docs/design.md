# Design notes

## Shape of the program

```
hudshell.exe (Rust, raw Win32)
 ├─ main window: topmost, tool window, layered, frameless; owns hotkeys, tray icon, config, focus rules
 ├─ pty.rs      ConPTY host: CreatePseudoConsole + Job Object (kill on close), reader/writer/exit threads
 ├─ wv.rs       WebView2 host (windowed hosting) + message bridge to the page; serves the UI from memory
 ├─ assets.rs   web/dist/* compiled into the exe (include_bytes!), served as https://hudshell.local/*
 ├─ config.rs   %APPDATA%\HUDshell\config.json: load, validate, atomic save
 └─ harness.rs  test harness, compiled only with `--features harness`
web/ (xterm.js 6.0.0 + app.js): renders the terminal, answers terminal queries, sends keystrokes back
```

Key decisions and why:

- **Real PowerShell via ConPTY.** HUDshell is only the window; the shell is the real `pwsh.exe`, so
  PSReadLine, profiles and tools behave as in any terminal. The system ConPTY is used (flags 0).
- **WebView2 + xterm.js renderer.** Chosen over writing a renderer. Cost: about 150-190 MB across the
  WebView2 helper processes (the HUDshell process itself is 3-4 MB). Accepted for now.
- **Bridge.** Output goes host -> page as base64 in bounded chunks with an acknowledgement per chunk
  (backpressure); input goes page -> host as base64 bytes. Only messages from `https://hudshell.local/`
  are accepted; navigation and new windows are blocked; DevTools, browser shortcuts, context menu and
  zoom are disabled.
- **Local content only.** The page, scripts and styles are embedded in the exe and served from memory by a
  `WebResourceRequested` handler; the content-security policy allows no remote sources.

## Overlay and focus model

- The window is `WS_EX_TOPMOST | TOOLWINDOW | NOACTIVATE | LAYERED`. While passive it never takes the keyboard
  (`MA_NOACTIVATE`), so a game keeps focus.
- **Click-through** adds `WS_EX_TRANSPARENT`: mouse input falls through. It is a user setting (default on).
- **Grab focus** (hotkey, tray, or clicking the terminal while interactive) clears `NOACTIVATE`, calls
  `SetForegroundWindow` (allowed because a registered hotkey or real input is in progress) and makes the window
  clickable while focused. **Release** restores the previous foreground window and click-through.
- If something else takes focus while HUDshell holds it, HUDshell returns to the passive state automatically.
- Only `RegisterHotKey` is used for global keys: no keyboard hooks, no input injection into other programs.
- Opacity is `SetLayeredWindowAttributes` on the whole window, so text and background fade together.
  Fading only the background would need a different (composition) way of hosting the web view.

- **Ghost** (Ctrl+Alt+X) remembers the real opacity and pointer mode, applies `ghost_opacity` (never brighter than the current
  level) with click-through forced on, and restores both on the second press. The dimmed value is applied to the window only; it is
  never written to the config. Toggling click-through, changing opacity or grabbing the keyboard ends ghost mode first.
- **Attention cue.** The page watches for BEL, OSC 9 and OSC 777;notify (OSC 9;n with a number is ConEmu progress / working directory and
  is ignored) and tells the host. If HUDshell does not hold the keyboard the host asks the page to pulse the border, or (window hidden)
  raises a tray notification. At most one per 3 s; the text is shown but never logged. Needs no hooks and no configuration of the programs.

## Clipboard, links and permissions

- Clipboard **reads** (right-click paste) and OSC 52 **writes** are done by the host with Win32, because the page-side clipboard API needs
  keyboard focus (a passive overlay has none) and would need a browser permission. Every WebView2 permission request is denied silently.
- All of it is opt-in: `copy_on_select`, `right_click = "paste"`, `osc52 = "write"`. An OSC 52 read query is never answered, so a program
  cannot learn what is on the clipboard.
- OSC 8 hyperlinks render but a click does nothing (xterm.js's default handler would show a confirm dialog and open a window over the game).
- Scrollback keys (Shift+PageUp/Down, Ctrl+Shift+Up/Down/Home/End) are handled in the page on the normal screen only; full-screen programs
  keep those keys.

## Terminal fidelity decisions

- **Unicode width:** conhost and xterm.js disagree on some emoji (for example U+2728 is two cells to ConPTY, one
  to xterm.js by default), which corrupted cursor-addressed redraws. The Unicode 11 addon fixes this and is the default.
- **Shift+Enter:** xterm.js has no kitty keyboard protocol. `app.js` answers the protocol's query/push/pop/set
  sequences; while a program has negotiated it, Shift+Enter is sent as `ESC [ 13 ; 2 u`, otherwise as ESC CR
  (`1b 0d`), which Claude Code and Codex both treat as a newline and PowerShell's PSReadLine ignores, so Shift+Enter can
  never submit a prompt by accident. Ctrl+J was rejected (Codex ignores it).
- **XTVERSION** is answered honestly (`HUDshell 0.1.0`).
- **Synchronized output (DEC 2026)** is used heavily by both CLIs and is supported by xterm.js 6.

## Configuration and failure handling

Missing config -> written with defaults. Invalid JSON -> kept as `config.json.bad-N`, defaults used, user told by a
tray notification. A UTF-8 byte-order mark is accepted. Values are clamped (opacity 20-100, font 8-48, scrollback
100-100000, window size minimums). A hotkey that fails to register is reported, not fatal. If Explorer restarts
the tray icon is re-added.

## Test hooks stay out of the product

Scripted scenarios, DevTools-protocol key injection, real-click and hotkey injectors, the foreground watchdog,
control-sequence census and command-line test switches live in `src/harness.rs` and a few `cfg`-gated lines
elsewhere. A normal build contains none of it and ignores every test switch (checked by `scripts/test-all.ps1`).
