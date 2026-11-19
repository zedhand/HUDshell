# Testing

## One command

```powershell
scripts\build-release.ps1                 # release exe (path-stripped)
cargo build --features harness            # harness build used by the scenario suites
scripts\test-all.ps1                      # everything except the real-prompt suite
scripts\test-all.ps1 -Suite ui,config     # selected suites
scripts\test-all.ps1 -Suite real -Sandbox C:\path\to\trusted\folder
```

The runner prints PASS / FAIL / WARN per check and exits with the number of failures.

**Before you run it:** the `ui`, `clickfocus`, `autotest`, `clipboard` and `hideshow` suites send real hotkeys
(Ctrl+Alt+letter) and some move and click the real mouse or paste with the real keyboard, so do not use the
keyboard or mouse while they run (the cursor, clipboard text and your settings file are restored afterwards).
`config` and `release` run the normal build against `%APPDATA%\HUDshell\config.json` (and send the real Ctrl+Alt+X / Ctrl+Alt+Q hotkeys): an
existing file is backed up and restored byte-for-byte, and files the tests created beside it are removed. `policy` temporarily replaces the
clipboard text and restores it. `scrollback`, `ghost` and `attention` send no OS input at all. `real` sends real prompts through Claude Code and Codex and uses a little usage.

## Suites

| Suite | What it proves |
|---|---|
| `static` | `cargo test`; the release exe is small, has no test-harness strings and no build-machine paths or user name; every UI file is embedded byte-for-byte |
| `keys` | the exact bytes sent to the shell for Enter, Shift+Enter, Backspace, Ctrl+C, Esc, arrows and more |
| `width` | emoji / CJK / box-drawing alignment between xterm.js and ConPTY; UI is served from memory; no stray navigation |
| `hideshow` | output produced while the window is hidden is all there after showing it |
| `clickfocus` | click-through vs interactive hit tests, a real click takes focus, grab/release cycles |
| `clipboard` | real Ctrl+V paste and Ctrl+Shift+C copy; no DevTools window opens |
| `autotest` | typing, history, paste, Ctrl+C, Claude Code and Codex start and exit, focus grab cycles, click-through, no focus stolen while passive |
| `ui` | opacity hotkeys and the 20% floor, the real ghost hotkey, settings panel keys and real mouse (click, drag, arrows, close, Esc) |
| `scrollback` | keyboard scrollback keys, checked from the viewport position after 200 lines (no OS input) |
| `ghost` | the ghost preset through the host functions: levels, restore, never brightens, pointer mode, leaving ghost (no OS input) |
| `attention` | bell / OSC 9 / OSC 777 cue, progress and cwd ignored, rate limit, not while focused, hidden path, text kept out of the log (no OS input) |
| `policy` | OSC 52, OSC 8, copy-on-select and right-click, driven by DevTools mouse events; temporarily uses the real clipboard and restores it |
| `release` | normal build runs from any folder, ignores test switches, single instance, tray icon, logging, opacity persists across restarts |
| `config` | invalid JSON, out-of-range values, bad hotkeys, hotkey conflicts, unknown keys, UTF-8 BOM, ghost clamp and not persisted, bad clipboard-policy values |
| `soak` | five minutes of continuous output: memory stays flat |
| `real` | real prompts in a sandbox folder through Claude Code and Codex: a tool task, a two-line Shift+Enter prompt, a long stream with a mid-stream resize and Esc interrupt, `/exit` |

## Harness build and switches (`--features harness` only)

Scenario switches: `--autotest`, `--keys`, `--widthtest`, `--probe*`, `--real`, `--clickfocus`, `--hideshow`,
`--clipboard`, `--scrolltest`, `--ghosttest`, `--attention`, `--policytest`, `--soak [seconds]`, `--selftest`, `--selftest2`, `--idle` (no script; for external drivers).
Modifiers: `--osc52 write`, `--right-click paste`, `--copy-on-select`, `--no-cue`, `--no-toast`, `--ghost-opacity <n>` (override the built-in test config),
`--delay <s>`, `--no-grab` (never send OS-level hotkeys or clicks), `--log-input` (log the bytes sent to
the shell), `--pty-flags <n>`, `--unicode default|11|graphemes`, `--shift-enter none|esc-cr|kitty`.
Environment: `HUDSHELL_TEST_DIR` (a scratch folder the scenarios `cd` into) and `HUDSHELL_SANDBOX` (a folder the CLIs
already trust, for `--real`). Any scenario switch uses built-in defaults, never reads or writes the real config and
adds no tray icon. The log is `hudshell-test.log` in the project root (read it as UTF-8).

## Safety properties of the harness

- Keystrokes and paste go to the page through DevTools-protocol key events; OS-level input is limited to the
  hotkeys above, a real Ctrl+V while HUDshell itself is the foreground window, and the mouse in `clickfocus`/`ui`.
- Click-through is verified with `WindowFromPoint`, not by clicking into another application.
- A foreground watchdog logs every foreground change per phase.
- `claude` / `codex` are started and exited only, unless `--real` is used. The script checks the screen first and
  skips (never answers) trust, login or update dialogs, never sends Shift+Enter unless a kitty keyboard negotiation
  was observed, and aborts the run if the PowerShell prompt is not back after an exit.
- Use a dead local proxy in the child shell if you need to start a CLI without any possibility of an API call.
