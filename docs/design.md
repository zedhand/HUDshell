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
- **Moving and resizing.** The web view covers the whole window, so a real mouse press on the top strip or an edge lands on the web view and
  never reaches the host's own hit-test (`WM_NCHITTEST` still answers "caption" / "border" there, for hit-test queries). The page therefore
  has an invisible handle on each edge and corner and treats the top strip as a handle: a press on one sends `nc` with its kind, and the host
  posts `WM_NCLBUTTONDOWN` with the matching hit-test code, which starts Windows' own move / resize loop (so edge snapping in `WM_MOVING`
  and the size limits apply). These presses are not "clicks on the terminal", so they never take the keyboard. Before 2026-11-20 this did not
  work: the window could only be moved with the corner hotkeys.
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

## Placement, opacity and start

- **Placement** (`src/place.rs`, pure functions with unit tests): the corner hotkeys dock the window in the next or previous corner of the
  monitor it is on, keeping its size; from a position that is not a corner the first press goes to the nearest one. Size presets are in
  96-dpi pixels and scaled for the monitor, keeping the nearest corner anchored. Magnetic snapping edits the rectangle in `WM_MOVING`.
- **One opacity function.** The window alpha comes from one place: the chosen value (the active one while the keyboard is held, if set),
  lowered by ghost mode or by idle dimming, never raised. Nothing but the user's own choice is ever written to `config.json`.
- **Second launch** posts a registered message to the running window, which unhides itself, leaves ghost mode and stays on top; the second
  process then exits. **Start with Windows** writes one value under `HKCU\...\Run` (tests use a different value name).

## Shells (tabs)

- The host keeps a table of sessions (`pty.rs`): each has an id, its own output queue and close flag, and its own ConPTY and job object.
  Every bridge message that belongs to a shell carries `s`, the session id. The page keeps one xterm.js view per id and shows one.
- The list of shells and which is active lives in the host (`shells.rs`, pure logic with unit tests), so the global hotkeys work while
  the page has no keyboard. The page draws what the host says: a tab strip (`tabs = auto | always`) or a short overlay list (`overlay`).
- Tab names come from each program's own window title (OSC 0 / 2), shown whole (up to 40 characters, the tab cuts the rest with an
  ellipsis), as a normal terminal tab does; an executable path (PowerShell's default) is replaced by the profile name.
- **Program label.** Every 2 s the host takes a snapshot of the process list and, for each shell, looks at the processes below the
  shell's own process (`src/progs.rs`, unit-tested on made-up process trees). A recognised program (`claude`, `codex`, `gemini`,
  `aider`, `opencode`; with `tab_program = "all"` also the deepest other program, ignoring helpers and nested shells) is put in front of the
  title (`claude · <title>`). It is read-only: nothing is injected, no program's configuration is touched, and the names are not logged
  (what a shell runs says what the user is doing). The label only changes when the answer changes. A tab is marked when it receives output while not shown, and marked more strongly when its program rings the bell.
- Closing asks twice (a second press within 3 s), because a shell may be running an agent. Closing the last shell starts a fresh one.
  A shell that exits stays in its tab until Enter starts a new one.
- A profile is `{name, dir, command, shell, color}`. The command is typed into the new shell about 0.9 s after it starts.

## Split panes

- By default (`pane_shells = "shared"`) panes are views onto shells from the one tab list, not a separate kind of shell.
  With `pane_shells = "own"` each pane keeps its own list of shells (still the same shells and the same limit of nine):
  the tab strip, list overlay, previous / next and the number keys refer to the focused pane's list, a new shell (and a profile
  picked in the picker) joins the focused pane, a new pane starts empty (or gets a new shell with `layout_fill = "new"`), closing a
  pane's last shell leaves the pane empty and moves the keyboard to a pane that has a shell, and going to fewer panes appends the
  dropped panes' shells to the focused pane's list so none is lost. The attention key still searches every pane. The pane's corner
  name reads `position/length name`, with a dot when another shell of that pane has unseen output. The host keeps the layout (`src/layout.rs`, pure logic
  with unit tests): the preset (single, side by side, stacked, 2 x 2), which shell is in each pane (or none), the focused pane, zoom,
  and the split percentages. The page only draws what it is sent; each shell's ConPTY is told its own pane's size.
- Shared mode: choosing a preset fills the panes with the focused shell, then the shells already on screen, then the rest of the tab order.
  With `layout_fill = "new"` missing shells are started; with `"empty"` the panes stay empty with a hint.
- Switching to a shell that is already on screen focuses its pane; switching to one that is not puts it into the focused pane.
  A new shell goes into the focused pane if it is empty, else the first empty one. Closing a shell refills its pane with a shell
  that is not on screen, or leaves it empty.
- The page reports the terminal area in cells; if a pane would fall under `pane_min`, the host draws a smaller preset (2 x 2 ->
  side by side -> stacked -> single) showing the focused pane and the ones after it, and goes back when the window grows.
- Zoom shows the focused pane alone at full size; moving the pane focus while zoomed shows the next pane zoomed.
- Equal splits by default. `split` sets uneven ones; `pane_dividers` adds draggable dividers (mouse only, so not while click-through).
- The layout is not restored at start unless `remember_layout` is on, and then only the preset and split (session restore is later).

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
