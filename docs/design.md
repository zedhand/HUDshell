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
- **Scaling.** The process is per-monitor DPI aware (v2). When the window is moved to a monitor with another scaling, `WM_DPICHANGED` applies the
  rectangle Windows suggests, so the window keeps its logical size (it grows 1.5 x going from 100% to 150%) and the page, whose scale WebView2
  follows by itself, re-fits the terminal. Pixel distances in the settings (`snap_distance`, `snap_margin`, `sizes`) are 96-dpi values scaled by
  the window's current scaling. The size stored in `config.json` is physical pixels (see known-gaps).
- **Moving and resizing.** The web view covers the whole window, so a real mouse press on the top strip or an edge lands on the web view and
  never reaches the host's own hit-test (`WM_NCHITTEST` still answers "caption" / "border" there, for hit-test queries). The page therefore
  has an invisible handle on each edge and corner and treats the top strip as a handle: a press on one sends `nc` with its kind, and the host
  posts `WM_NCLBUTTONDOWN` with the matching hit-test code, which starts Windows' own move / resize loop (so edge snapping in `WM_MOVING`
  and the size limits apply). These presses are not "clicks on the terminal", so they never take the keyboard. Before 2026-11-20 this did not
  work: the window could only be moved with the corner hotkeys.
- **Snapping and maximizing.** Windows proposes each rectangle of a drag relative to the one that was *last applied*, so snapping must not
  feed its own result back (a window held by the snap distance could never be pulled away slowly: found by a real-mouse test on 2026-11-20).
  `place::SnapDrag` keeps the position the window would have without snapping and snaps that, so the magnet holds and then releases.
  Near a corner (both axes within 3 x the snap distance plus the margin) the window goes flush into the corner with no gap
  (`place::corner_snap`); along a single edge it keeps the margin. Windows 11 rounds a window's corners all-or-none, so
  `update_corner_style` (on `WM_MOVE` / `WM_SIZE`) switches `DWMWA_WINDOW_CORNER_PREFERENCE` to `DONOTROUND` while the window touches a
  corner of the work area or is maximized, and back to `ROUND` otherwise.
  Releasing a move with the cursor on the top edge of a monitor (or double-clicking the strip) makes the window fill that monitor
  (`maximize_area = monitor`, over the taskbar: the window is also raised within the topmost band so the taskbar does not cover it) or its work
  area (`work`); it is done by resizing the window (never with Windows' own maximize, which would activate it and take the keyboard), and the
  rectangle it had is kept. A maximized window leaves that state on the first actual movement of a drag from its strip (not on a plain
  click), keeping its proportional place under the cursor, at its earlier size in the scaling of the monitor it is on. Windows keeps the
  grab offset it took in the maximized window and bases every later proposal of the drag on it, which would put the restored window flush
  left; `RESTORE_ANCHOR` works out the sideways difference at the first proposal after the restore and applies it for the rest of the drag
  (found 2026-11-26 by the `snap` suite). The corner and size hotkeys restore it first; the saved window position is the restored rectangle.
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

## Text size

- Each terminal has a base size in points (starting at `font_size`, 8-48). The size xterm.js draws is `round(base * text_scale / 100)` (at least 6),
  so `text_scale` is one multiplier over everything and a terminal's own size stays a number the user can reason about.
- The bigger / smaller / reset hotkeys (`text_scale_hotkeys`) change the base size of the focused terminal only, kept by the host for the run
  (not saved: a saved per-terminal size needs session restore); with `"all"` they change every terminal and save `font_size` (the earlier
  behaviour). Reset goes back to the `font_size` of the start. A terminal opened later starts at the common size.
- `line_height` and `letter_spacing` are xterm.js options of every terminal. `ui_scale` sets one CSS variable that HUDshell's own text and
  the tab strip height are multiples of; at 100 the pixel values are exactly what they were before the setting existed.
- The settings panel has rows for `text_scale` and `ui_scale` (steps of 10%), `ghost_opacity` (10), `tabs` (auto / always / overlay), `line_height` (5) and `letter_spacing` (1 px), all live, saved and sent to every window; every row has a key as well as buttons (the keys are listed in the README). The panel keeps 12 lines and the width of its opacity bar, so the real-mouse checks that aim at it keep their positions. A config file edited by hand is reloaded while running (see Live settings).

## Session restore

- With `restore_session = "shells"` the host writes `session.json` (next to `config.json`) whenever the shell list or the layout changes and at
  quit: the profile of every shell in tab order, the active shell and the layout (preset, split, which shell is in which pane, the per-pane lists in
  own mode). Shell ids are saved as tab-order indexes, so they survive the new numbering (`src/session.rs`, pure logic with unit tests).
- At start the file is read first and saving is held back until the page is ready, so the start itself never overwrites it. Shell 1 runs the
  first saved profile; once the page is ready the other shells are started from their profiles and the layout is put back (it wins over the
  `layout` setting). A missing, damaged, inconsistent or newer-version file is ignored and one shell starts.
- Only profiles are restored. A shell's current folder and the program running in it are not known to HUDshell, so they come back as the
  profile's start folder and start command say. A profile that no longer exists becomes the default profile.

## Several windows

One process can hold up to four windows (`perwin::MAX_WINDOWS`), each with its own web view, shells (up to 9), layout, pointer mode, opacity and
drag state. The shells themselves (ConPTY sessions) are process-wide with ids that are unique across windows (`NEXT_SHELL_ID`); a window only knows
which ids it shows.

- **Per-window state without rewriting every call site.** `perwin.rs` keeps a table of windows (a serial number each, not the HWND, which Windows
  may reuse) and a "current window". A `PerWin<T>` static holds one `T` per window and dereferences to the current window's, so
  `CLICK_THROUGH.load(..)`, `shells_lock()`, `layout_lock()` and the rest kept their names. The current window is set on entry to the window procedure
  and to each page message (`enter_hwnd`) and restored when the guard is dropped; everything runs on the UI thread. `wv.rs` keeps its controller,
  web view and in-flight byte count in maps keyed by the same serial.
- **Output routing.** The pty reader threads wake one window (`pty::set_wake_target`, moved to the first window if that one closes); the handler
  pumps every window, each taking only the chunks of its own shells (`pty::take_chunk_for`). A session no window lists (a test script's) goes to
  the first window. "Shell ended" goes to the window that holds the shell (`in_window_of`).
- **Detaching.** `detach_shell` creates the new window first (`create_window`: placed 36 px off the old one, same pointer mode, page started with the
  shell's id through `window.__hudshell.first`), then takes the shell out of the old window (`remove_shell_from_window`, the part of closing a shell
  that is not killing it). The new page has no history of the shell, so the shell is made to repaint: `pty::redraw` resizes it one column narrower
  and back. The old page hands over a snapshot first (`snapshot` message, answered by the serialize addon with the screen and the scrollback as terminal text, 1.5 s timeout), which the new page writes before the shell's repaint. At most four windows; a window's only shell cannot be detached; closing the last shell of a
  window closes the window when another is open (with one window it still starts a fresh shell).
- **Hotkeys and the tray.** The hotkeys and the tray icon belong to the first window; when it closes they move to the next (`window_gone`). A
  hotkey acts on the active window (`ACTIVE_WINDOW`: the one that last had the keyboard, was clicked or dragged, or was made). The tray menu
  acts on it too and adds "all windows" items for ghost and show / hide.
- **Snapping and groups.** `place::snap_to_rects` puts a dragged window flush against another (a screen edge wins on its axis) and lines up the
  matching edge; `place::group_of` finds the windows that touch the dragged one, directly or through each other, and they follow it by the same
  amount (Shift held at the start moves only that window).
- **Session.** The first window's session carries the others in `windows` (shells by profile, layout, and the window's place); an old file reads as
  before. The other windows are opened from a posted message once the first page is ready, and each finishes its own session when its own page is.
  Saving starts when the last one has; quitting saves once, with every window still open (`quit_all`).
- **Lesson: a web view cannot be created from inside a web view's own message callback** (it waits for itself and the window stays a white
  rectangle). Anything a page message opens a window for is therefore posted (`WM_APP_DETACH`, `WM_APP_RESTORE_WINDOWS`), like the recovery path
  was already.

## When the web view fails

- The web view reports a failed process through one event; its handler only posts a message to the main window (rebuilding blocks, so it must not
  run inside the callback). A failed render process, an unresponsive one or a frame's is cured by loading the page again; when the browser process itself
  exited the controller and web view are created again, the unacknowledged-output counter is reset (the dead page will never acknowledge it) and the
  new page, once it asks for the shell list, is told to open the shells it does not know. The pty sessions are independent of the page, so the programs keep
  running; the old screens are gone.

## Live settings

- In a normal run a timer checks the modified time of `config.json` every 1.5 s (tests never read or write the real file). A changed file is parsed
  without side effects (`config::try_load`: a bad file is not renamed, unlike at start) and compared with the settings in use key by key
  (`config::changed_keys`; the window position is ignored because HUDshell writes it itself, which also makes HUDshell's own saves a no-op).
  An unreadable or invalid file must be seen twice in a row before a notice is shown (an editor may be halfway through writing).
- What changed decides what is applied: opacity (the one opacity function), colours, hotkeys (all unregistered and registered again, with the
  usual conflict notices and the AltGr check), tab mode and profiles, pane options, and one `cfg` message with the text, font, scrollback and
  clipboard settings that the page applies itself (a terminal that has its own text size keeps it). Settings only read at start (`shell`,
  `unicode`, `shift_enter`, `pane_shells`, `layout`, `start_hidden`, `start_ghost`) produce a notice that they apply after a restart.

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
