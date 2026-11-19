# Test results

Machine: Windows 11 build 26300, WebView2 Runtime 157.x, Rust 1.99 (MSVC), PowerShell 7.7, Claude Code 2.1.328,
Codex CLI 0.170.0, three displays (all at 100% scaling). Latest full regression: 2026-11-19 with `scripts/test-all.ps1`.
"Tested" means observed in a log, a cell-text dump or a screenshot; anything not listed is in [known-gaps.md](known-gaps.md).

## Latest full run (scripts/test-all.ps1 -Sandbox ..., 2026-11-19, after the fixes below): 159 pass, 0 fail, 2 intentional skips

Run on freshly built release and harness executables, including the real-prompt suite through both CLIs. Per suite: static 16,
keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 18 (+2 intentional skips), ui 20, scrollback 8, ghost 14,
attention 10, policy 12, release 19, config 18, soak 4, real 2, all passing. The settings file was byte-identical afterwards and no
`ABORT:` line was logged. The host clipboard read for right-click paste took 0 ms (no stall). No browser permission request was
logged. Whether a prompt appeared on screen during this run was **not** recorded by any test.

The table and notes below describe the **first** full run of the day (154 pass, 4 fail), kept because they show what was found and fixed.

### First full run: 154 pass, 4 fail (all four explained and fixed), 2 intentional skips

Run after adding keyboard scrollback, the ghost preset, the attention cue and the clipboard / link policies. It includes the
real-prompt suite through both CLIs (Codex had usage again).

| Suite | Result in that run |
|---|---|
| static | 16 / 16 pass (12 unit tests; release exe under 3 MB; no test code, build-machine paths or former project name; UI embedded byte-for-byte) |
| keys, width, hideshow, clickfocus, clipboard | all pass (keys 6, width 3 incl. 19 of 19 glyph lines aligned, hideshow 3, clickfocus 3 incl. 6 of 6 hit tests, clipboard 3) |
| autotest | 18 pass, 2 intentional skips (a step is skipped by design unless a program has negotiated the kitty protocol) |
| ui | 20 / 20 pass (now includes the real Ctrl+Alt+X ghost hotkey: dims to 40%, second press restores, never took focus) |
| scrollback (new) | 8 / 8 pass |
| ghost (new) | 14 / 14 pass |
| attention (new) | 10 / 10 pass |
| policy (new) | 11 / 11 pass |
| release | 19 / 19 pass |
| config | 15 pass, **3 fail** in that run; 18 / 18 pass after the fix below |
| soak (5 min continuous output) | 4 / 4 pass: whole process tree 212 -> 214 MB after the first minute |
| real (Claude Code 2.1.328 and Codex CLI 0.170.0) | run reached the end; its "never aborted" check **failed falsely** (see below); passes in the second run |

**The four failures, and what was done**

- **config (3 checks).** The runner removed leftover `config.json.bad-N` files only when no `config.json` had existed before the
  tests. My own earlier test-mode mistake (below) had created a `config.json`, so the other branch ran and left the file from the
  invalid-JSON case behind, which made three later "no .bad file" checks fail. The runner now removes only files that appeared during
  the tests and never one that was already there. The config suite was rerun alone afterwards: 18 / 18 pass and the settings file was
  byte-identical afterwards.
- **real "never aborted" (1 check).** The check searched the log with a case-insensitive `-match 'ABORT'`, which also matched the
  scenario's own echo of its step name `abort-unless-prompt|end`. The harness writes `ABORT:` only when it really aborts, and the log
  has no such line (verified with a case-sensitive search). The check now looks for `ABORT:` case-sensitively. The real suite was rerun after that fix and
  the check passed (second full run).

**Real prompts, read from the screen dumps of that run (sandbox folder):** both CLIs completed a tool task (the file existed already
and was run), accepted a two-line prompt typed with Shift+Enter (both lines visible in the composer before submit), streamed a long
reply that was interrupted with Esc ("Interrupted" in Claude Code, "Conversation interrupted" in Codex), and `/exit` returned to the
PowerShell prompt. Codex printed "Disconnected from this task. Any running work continues." on exit. The run happened inside a Claude
Code session, so Claude Code showed a warning about an inherited child-session marker; the CLIs' own behaviour was otherwise normal.
Codex reported 16% of its weekly limit left at that point.

**A test-mode mistake found while adding scenarios.** New scenario switches were not registered in the harness's test-mode list, so a
few early runs used the real settings file and saved to it (opacity, window geometry, a conflicting default hotkey). It was repaired by
hand, and the runner and harness now share a single list of scenario switches (`SCENARIO_FLAGS`), so a new scenario cannot forget to
be registered. The ghost and policy runs were checked afterwards to leave the settings file untouched.

## New in this round (what each test showed)

- **Keyboard scrollback** (normal screen only): Shift+PageUp/PageDown scroll a page, Ctrl+Shift+Up/Down a line, Ctrl+Shift+Home/End go to
  the top / bottom. Verified from the viewport position after 200 lines of output (view 0..172). Full-screen programs keep these keys.
  Not tested: with a real mouse wheel, or inside Claude Code / Codex.
- **Ghost preset** (default Ctrl+Alt+X; Ctrl+Alt+H, L and M were already taken on this machine): 40% opacity plus click-through, restored on
  the second press; never brightens a window already dimmer than the ghost level; forces click-through and then restores the previous
  pointer mode; leaves ghost when click-through is toggled, opacity is changed, or the keyboard is grabbed; the dimmed value is never
  written to `config.json` (checked with the real hotkey in a normal build). The tray menu item was added but not exercised by a test.
- **Attention cue**: bell, OSC 9 and OSC 777 `notify` pulse the window border when HUDshell does not hold the keyboard; OSC 9;4
  (progress) and 9;9 (working directory) are ignored; at most one cue per 3 s; a tray notification is requested instead when the
  window is hidden (the notification itself was **not** observed on screen: test mode does not show one); notification text never
  reaches the log. **Measured: in the real-prompt run (both CLIs, stock settings) the control-sequence census shows
no OSC 9 or OSC 777 notifications at all** (and no OSC 52), so with default settings this cue is **not triggered by Claude Code or
Codex**; it serves other programs, scripts and `Write-Host "`a"`-style bells. The census does not count BEL, so whether either CLI rings
the bell is unmeasured. The same census did show 8 OSC 8 hyperlinks, which is what the link policy below covers.
- **Clipboard and link policies** (all opt-in, driven by DevTools mouse events and the real clipboard): OSC 52 writes are refused by
  default and applied by the host when `osc52` is `write`; an OSC 52 read query is never answered; copy-on-select and right-click paste /
  copy work when enabled and do nothing by default; OSC 8 links are drawn but a click is a no-op (xterm.js's default would open a
  confirm dialog). **Not tested: a real click on a link.**
- **Browser permissions:** HUDshell now denies every WebView2 permission request and logs only its kind. No request was logged in
  the runs above. During one run a prompt about pasting clipboard text appeared on screen and could not be answered in time; its source
  was not identified (no WebView2 request was logged, and no Windows system prompt of that kind turned up in a search). See known gaps.

## CI preparation (2026-11-19)

The three CI steps (`scripts/build-release.ps1`, `cargo build --features harness`, `scripts/test-all.ps1 -Suite static`)
were run locally: `static` passed 16 / 16 under both PowerShell 7 and Windows PowerShell 5.1. Running it under PowerShell 7
exposed a compile error in the `Add-Type` helper at the top of `test-all.ps1` (`Point` lives in `System.Drawing.Primitives`
there); it now picks the reference by edition and compiles in both shells. The suites that use that helper (ui, clickfocus,
and so on) passed in the full run above. The workflow then ran on GitHub (`windows-latest`) after the push and passed all steps (first with checkout@v4 and setup-node@v4, then again after bumping them to v7).

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
Found in this round: the first default for the ghost hotkey (Ctrl+Alt+H) was already taken by another program on the test machine, so it
was changed to Ctrl+Alt+X; an OSC 52 write done through the page's clipboard API stalled when the window was passive (now done by the
host); new test scenarios were missing from the harness's test-mode list and touched the real settings file; the runner left a `.bad-N`
file behind when a settings file had existed; and a log check matched a step name instead of a real abort (all fixed above).
