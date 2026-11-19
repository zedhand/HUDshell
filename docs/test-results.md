# Test results

Machine: Windows 11 build 26300, WebView2 Runtime 157.x, Rust 1.99 (MSVC), PowerShell 7.7, Claude Code 2.1.328,
Codex CLI 0.170.0, three displays (all at 100% scaling). Latest full regression: 2026-11-19 with `scripts/test-all.ps1`.
"Tested" means observed in a log, a cell-text dump or a screenshot; anything not listed is in [known-gaps.md](known-gaps.md).

## Latest full run (scripts/test-all.ps1): 105 pass, 0 fail, 2 intentional skips

| Suite | Result |
|---|---|
| static | 15 / 15 pass (11 unit tests; release exe 0.99 MB; no test code or build-machine paths; UI embedded byte-for-byte) |
| keys, width, hideshow, clickfocus, clipboard | all pass (width: 19 of 19 glyph lines aligned; click-focus: 6 of 6 hit tests) |
| ui | 18 / 18 pass (opacity hotkeys, 20% floor, panel keys, real mouse click and drag, arrows, close, Esc) |
| release | 19 / 19 pass (runs from any folder, ignores test switches, single instance, opacity persists across restart) |
| config | 13 / 13 pass (invalid JSON, clamping, bad hotkey, hotkey conflict, unknown keys, UTF-8 BOM) |
| autotest | 18 / 18 pass, 2 intentional skips (Shift+Enter is only sent once a program has negotiated the kitty protocol) |
| soak (5 min continuous output) | 4 / 4 pass: host 4.2 -> 5.1 MB private; whole process tree 224 -> 213 MB |
| real (real prompts through both CLIs) | passed in an earlier run (below); not re-run after the latest changes |

## CI preparation (2026-11-19)

The three CI steps (`scripts/build-release.ps1`, `cargo build --features harness`, `scripts/test-all.ps1 -Suite static`)
were run locally: `static` passed 16 / 16 under both PowerShell 7 and Windows PowerShell 5.1. Running it under PowerShell 7
exposed a compile error in the `Add-Type` helper at the top of `test-all.ps1` (`Point` lives in `System.Drawing.Primitives`
there); it now picks the reference by edition and compiles in both shells. The suites that use that helper (ui, clickfocus,
and so on) were **not** re-run after this change. The workflow file itself has not run on GitHub, and the other suites were not run.

## Behaviour verified earlier

**Overlay and focus** (with a full-screen game focused): showing, resizing, dragging and passive clicks never
changed the foreground window; grab via hotkey took focus within about 12 ms in 25 of 25 tries and typing worked;
release returned the previous window within 250 ms; focus lost to another program returns the window to passive;
click-through hit tests pass both ways; a real click on the interactive terminal requests and receives focus.

**Terminal** (real PowerShell 7 through ConPTY): typing, Backspace, history recall, paste (single and multi-line),
Ctrl+C interrupting a running command, a 3000-line / 276 KB flood arriving complete with a 2000-line scrollback cap,
Unicode round trips (CJK, emoji, box drawing, accents), clean shutdown with no orphan shell or OpenConsole processes.

**Claude Code and Codex:** both render (banner, composer, status line), survive resizes while running (including
mid-stream), and exit cleanly. Real-prompt run in a trusted sandbox folder: each CLI completed a tool task (created and ran a
script), accepted a two-line Shift+Enter prompt and answered, streamed a long reply that rendered cleanly through a
mid-stream resize and was interrupted with Esc, and `/exit` returned to the prompt. Control-sequence census over that run:
synchronized output begin/end 816 / 816 (balanced), alternate screen on/off balanced, bracketed paste, focus events,
mouse reporting, window-title updates; neither CLI pushed kitty keyboard flags.

**Shift+Enter:** key bytes verified at the shell input (before negotiation `1b 0d`; after a kitty push `1b 5b 31 33 3b 32 75`; after a pop
`1b 0d`). ESC CR inserts a newline in Claude Code and Codex and is a harmless no-op in PSReadLine. Ctrl+J was rejected
(works in Claude Code, ignored by Codex). Full key-byte table: Enter 0d, Ctrl+Enter 0d, Alt+Enter 1b 0d, Backspace 7f,
Ctrl+Backspace 08, Tab 09, Shift+Tab 1b 5b 5a, Esc 1b, arrows 1b 5b 41-44, Ctrl+Left/Right 1b 5b 31 3b 35 44/43, Home 1b 5b 48,
End 1b 5b 46, Delete 1b 5b 33 7e, PageUp/PageDown 1b 5b 35/36 7e, Ctrl+J 0a, Ctrl+C 03, Ctrl+A 01, Alt+B 1b 62.

**Emoji width:** reproduced deterministically (U+2728 is 2 cells to conhost, 1 to xterm.js default rules); the Unicode 11
addon aligns all 19 test glyphs at every ConPTY flag value, so it is the default.

**Window and tray:** frameless window with rounded corners and no border line; all 10 hit-test zones return the expected
codes; geometry restored exactly on each of three displays (including negative coordinates) and clamped when off-screen;
tray icon added, right-click menu works (including a real physical right-click), icon re-added after a real Explorer restart;
a hotkey-conflict notification was seen on screen.

**Security posture:** DevTools, browser shortcuts (Ctrl+Shift+C/I, F12, Ctrl+P, F5, zoom), context menu, status bar, pinch zoom
and swipe navigation are disabled (a Ctrl+Shift+C bug that opened DevTools was found and fixed); navigation and new windows
are blocked; page messages from other origins are dropped.

**Resources** (release build, idle at the prompt): hudshell.exe about 3.4 MB private; the six WebView2 helper processes
about 155 MB private (about 150-190 MB while streaming); idle CPU about 0.4% of one core; streaming about 1.8-2.5% on average.

**Opacity over a full-screen game:** 100% / 60% / 30% checked by screenshot; the game never lost focus. At 60% the prompt is
clearly readable, at 30% it is dim but legible. The whole window fades, so text fades with the background.

## Bugs found by testing (all fixed)

Passive window became activatable after external focus loss; tray grab recorded HUDshell as the window to return to;
Ctrl+Shift+C opened DevTools; Shift+Enter submitted a prompt (it arrived as plain Enter); emoji width corrupted TUI redraws;
a `config.json` with a UTF-8 byte-order mark was rejected as invalid.
