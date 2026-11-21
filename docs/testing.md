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

**Before you run it:** the `ui`, `clickfocus`, `autotest`, `clipboard`, `hideshow`, `input` and `cost` suites send real hotkeys
(Ctrl+Alt+letter) and some move and click the real mouse (`input` also drags and resizes the window; `kbd` switches the keyboard layout to German and French while it runs and back at the end, so Windows shows it for every window meanwhile) or paste with the real keyboard, so do not use the
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
| `autotest` | typing, history, paste, Ctrl+C, Claude Code and Codex start and exit (with their real tab label while running), focus grab cycles, click-through, no focus stolen while passive |
| `ui` | opacity hotkeys and the 20% floor, the real ghost hotkey, corner / font / new-shell / switch / close / layout / pane / zoom hotkeys, settings panel keys and real mouse (click, drag, arrows, close, Esc) |
| `place` | corner docking, size presets, font hotkeys (and that the page applied the font) and magnetic snapping, through the host functions (no OS input) |
| `qol` | idle dim, active opacity, start options and the Start with Windows registry entry (test-only value name, removed afterwards) |
| `sessions` | several shells in one window: isolation, which one the host writes to, restart after exit, close (no OS input) |
| `shells` | tabs, profiles, switching, marks, titles, closing, picker, tab modes, the nine-shell limit (no OS input) |
| `shells` (labels part) | the tab shows a 35-character title whole; `tab_program` = known / all / off with a renamed copy of cmd.exe called `codex.exe` that only pings localhost (never a real Claude Code or Codex): the label appears, goes away when it ends, ordinary programs are labelled only in `all` mode (no OS input) |
| `input` | the real profile-picker hotkey and key, real mouse clicks on tabs, a real divider drag, real window drags (far from edges the window moves exactly; near the left or top edge it snaps to the margin), a real edge resize, and that none of it takes the keyboard |
| `cost` | memory (whole process tree and the HUDshell process) and idle CPU with 1, 3 and 9 shells; the limits it checks are sanity bounds I chose, not requirements |
| `text` | per-terminal text size (each shell keeps its own, reset, switching back), `text_scale_hotkeys = all`, `text_scale`, `line_height` / `letter_spacing` and `ui_scale` (no OS input); the real Ctrl+Alt+= / - presses are in `ui` |
| `reload` | settings reloaded while running (opacity, text scale, UI scale, tab mode, line height, letter spacing), an invalid text keeps the settings in use, and a start-only setting is reported as needing a restart (no OS input; the file watching itself and the tray item are not covered) |
| `restore` | `restore_session = shells`: a first run opens shells from three profiles in a 2 x 2 layout and quits (the session file holds them); the next start brings back the same tabs, layout and focused pane; with the setting off nothing is restored and the file is untouched; a damaged file is ignored (no OS input) |
| `recover` | the web view's browser process (only this instance's) is killed while two shells run: HUDshell creates the web view again, opens the shells in the new page, and what is typed into both afterwards shows up (no OS input) |
| `dpi` | moving the window between the primary monitor and a monitor with another scaling (needs one; skipped with a warning otherwise): window and page scaling follow, the logical size is kept, the corner hotkey docks with the scaled margin, and back (no OS input) |
| `snap` | real mouse drags on every monitor: 6 px from each corner the window snaps flush into it with no gap and its corners are square (screenshots of each corner and of the maximized window go to a git-ignored folder, `.claude/scratch-archive/shots`, and are never committed; the window can also be pulled away from an edge slowly); dragging to the top edge of a screen maximizes it to the whole monitor with square corners (rounded again after the restore), dragging the maximized window's strip down restores its size, a double click on the strip maximizes and restores; and dragging the window across the border to each other monitor puts it wholly there at that monitor's scaling and snaps into its corner |
| `kbd` | German and French layouts, switched on while it runs (Windows shares one layout between all windows, so they apply everywhere meanwhile) and switched back at the end with a check that they really are back (your language list is never edited): real key presses give the layout's letters, a dead key plus a letter gives the accented letter, the default hotkeys still fire, the AltGr clash is reported as a warning, and starting HUDshell with German active shows a notice naming the clashing hotkeys while the normal layout shows none |
| `layout` | split panes: each preset, new shells for empty panes, pane sizes, which pane gets typing, zoom, small-window fallback, closing and refilling a pane, `layout_fill = empty`, start layout, uneven split, dragging a divider with page mouse events, and `pane_shells = own` (new shells join the focused pane, strip / keys / close stay inside it, empty pane, merging into one pane) (no OS input) |
| `scrollback` | keyboard scrollback keys, checked from the viewport position after 200 lines (no OS input) |
| `ghost` | the ghost preset through the host functions: levels, restore, never brightens, pointer mode, leaving ghost (no OS input) |
| `attention` | bell / OSC 9 / OSC 777 cue, progress and cwd ignored, rate limit, not while focused, hidden path, text kept out of the log (no OS input) |
| `policy` | OSC 52, OSC 8, copy-on-select and right-click, driven by DevTools mouse events; temporarily uses the real clipboard and restores it |
| `release` | normal build runs from any folder, ignores test switches, single instance (a second launch brings the first window forward), tray icon, logging, opacity persists across restarts |
| `config` | invalid JSON, out-of-range values, bad hotkeys, hotkey conflicts, unknown keys, UTF-8 BOM, ghost clamp and not persisted, bad clipboard-policy values, bad layout values, `remember_layout` off / on (a real Ctrl+Alt+' press) |
| `soak` | five minutes of continuous output: memory stays flat |
| `real` | real prompts in a sandbox folder through Claude Code and Codex: a tool task, a two-line Shift+Enter prompt, a long stream with a mid-stream resize and Esc interrupt, `/exit` |

## Harness build and switches (`--features harness` only)

Scenario switches: `--autotest`, `--keys`, `--widthtest`, `--probe*`, `--real`, `--clickfocus`, `--hideshow`,
`--clipboard`, `--placetest`, `--idletest`, `--starttest`, `--autostarttest`, `--sessiontest`, `--shellstest`, `--tabmodes`, `--shellsfull`, `--layouttest`, `--layoutempty`, `--layoutstart`, `--layoutown`, `--labeltest`, `--inputtest`, `--dpitest` (with `--at X,Y --cross X,Y`), `--size W,H` (start size of `--inputtest`, 96-dpi pixels), `--texttest`, `--reloadtest`, `--recovertest`, `--startfailtest`, `--sessionsave` / `--sessionload` (with `--restore-session shells --session-file <path>`), `--scrolltest`, `--ghosttest`, `--attention`, `--policytest`, `--soak [seconds]`, `--selftest`, `--selftest2`, `--idle` (no script; for external drivers).
Modifiers: `--osc52 write`, `--right-click paste`, `--copy-on-select`, `--no-cue`, `--no-toast`, `--ghost-opacity <n>`, `--opacity <n>`, `--active-opacity <n>`, `--idle-dim <s>`, `--idle-opacity <n>`, `--start-hidden`, `--start-ghost`, `--tabs auto|always|overlay`, `--test-profiles`, `--layout <name>`, `--layout-fill new|empty`, `--pane-shells shared|own`, `--tab-program known|all|off`, `--at X,Y[,W,H]` (start the window there, physical pixels), `--text-scale <n>`, `--ui-scale <n>`, `--line-height <n>`, `--letter-spacing <n>`, `--text-hotkeys focused|all`, `--split <c>,<r>`, `--pane-min <c>,<r>`, `--dividers` (override the built-in test config),
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
