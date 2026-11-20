# Known gaps

Everything here is either untested or not implemented. Verified behaviour is in [test-results.md](test-results.md).

## Not yet tested (needs hardware, another setup, or a person)

| Item | What it takes |
|---|---|
| Pane frames and names drawn over the edge cells of a pane; the small-window fallback on a monitor that is not at 100% | by hand |
| Mixed-DPI behaviour (all test displays were 100%) | a second display at 125-200% |
| Other games; exclusive-fullscreen games (overlays cannot draw over them); anti-cheat titles | try each under normal use; stop at any warning |
| Frame-time / FPS impact next to a running game | capture frame times with and without HUDshell |
| Games that capture raw mouse or controller input when focus is grabbed | observe on grab / release |
| IME composition (CJK) | a real IME; xterm.js has open IME issues |
| Non-US layouts, AltGr, dead keys | another layout installed |
| Claude Code / Codex permission, trust, login and update dialogs | manual walkthrough |
| Windows versions other than build 26300 | a Windows 10 22H2 or Windows 11 24H2 VM |
| A machine without the WebView2 runtime, Node or PowerShell 7 | a clean VM; check messages |
| User names with spaces or non-ASCII characters | a test user profile |
| Sleep/resume, display unplug/replug while running, remote desktop | by hand |
| Long, busy sessions at low opacity over a game (readability) | judgement over real use |
| Whether Claude Code or Codex ring the terminal bell (the census does not count BEL; they sent no OSC 9 / OSC 777 in the real run) | count BEL in the census, or configure the CLIs' own notification setting (not done: it would edit their configuration) |
| The permission prompt about pasting clipboard text seen once during a policy run | its source was not identified; no WebView2 permission request was logged and a search found no documented Windows prompt of that kind. If it recurs, note the exact wording and the step |
| Corner docking, size presets, edge snapping and mouse resizing on a second monitor with different scaling, and with the taskbar on another edge (tested on one monitor at 100%, with real mouse drags for snapping and resizing) | a second display at 125-200% |
| Tab strip while click-through is on (clicks pass through by design) | by hand |
| A profile's start command timing (typed 0.9 s after the shell starts; a slow profile script could miss it) | try a heavy profile |
| Clicking an OSC 8 link, the ghost tray menu item, the tray notification for attention requests, keyboard scrollback inside the CLIs | by hand |

## Not implemented

- Full settings UI (the panel covers opacity and colour theme; font, hotkeys and shell are edited in `config.json`).
- Live config reload (config is read at start); `WM_DPICHANGED` handling; WebView2 crash recovery.
- A friendly error when the shell cannot start (it is logged only).
- Several windows (detach, snap to each other, move as a group) and session restore (reopening the same shells, folders and panes at launch). Planned next.
- Per-pane shell lists (`pane_shells = "own"`) have no per-pane close confirmation text, no drag of a shell from one pane to another, and the list hotkey shows only the focused pane's shells.
- Program labels (`tab_program`) look at process names only: a tool started through `node` (for example an npm-installed Claude Code) shows as `node` in `all` mode and not at all in `known` mode; any program that happens to be called `claude` or `codex` gets the label. Detection of the real Claude Code (native `claude.exe`) and Codex (the native `codex.exe` its npm launcher starts) has **not** been tested: the tests use a renamed copy of cmd.exe as a stand-in. The label can lag by up to 2 s and a very short program may never be seen.
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
  No `cargo fmt` or `clippy` gate yet.
- Workflow actions are pinned to major versions (`checkout@v7`, `setup-node@v7`, `rust-cache@v2`), not commit hashes.
- GitHub private vulnerability reporting (linked from the issue template and CONTRIBUTING) has been enabled in the
  repository settings by the owner; the report flow itself has not been exercised.
