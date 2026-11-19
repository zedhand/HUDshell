# Known gaps

Everything here is either untested or not implemented. Verified behaviour is in [test-results.md](test-results.md).

## Not yet tested (needs hardware, another setup, or a person)

| Item | What it takes |
|---|---|
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
| Real-prompt suite (`-Suite real`) after the latest changes | run when both CLIs have usage available |

## Not implemented

- Full settings UI (the panel covers opacity and colour theme; font, hotkeys and shell are edited in `config.json`).
- Live config reload (config is read at start); `WM_DPICHANGED` handling; WebView2 crash recovery; shell-exit restart.
- A friendly error when the shell cannot start (it is logged only).
- Autostart with Windows; the second launch bringing the first to the front (it just exits).
- OSC 8 link click policy and OSC 52 clipboard policy (neither is acted on nor blocked deliberately).
- Mouse selection extras (copy on select, right-click menu) and keyboard scrollback keys.
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
