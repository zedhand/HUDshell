# Known gaps

Everything here is either untested or not implemented. Verified behaviour is in [test-results.md](test-results.md).

## Not yet tested (needs hardware, another setup, or a person)

| Item | What it takes |
|---|---|
| Pane frames and names drawn over the edge cells of a pane; the small-window fallback on a monitor that is not at 100% | by hand |
| Scalings other than 100%, 125%, 150%, 175%, 200% and 225% (tested: the main screen at 125%, 175%, 200% and 225%, and the left screen at 100%, 125%, 150% and 175%, all that Windows offers for them); the third (100%, 480 px high) screen offers nothing else | a display that offers more scalings |
| The size saved in `config.json` is in physical pixels: if the display scaling is changed between runs, the window comes back the same number of pixels, so a different logical size | change the scaling and restart |
| Other games (one borderless game was tried 2026-11-21: see test-results); exclusive-fullscreen games (overlays cannot draw over them); anti-cheat titles beyond that one | try each under normal use; stop at any warning |
| Frame-time / FPS impact next to a running game (measured once with PresentMon in one scene that was capped at 30 fps, which hides small costs: no difference was visible) | a scene that is not frame-capped, and a heavier game |
| Games that capture raw mouse or controller input when focus is grabbed | observe on grab / release |
| IME composition beyond Japanese: the Microsoft Japanese IME was tested (romaji to kanji arrives in the terminal); Chinese and Korean input methods were not (their layouts cannot be switched on in a test window without changing the user's settings) | those input methods |
| Layouts: German, French, Russian, Swedish and the Japanese keyboard layout were tested with real key presses (the characters match what Windows says each key gives); Chinese and Korean layouts could not be switched in; layouts that have no key for the punctuation hotkeys (` ; ' / , . - = [ ]) | another layout installed |
| Claude Code / Codex permission, trust, login and update dialogs | manual walkthrough |
| Windows versions other than build 26300 | a Windows 10 22H2 or Windows 11 24H2 VM |
| A machine without the WebView2 runtime, Node or PowerShell 7 | a clean VM; check messages |
| User names with spaces or non-ASCII characters | a test user profile |
| Sleep/resume: the test exists (`sleepresume`) but Windows did not really sleep (it entered away mode because a streaming program and an audio stream asked it to stay awake), so nothing was proven; remote desktop | close the streaming program and stop audio, then run `-Suite sleepresume` |
| Long, busy sessions at low opacity over a game (readability) | judgement over real use |
| Whether Claude Code or Codex ring the terminal bell (the census does not count BEL; they sent no OSC 9 / OSC 777 in the real run) | count BEL in the census, or configure the CLIs' own notification setting (not done: it would edit their configuration) |
| The permission prompt about pasting clipboard text seen once during a policy run | its source was not identified; no WebView2 permission request was logged and a search found no documented Windows prompt of that kind. If it recurs, note the exact wording and the step |
| Corner docking, size presets, snapping and tiling with the taskbar on the left, top, right or bottom edge (tested 2026-11-21 by setting the registry value and restarting Explorer; the original setting was restored each time); the secondary monitors' taskbars were not moved | the secondary monitors' taskbars |
| Tab strip while click-through is on (clicks pass through by design) | by hand |
| A profile's start command timing (typed 0.9 s after the shell starts; a slow profile script could miss it) | try a heavy profile |
| Clicking an OSC 8 link, the ghost tray menu item, the tray notification for attention requests, keyboard scrollback inside the CLIs | by hand |

## Not implemented

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
