# Known gaps

Everything here is either untested or not implemented. Verified behaviour is in [test-results.md](test-results.md).

## Not yet tested (needs hardware, another setup, or a person)

| Item | What it takes |
|---|---|
| Pane frames and names drawn over the edge cells of a pane; the small-window fallback on a monitor that is not at 100% | by hand |
| Scalings other than 100%, 125%, 150%, 175%, 200% and 225% (tested: the main screen at 125%, 175%, 200% and 225%, and the left screen at 100%, 125%, 150% and 175%, all that Windows offers for them; `place`, `dpi`, `input` and `snap` were run again with the left screen at 150% on 2026-11-26, `-Suite mixscale`); the third (100%, 480 px high) screen offers nothing else | a display that offers more scalings |
| The size saved in `config.json` is in physical pixels: if the display scaling is changed between runs, the window comes back the same number of pixels, so a different logical size | change the scaling and restart |
| Other games: two borderless games were tried (2026-11-21 and 2026-11-22: see test-results); anti-cheat titles. **Exclusive fullscreen (one game, 2026-11-22): HUDshell was not visible over it and taking the keyboard (Ctrl+Alt+G) minimized the game**; use borderless / windowed mode. The guard (`exclusive_guard`, `exclusive_programs`) learns such a game at the first grab (it is minimized once, then restored) and refuses later grabs; tested with this one game only | try each under normal use; stop at any warning |
| Frame-time / FPS impact next to a running game: measured with PresentMon in a 30 fps-capped scene and in a second game (about 58 fps, no visible cost, but the baseline minute had a stall and the two HUDshell runs look limited by the game's own frame limit); no heavy, uncapped scene was measured cleanly | a repeat with a steady, uncapped scene and the game verified in front |
| Controller over a game: another program could read the controller with HUDshell shown and with the keyboard grabbed, and the person said the game reacted, also after HUDshell gave the keyboard back, with the game verified in front (2026-11-22, 15 of 15); controllers that are not XInput pads and long play were not tried | a non-XInput controller; long play |
| IME composition: Japanese (earlier sitting) and Korean (2026-11-22) arrive as real characters in the terminal; Chinese (Pinyin) composition was not tested: its IME stayed in direct input mode in the test | a way to switch that IME to composition mode |
| Layouts: German, French, Russian, Swedish, and the Japanese, Chinese and Korean keyboard layouts were tested with real key presses (the characters match what Windows says each key gives); layouts that have no key for the punctuation hotkeys (` ; ' / , . - = [ ]) | another layout installed |
| Claude Code / Codex permission, trust, login and update dialogs | manual walkthrough |
| Windows versions other than build 26300 | a Windows 10 22H2 or Windows 11 24H2 VM |
| A machine without the WebView2 runtime, Node or PowerShell 7 | a clean VM; check messages |
| User names with spaces or non-ASCII characters | a test user profile |
| Sleep/resume: one real sleep of 14 seconds was logged by Windows and HUDshell and both shells survived it (2026-11-22; the network adapter woke the computer early); a long sleep (minutes), hibernation and remote desktop were not tested | a sleep that lasts minutes (for example with the adapter's wake setting off, which is a system setting) |
| Long, busy sessions at low opacity over a game (readability) | judgement over real use |
| Whether Claude Code or Codex ring the terminal bell (the census does not count BEL; they sent no OSC 9 / OSC 777 in the real run) | count BEL in the census, or configure the CLIs' own notification setting (not done: it would edit their configuration) |
| The permission prompt about pasting clipboard text seen once during a policy run | its source was not identified; no WebView2 permission request was logged and a search found no documented Windows prompt of that kind. If it recurs, note the exact wording and the step |
| Corner docking, size presets, snapping and tiling with the taskbar on the left, top, right or bottom edge (tested 2026-11-21 by setting the registry value and restarting Explorer; the original setting was restored each time); the other monitors' taskbars were shown and hidden (2026-11-25, `tbmulti`; Windows 11 keeps all taskbars on one edge, so they cannot be moved separately) | the secondary monitors' taskbars |
| Tab strip while click-through is on (clicks pass through by design) | by hand |
| A profile's start command timing (typed 0.9 s after the shell starts; a slow profile script could miss it) | try a heavy profile |
| Clicking an OSC 8 link, the ghost tray menu item, the tray notification for attention requests, keyboard scrollback inside the CLIs | by hand |

## Not implemented

- **Hotkeys that another program also binds** act in both programs when that program is in front (a registered hotkey does not stop the program in front from seeing the chord). Found 2026-11-22: Windows Terminal binds Ctrl+Alt+, (it opens its defaults file in the default editor: the test run opened VS Code twice before the previous-corner hotkey was moved to Ctrl+Alt+Shift+.) and Ctrl+Alt+1 to 9 (switch to tab 1-9), which are also HUDshell's shell-jump hotkeys; they were kept (changeable in `config.json`) and documented in the README. Other programs' bindings were not checked.
- Exclusive fullscreen cannot be detected before a grab (Windows reports the same state as for a borderless game): HUDshell learns a program only after a grab minimized it once, or from the hand-edited `exclusive_programs`. HUDshell is never drawn over such a game.
- Full settings UI (the panel covers opacity, colour theme, text and UI scale, ghost opacity, tab style, line height and letter spacing; font, hotkeys, layout and shell are edited in `config.json`).
- Live settings reload (the `reload` suite passed 9/9 on 2026-11-21 by feeding settings text to the reload code; **not** tried with a real edit of `config.json` while HUDshell runs, the 1.5 s file watching, the tray item or the hotkey re-registration). Changing `window` by hand in the file while HUDshell runs has no effect until restart.
- **AltGr clash on layouts that have it.** AltGr (right Alt) is Ctrl+Alt on Windows, and every default hotkey is Ctrl+Alt+something, so a character typed with AltGr that shares a key with a hotkey is taken by HUDshell instead: on a German layout AltGr+Q (`@`) triggers the quit hotkey (observed: HUDshell quit); AltGr+E (`€`), AltGr+7 / 8 / 9 / 0 (`{ [ ] }`) and others will be taken the same way. HUDshell lists the clashing hotkeys in a tray notification at start (`kbd` checks 2026-11-21: named Ctrl+Alt+Q with a German layout active, silent with the normal layout; the notification itself was read from the log, not seen). Workaround: change those hotkeys in `config.json` to a chord that does not use Ctrl+Alt together. A different default modifier is not implemented.
- Several windows (tested 2026-11-21, see test-results): a detached or moved shell takes its screen and scrollback along (the old page hands over a snapshot, the new page writes it, then the shell repaints its screen; if the old page does not answer within 1.5 s the shell moves without it); a full-screen program (alternate screen) in the moved shell was tried and survives; at most 4 windows; a text size set with the hotkeys is per terminal and stays in the window it was made in (the colour theme, text and UI scale and reloaded settings go to every window); a window off every screen after a display change is moved back (tested with a simulated message and with a real unplug of one monitor, twice: the rescued windows keep their logical size and are staggered; they do not return to the monitor when it is plugged back); a saved window whose monitor is gone opens at the default place on a screen.
- Per-pane shell lists (`pane_shells = "own"`) have no per-pane close confirmation text, no drag of a shell from one pane to another, and the list hotkey shows only the focused pane's shells.
- Program labels (`tab_program`) look at process names only: a tool started through `node` (for example an npm-installed Claude Code) shows as `node` in `all` mode and not at all in `known` mode; any program that happens to be called `claude` or `codex` gets the label. Detection of the real Claude Code (native `claude.exe`) and Codex (the native `codex.exe` its npm launcher starts) has **not** been tested: the tests use a renamed copy of cmd.exe as a stand-in. The label can lag by up to 2 s and a very short program may never be seen.
- Text size: a terminal's own size is not saved (it lasts until HUDshell quits); the panel's rows for the scales, ghost opacity, tab style, line height and letter spacing were driven by their keys in the `ui` suite (their `[-]` / `[+]` buttons were not clicked by a test, only `[close]` was). The drag strip, the edge handles and the pane divider do not follow `ui_scale`. A fractional result of `text_scale` is rounded to a whole number of points.
- Windows' own snapping is not used: HUDshell snaps itself (maximize at the top edge, halves at the left and right edges with `edge_tiling`, flush corners; the maximize and the halves fill the whole monitor over the taskbar by default, `maximize_area = work` keeps the taskbar visible). There are no snap-layout corners or quarter tiles, and Win+Arrow is not handled. `maximize_area = work` was tested with the taskbar visible at the bottom (maximize and both half tiles stop at the taskbar); a taskbar on the left or top was not (this Windows cannot move it). The maximized state is not saved: HUDshell always starts at the normal size.
- Session restore (`restore_session = "shells"`; the `restore` suite passed 7/7 on 2026-11-21, with the shells and layout restored across two real starts). It restores shells by profile and the layout, not folders or running programs, and it saves only when the shell list or the layout changes (a shell's own title or output is never saved). It is off by default.
- When the shell cannot start at all (no PowerShell 7 found), the tab now says so and Enter tries again (the `sessions` suite passed 2026-11-21; the test feeds the page the same message because a real failure cannot be forced while other shells are available to fall back on).
- WebView2 failure recovery (the `recover` suite passed 6/6 on 2026-11-21: this instance's browser process was killed and the web view and shells came back; a render-process failure was not forced). A render process failure reloads the page; a browser process exit creates the web view again and opens the shells in the new page. The shells keep running, but what the old page showed (the screen and scrollback of every tab) is lost. Other process kinds (GPU, utility) are only logged.
- Free-form splitting: a window has at most four panes in the fixed presets (single, side by side, stacked, 2 x 2). `remember_layout` saves the preset and split only, not which shell is in which pane.
- Tabs: rename by double-click and drag-to-reorder were added 2026-11-21 (`tabsedit`, `tabsui`); with per-pane shell lists (`pane_shells = "own"`) the strip keeps its order (dragging says so); there is no tab for a shell started outside HUDshell.
- Win+Arrow (native Windows snapping) is not supported or tested; HUDshell snaps itself.
- Opening OSC 8 links: deliberately **not** done (clicks are ignored); copy the link text instead. OSC 52 is refused unless `osc52` is `write`.
- A right-click context menu (right-click copies / pastes only when `right_click` is `paste`), and kitty-style OSC 99 notifications. Per-game window places exist for the first window only (`game_positions`, off by default); the other windows are not moved by the program in front, and only the file name of the executable is used (two programs with the same file name share a place).
- Background-only opacity (text stays crisp): needs composition hosting of the web view.
- Installer, code signing, app icon and version resource, AppUserModelID (the notification header reads "hudshell.exe").
- WebGL renderer (DOM renderer only), ligatures, bundled fonts, bundled ConPTY (`conpty.dll`/OpenConsole).

## Robustness changes of 2026-11-26 that no input suite has exercised yet

A whole-application code review on 2026-11-26 led to the changes below. They compile, clippy is clean (plain and harness), the unit tests
pass (89), the `static` suite passes (17) and twelve desktop suites passed afterwards (173 checks: config, reload, shells, windows,
winmove, winrestore, ui, clipboard, policy, attention, exclguard, hideshow; see test-results.md). Those suites do not reach every
change; what no test has exercised is listed at the end of that run's entry, and the other desktop suites were not re-run:

- Settings file: a wrong-typed value costs only that value; UTF-16 files are read; an unreadable file is never replaced; a file that
  changed on disk since HUDshell read it (a hand edit not yet reloaded, or a broken file being fixed) is never saved over; saves are
  flushed before the rename; at most 10 `.bad-N` copies. Covered by unit tests for the pieces (decoding, lenient parsing, the
  may-replace rule, the durable write), not by a run that edits the file while HUDshell writes it.
- Shells: the process, input and output handles are closed when a shell ends (they leaked before); a tab closed while its output queue
  was full no longer leaves its reader thread waiting; resizing runs on a worker thread, never on the UI thread under the session lock
  (a known ConPTY freeze pattern, never reproduced here); the shell starts suspended and joins its kill-on-close job before it runs; a
  start that fails half-way closes everything it opened; the shell gets its environment block instead of HUDshell changing its own;
  an unreachable network start folder is given up after 1.5 s; input to a shell that does not read is capped at 8 MB; a profile's start
  command is typed once the shell has gone quiet (0.9-6 s) instead of after a fixed 0.9 s.
- Shell search: Program Files first, then only full-path `PATH` folders; a configured shell must be a full path without quotes.
- Windows: a closed window closes its WebView2 controller; a wake lost with a closing window no longer stops output in the others; a
  failed detach no longer kills the moved shell; an Explorer restart adds one tray icon, not one per window; notifications from
  windows 2-4 go through the first window's icon; config reloads, idle dimming and the "all" text size reach every window; game
  positions keep working after the first window closes; stale move snapshots and pending moves are dropped.
- Focus: the settings panel and the picker open only when the keyboard was really taken; losing the keyboard clears their grab.
- Exclusive guard: learns only while HUDshell still holds the keyboard, never learns hosts of other programs' windows, keeps its state per
  window, and the tray menu can clear the list.
- Page: pasted text (Ctrl+V and right-click paste) loses bracketed-paste markers and control characters; tab titles and notifications
  lose invisible formatting characters; notifications name their tab; OSC 52 only from a shell on screen, once a second, with a note on
  screen; WebView2 external drops, autofill, password saving, host objects and the built-in error page are off; a request the asset
  handler cannot read gets an error response; the CSP adds `base-uri` / `form-action` and is also sent as a header.
- Single instance: a lock that exists but cannot be opened counts as "running"; a second launch waits up to 2 s for the first window.
- Test runner: a changed display scaling is backed up to a file and put back by the next run; the IME test removes only the language
  features it installed; PresentMon is copied and hashed again just before its elevated start; temp result files have random names;
  the final clean-up runs after Ctrl+C. Not run (they change system settings and need the owner present).

Left as they are (judged not worth the change): program labels can be fooled by PID reuse (cosmetic); the window procedure trusts
messages from same-user processes (such a process could end HUDshell anyway); each window that was ever open keeps a few small
per-window values until HUDshell quits.

## Partly verified

- On a monitor shorter than the window (the 1920x480 strip), docking cuts the window to fit (900x464) and, because it then fills the height, the top and bottom corners are the same place: the hotkey alternates left and right instead of visiting four corners. The size hotkey's first press from such a window goes to the nearest preset (clamped to the work area) rather than the next one, by design. Fixed 2026-11-25 (docking used to leave the window hanging 48-96 px past the work area); tested on the strip with `tbmulti`, not with a real game.
- The test runner's preflight (stops when a program fills the main screen) passed with nothing in the way, and refused (exit code 3) with a stand-in window and with a real full-screen program (Edge kiosk) in front; it stops a leftover HUDshell. It also refuses a maximized window (tested with Notepad and a browser). A real game was not tried. The neutral window for the tests' keys worked in the full run of 2026-11-22 but not with a browser window in front.
- Tray icon: lives in the Windows 11 overflow flyout until pinned; pinned-on-taskbar case not checked.
- Click-through delivering input to the window underneath is proven by hit-testing, not by a recorded receiver.
- Light theme (`paper`) with programs that assume a dark background; the 16 ANSI colours are fixed.
- Rendering was checked from cell-text dumps and screenshots, not pixel-for-pixel against another terminal.
- Not tried: multi-hour soak, 100k-line scrollback, pastes over 1 MB, Windows PowerShell 5.1 beyond starting.

## Housekeeping

- Continuous integration (`.github/workflows/ci.yml`: release build, harness build, `static` suite on `windows-latest`)
  ran green once on GitHub. The desktop suites cannot run in CI (they need an interactive session) and stay manual.
  `cargo clippy -D warnings` (normal and harness builds, tests included) runs in CI since 2026-11-21; there is no `cargo fmt` gate (the code is formatted by hand with long lines).
- Workflow actions are pinned to commit hashes (2026-11-26), and CI builds with `npm ci --ignore-scripts` and `cargo --locked`; a weekly
  advisories job runs `npm audit` and `cargo audit`. It ran green on GitHub for commit 55cf18d (both jobs: build, clippy, static suite, `npm audit`, `cargo audit`). The weekly scheduled run has not happened yet.
- `rust-toolchain.toml` follows stable MSVC: rustup refuses an exact version together with the target in the channel name, and an exact
  version alone picked a GNU toolchain on a machine whose default host is GNU. `rust-version` in Cargo.toml (1.99) says which release
  builds were made with; the clippy and rustfmt components are listed.
- GitHub private vulnerability reporting (linked from the issue template and CONTRIBUTING) has been enabled in the
  repository settings by the owner; the report flow itself has not been exercised.
