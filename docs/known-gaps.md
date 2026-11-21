# Known gaps

Everything here is either untested or not implemented. Verified behaviour is in [test-results.md](test-results.md).

## Not yet tested (needs hardware, another setup, or a person)

| Item | What it takes |
|---|---|
| Pane frames and names drawn over the edge cells of a pane; the small-window fallback on a monitor that is not at 100% | by hand |
| Scalings other than 100% and 150% (tested: one 100% and one 150% monitor, side by side); dragging the window across the border between them with the real mouse (the scaling change happens during the drag; the tests move the window with a function call); the settings panel's real-mouse checks at 150% (they use pixel offsets for 100%); the third (100%, 480 px high) monitor | a display at 125% / 200%, and a hand drag across |
| The size saved in `config.json` is in physical pixels: if the display scaling is changed between runs, the window comes back the same number of pixels, so a different logical size | change the scaling and restart |
| Other games; exclusive-fullscreen games (overlays cannot draw over them); anti-cheat titles | try each under normal use; stop at any warning |
| Frame-time / FPS impact next to a running game | capture frame times with and without HUDshell |
| Games that capture raw mouse or controller input when focus is grabbed | observe on grab / release |
| IME composition (CJK) | a real IME; xterm.js has open IME issues |
| Layouts beyond German and French (tested with real key presses: accented letters, dead keys, the default hotkeys still fire); IME composition; layouts that have no key for the punctuation hotkeys (` ; ' / , . - = [ ]) | another layout installed |
| Claude Code / Codex permission, trust, login and update dialogs | manual walkthrough |
| Windows versions other than build 26300 | a Windows 10 22H2 or Windows 11 24H2 VM |
| A machine without the WebView2 runtime, Node or PowerShell 7 | a clean VM; check messages |
| User names with spaces or non-ASCII characters | a test user profile |
| Sleep/resume, display unplug/replug while running, remote desktop | by hand |
| Long, busy sessions at low opacity over a game (readability) | judgement over real use |
| Whether Claude Code or Codex ring the terminal bell (the census does not count BEL; they sent no OSC 9 / OSC 777 in the real run) | count BEL in the census, or configure the CLIs' own notification setting (not done: it would edit their configuration) |
| The permission prompt about pasting clipboard text seen once during a policy run | its source was not identified; no WebView2 permission request was logged and a search found no documented Windows prompt of that kind. If it recurs, note the exact wording and the step |
| Corner docking, size presets, edge snapping and mouse resizing with the taskbar on another edge or hidden (tested on a 100% and a 150% monitor with the taskbar at the bottom of the primary one) | move the taskbar |
| Tab strip while click-through is on (clicks pass through by design) | by hand |
| A profile's start command timing (typed 0.9 s after the shell starts; a slow profile script could miss it) | try a heavy profile |
| Clicking an OSC 8 link, the ghost tray menu item, the tray notification for attention requests, keyboard scrollback inside the CLIs | by hand |

## Not implemented

- Full settings UI (the panel covers opacity, colour theme, text and UI scale, ghost opacity, tab style, line height and letter spacing; font, hotkeys, layout and shell are edited in `config.json`).
- Live settings reload (the `reload` suite passed 9/9 on 2026-11-21 by feeding settings text to the reload code; **not** tried with a real edit of `config.json` while HUDshell runs, the 1.5 s file watching, the tray item or the hotkey re-registration). Changing `window` by hand in the file while HUDshell runs has no effect until restart.
- **AltGr clash on layouts that have it.** AltGr (right Alt) is Ctrl+Alt on Windows, and every default hotkey is Ctrl+Alt+something, so a character typed with AltGr that shares a key with a hotkey is taken by HUDshell instead: on a German layout AltGr+Q (`@`) triggers the quit hotkey (observed: HUDshell quit); AltGr+E (`€`), AltGr+7 / 8 / 9 / 0 (`{ [ ] }`) and others will be taken the same way. HUDshell lists the clashing hotkeys in a tray notification at start (`kbd` checks 2026-11-21: named Ctrl+Alt+Q with a German layout active, silent with the normal layout; the notification itself was read from the log, not seen). Workaround: change those hotkeys in `config.json` to a chord that does not use Ctrl+Alt together. A different default modifier is not implemented.
- Several windows (tested 2026-11-21, see test-results): a detached or moved shell starts with an empty view (the shell repaints its screen, but the scrollback it had in the old window is gone); at most 4 windows; a text size set with the hotkeys is per terminal and stays in the window it was made in (the colour theme, text and UI scale and reloaded settings go to every window); a window off every screen after a display change is moved back (tested with a simulated display-change message, not with a monitor actually unplugged); a saved window whose monitor is gone opens at the default place on a screen.
- Per-pane shell lists (`pane_shells = "own"`) have no per-pane close confirmation text, no drag of a shell from one pane to another, and the list hotkey shows only the focused pane's shells.
- Program labels (`tab_program`) look at process names only: a tool started through `node` (for example an npm-installed Claude Code) shows as `node` in `all` mode and not at all in `known` mode; any program that happens to be called `claude` or `codex` gets the label. Detection of the real Claude Code (native `claude.exe`) and Codex (the native `codex.exe` its npm launcher starts) has **not** been tested: the tests use a renamed copy of cmd.exe as a stand-in. The label can lag by up to 2 s and a very short program may never be seen.
- Text size: a terminal's own size is not saved (it lasts until HUDshell quits); the panel's rows for the scales, ghost opacity, tab style, line height and letter spacing were driven by their keys in the `ui` suite (their `[-]` / `[+]` buttons were not clicked by a test, only `[close]` was). The drag strip, the edge handles and the pane divider do not follow `ui_scale`. A fractional result of `text_scale` is rounded to a whole number of points.
- Windows' own snapping is not used: HUDshell snaps itself (maximize at the top edge, halves at the left and right edges with `edge_tiling`, flush corners; the maximize and the halves fill the whole monitor over the taskbar by default, `maximize_area = work` keeps the taskbar visible). There are no snap-layout corners or quarter tiles, and Win+Arrow is not handled. `maximize_area = work` and a taskbar on another edge were not tested (the taskbar is hidden on the test machine, so the work area equals the monitor there). The maximized state is not saved: HUDshell always starts at the normal size.
- Session restore (`restore_session = "shells"`; the `restore` suite passed 7/7 on 2026-11-21, with the shells and layout restored across two real starts). It restores shells by profile and the layout, not folders or running programs, and it saves only when the shell list or the layout changes (a shell's own title or output is never saved). It is off by default.
- When the shell cannot start at all (no PowerShell 7 found), the tab now says so and Enter tries again (the `sessions` suite passed 2026-11-21; the test feeds the page the same message because a real failure cannot be forced while other shells are available to fall back on).
- WebView2 failure recovery (the `recover` suite passed 6/6 on 2026-11-21: this instance's browser process was killed and the web view and shells came back; a render-process failure was not forced). A render process failure reloads the page; a browser process exit creates the web view again and opens the shells in the new page. The shells keep running, but what the old page showed (the screen and scrollback of every tab) is lost. Other process kinds (GPU, utility) are only logged.
- Free-form splitting: a window has at most four panes in the fixed presets (single, side by side, stacked, 2 x 2). `remember_layout` saves the preset and split only, not which shell is in which pane.
- Tab drag-to-reorder, rename by hand, and a tab for a shell started outside HUDshell.
- Win+Arrow (native Windows snapping) is not supported or tested; HUDshell snaps itself.
- Opening OSC 8 links: deliberately **not** done (clicks are ignored); copy the link text instead. OSC 52 is refused unless `osc52` is `write`.
- A right-click context menu (right-click copies / pastes only when `right_click` is `paste`), kitty-style OSC 99 notifications, and per-game position profiles.
- Background-only opacity (text stays crisp): needs composition hosting of the web view.
- Installer, code signing, app icon and version resource, AppUserModelID (the notification header reads "hudshell.exe").
- WebGL renderer (DOM renderer only), ligatures, bundled fonts, bundled ConPTY (`conpty.dll`/OpenConsole).

## Partly verified

- Tray icon: lives in the Windows 11 overflow flyout until pinned; pinned-on-taskbar case not checked.
- Click-through delivering input to the window underneath is proven by hit-testing, not by a recorded receiver.
- Light theme (`paper`) with programs that assume a dark background; the 16 ANSI colours are fixed.
- Rendering was checked from cell-text dumps and screenshots, not pixel-for-pixel against another terminal.
- Not tried: multi-hour soak, 100k-line scrollback, pastes over 1 MB, Windows PowerShell 5.1 beyond starting.

## Housekeeping

- Continuous integration (`.github/workflows/ci.yml`: release build, harness build, `static` suite on `windows-latest`)
  ran green once on GitHub. The desktop suites cannot run in CI (they need an interactive session) and stay manual.
  `cargo clippy -D warnings` (normal and harness builds, tests included) runs in CI since 2026-11-21; there is no `cargo fmt` gate (the code is formatted by hand with long lines).
- Workflow actions are pinned to major versions (`checkout@v7`, `setup-node@v7`, `rust-cache@v2`), not commit hashes.
- GitHub private vulnerability reporting (linked from the issue template and CONTRIBUTING) has been enabled in the
  repository settings by the owner; the report flow itself has not been exercised.
