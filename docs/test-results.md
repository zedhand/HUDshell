# Test results

Machine: Windows 11 build 26300, WebView2 Runtime 157.x, Rust 1.99 (MSVC), PowerShell 7.7, Claude Code 2.1.328 (2.1.329 on 2026-11-20),
Codex CLI 0.170.0, three displays (all at 100% scaling). Latest full regression: 2026-11-20 with `scripts/test-all.ps1`.
"Tested" means observed in a log, a cell-text dump or a screenshot; anything not listed is in [known-gaps.md](known-gaps.md).

## CI check of the commits after the last full run (2026-11-21)

Commit 7411ade (live settings reload, session restore, web view recovery, start-failure message, AltGr warning, panel rows) was pushed before any
suite had been run on it. CI (the `static` suite on GitHub's `windows-latest`) failed once: one of the new unit tests, written by me, expected the
wrong pane order. After the fix (00765f1) CI passed: **66 unit tests pass** and the release exe is 1.19 MB. That is **all** that has run for
these features: no desktop suite (`reload`, `restore`, `recover`, `sessions`, `kbd`, `ui`, ...) has been run on them, so every behaviour they
describe is still unverified. The previous full run below is of the code before them.

## Latest full run (scripts/test-all.ps1, no sandbox, 2026-11-20, mixed scaling, text scaling, snapping and maximize): 394 pass, 0 fail, 3 warnings

Run on freshly built release and harness executables, on a machine with three monitors: 2560x1440 at 100% (primary), 1920x1080 at **150%**
and a 1920x480 strip at 100%. The real-prompt suite was **not** run, so no prompt went to Claude Code or Codex. The three warnings are the
two intentional `autotest` skips (same kitty-protocol guard as before) and one **known limit reported by `kbd`**: AltGr+Q, which types `@` on
a German layout, is also HUDshell's quit hotkey (Ctrl+Alt+Q), and the hotkey wins (see known-gaps and the README). Per suite: static 16 (58 unit
tests; release exe 1.13 MB), keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 22 (+2 skips), ui 29, scrollback 8, ghost 14,
attention 10, policy 12, place 24 (corners, size presets, font and snapping on the primary and on the 150% monitor), dpi 8, text 13, qol 16,
sessions 10, shells 34, layout 35, input 26 (the real-mouse suite on the primary and on the 150% monitor), snap 34, kbd 12 (+1 warning),
cost 5, release 20, config 24, soak 4. The settings file was restored byte-for-byte by `release` and `config`; soak memory 215 -> 216 MB.
The keyboard layout in use before the run was checked to be back after `kbd`, which switches it to German and French while it runs.

**New since the previous full run, with what each showed**

- **Mixed scaling (`dpi`, `place` and `input` on both monitors).** Moving the window between the 100% and 150% monitors keeps its logical size
  (700x420 becomes 1050x630 and back), the page switches scale with it, docking uses the scaled margin (12 px at 150%), and the real-mouse
  suite passes on both monitors. The window had no `WM_DPICHANGED` handling before; it was added for this.
- **Text scaling (`text`, 13 checks).** Each terminal keeps its own size (hotkeys change only the focused one, reset goes back, switching back
  shows the remembered size); `text_scale_hotkeys = all` changes every terminal; `text_scale = 150` shows 14 points as 21; line height and
  letter spacing reach the terminals and give fewer rows and columns; `ui_scale = 150` makes the tab strip 30 px high with 18 px text. The
  defaults leave HUDshell's own text pixel-identical to before.
- **Keyboard layouts (`kbd`).** With German and French active, real key presses gave each layout's letters (including accented ones and a dead
  key followed by a letter), and the default hotkeys still fired. The AltGr clash above is the one finding. Not covered: any other layout, IME.
- **Snapping, maximizing (`snap`, 34 checks, real mouse, every monitor).** A drag ending 6 px from each corner snaps flush on both axes with the
  scaled margin; dragging to the top edge of the screen maximizes the window to the work area; dragging the maximized window's strip down
  restores its size; a double click on the strip maximizes and restores; dragging across the border to each other monitor puts the window
  wholly there at that monitor's scaling and it then snaps into that monitor's corner.

**Defects found by these tests and fixed.** (1) *Snapping could not be pulled away from.* Windows proposes each rectangle of a drag relative to the one
last applied, so a window held by the snap distance (16 px) never escaped when moved in small steps; it only jumped out with big movements.
The first `snap` run showed it (the bottom corners of the narrow monitor never reached their target). Fixed by tracking the position the
window would have without snapping (`place::SnapDrag`, with unit tests). (2) The window could not be moved or resized with the mouse at all
before 2026-11-20 (see the section below). **Mistakes of mine on the way, kept for the record:** the first `snap` run also failed on two faults of my
test script (a phantom "other monitor" given to the non-primary monitors, and a crash reading the log while the app was writing it; the
reader now shares the file and retries); a first attempt to start a drag only after the cursor had moved made 10 checks fail (the window lagged
behind the cursor) and was reverted; and the `kbd` suite once left the keyboard on German because Windows shares one layout between all windows and the
suite did not switch it back at the end. It was restored by hand, and the suite now restores it and checks that it really is back.

**Not covered:** scalings other than 100% and 150%; dragging across the monitor border by hand (the tests drag with the real mouse by script);
the settings panel's real-mouse checks at 150% (they use pixel offsets for 100%); the real-prompt suite; layouts other than German and French, and
IME; sleep / resume; a clean machine, other Windows versions or a game. These stay in known-gaps.

## Previous full run (commit 65602f9, window-drag fix, before mixed scaling and snapping): 301 pass, 0 fail, 2 intentional skips

Run on freshly built release and harness executables after the fix described under "A defect found by `input`" below. The real-prompt suite
was **not** run (no sandbox given), so no prompt went to Claude Code or Codex. Per suite: static 16 (55 unit tests; release exe 1.12 MB),
keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 22 (+2 intentional skips), ui 29, scrollback 8, ghost 14, attention 10,
policy 12, place 13, qol 16, sessions 10, shells 34, layout 35, input 13, cost 5, release 20, config 22, soak 4. The settings file was
restored byte-for-byte by `release` and `config`; soak memory 219 -> 215 MB. The per-suite detail of the new suites is in the next
two sections.

## Previous full run (commit 074ba1b, split panes build, before the window-drag fix): 279 pass, 0 fail, 2 intentional skips

Run on freshly built release and harness executables after adding split panes (presets, zoom, per-pane shell lists), program labels on
tabs and whole-title tab names. The real-prompt suite was **not** run (no sandbox given), so no prompt went to Claude Code or Codex. Per
suite: static 16 (54 unit tests; release exe 1.12 MB), keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 18 (+2
intentional skips), ui 29, scrollback 8, ghost 14, attention 10, policy 12, place 13, qol 16, sessions 10, shells 34, layout 35,
release 20, config 22, soak 4. The settings file was restored byte-for-byte by `release` and `config`; soak memory 214 -> 214 MB.

- **New real-hotkey checks, all passing:** Ctrl+Alt+' stepped through side by side, stacked, grid and single; Ctrl+Alt+O moved the
  keyboard to the next pane; Ctrl+Alt+Z zoomed and unzoomed; the window never took focus. These three chords registered on this machine
  (no conflict notice). `config` also saved the layout only with `remember_layout` on (a real Ctrl+Alt+' press) and replaced bad layout
  values by the defaults.
- **Not covered:** dragging a pane divider with the real mouse (the `layout` suite drags with events inside the page), a non-100%
  display, and the real Claude Code and Codex for the program label: the `shells` tests use a renamed copy of cmd.exe called `codex.exe`
  that only pings localhost, so whether the real `claude.exe` and the `codex.exe` started by Codex's npm launcher are recognised is
  unverified.

**The path to this result.** The first full run of this build was 266 pass, 4 fail. Three `shells` checks failed because of a real
bug I had introduced: closing the shell in a single pane showed the first shell in tab order instead of its right-hand neighbour (the pane
refill ignored the neighbour chosen by the tab list); fixed, with a unit test. One `layout` check failed because of a wrong assumption in
the test (two shells bring up the tab strip, which costs one row) and a PowerShell variable-name slip in its message; the test was
corrected. After the fixes the no-input suites were rerun (95 pass), and then this full run.

## Real-input runs added after that (2026-11-20, same machine; commit 074ba1b plus the window-drag fix below)

Run for the items the full run did not cover. The real-prompt suite was still not run.

- **`autotest` (22 pass, 2 intentional skips):** with the real Claude Code and the real Codex running in a tab, the tab was labelled
  `claude` / `codex` (the Codex tab read `codex · <folder name>`); after each exited the label was gone. No prompt was sent and no dialog
  answered. The label is read from process names; the launcher chain Codex uses (npm shim, then its native binary) was recognised.
- **`cost` (5 pass):** whole process tree 193 MB with 1 shell, 254 MB with 3, 438 MB with 9 (about 31 MB per extra shell); the HUDshell
  process itself 4.2 / 4.5 / 5.5 MB; idle CPU over 10 s 1.1% / 0.8% / 2.0% of one core. One machine, 100% scaling, shells sitting at a
  prompt; busy shells cost more. The pass limits (under 1.5 GB, under 10% of one core) are my own sanity bounds.
- **`input` (13 pass after the fix):** real Ctrl+Alt+Shift+N opened the picker and took the keyboard, a real key 1 started a shell and the
  keyboard went back, Esc cancelled it; real clicks switched tabs; a real divider drag moved the split from 50% to 65%; real window drags
  moved the window by exactly the drag distance away from edges (40,30) and snapped it to the 8 px margin at the left and top edges; a real
  drag of the right edge made the window 60 px wider (900 -> 960); none of those took the keyboard.

**A defect found by `input`, present since the first release and now fixed.** The first `input` run failed its three window-drag checks
(the window did not move at all). A probe that sends no input showed why, and showed the same on the commit before this session: the web
view covers the whole window, so a real press on the top strip or an edge landed on the web view and never reached the host's drag / resize
logic; the window could only be moved with the corner hotkeys. The fix (see design.md, "Moving and resizing") passed on the second run. The
earlier statement that dragging was verified referred only to the foreground window not changing during a drag. A second `input` check
("never took the keyboard") failed once because earlier clicks in the same run had already grabbed the keyboard by design; the test now
releases it first.

The other suites were rerun on the fixed build in the full run above (301 pass).

## Earlier full run today (phases 1-3 build, 2026-11-20): 228 pass, 0 fail, 2 intentional skips

First full run after adding corners and size presets, edge snapping, font hotkeys, active and idle opacity, start options, second
launch, Start with Windows, several shells per window and tabs. Run on freshly built release and harness executables, Claude Code
2.1.329 and Codex CLI 0.170.0. **The real-prompt suite was not run** (no sandbox given), so this run sent no prompts to either CLI; the
last real-prompt run is the 2026-11-19 one below.

Per suite: static 16, keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 18 (+2 intentional skips, the same
kitty-protocol guard as before), ui 26, scrollback 8, ghost 14, attention 10, policy 12, place 13, qol 16, sessions 10, shells 25,
release 20, config 18, soak 4. The unit tests are now 30. The release exe is 1.07 MB.

- **New real-hotkey checks (first time run), all passing:** Ctrl+Alt+. docked the window in a corner and a second press moved it to the
  next corner, Ctrl+Alt+, went back, and the window never took focus; Ctrl+Alt+= / Ctrl+Alt+- changed the font size; Ctrl+Alt+N opened a
  second shell; Ctrl+Alt+PageUp / PageDown / 1 switched shells; Ctrl+Alt+W closed a shell only on the second press.
- **Second launch (first time run):** a second start of the normal build exited and brought the hidden first window forward.
- **Soak:** whole process tree 217 -> 219 MB after the first minute (five minutes of output); host process under 40 MB private.
- The settings file was restored byte-for-byte by both the `release` and `config` suites. Not covered by this run: the profile-picker
  hotkey with a real key press (it takes the keyboard; tested through the host function only), a real mouse click on a tab, and real
  mouse drags for edge snapping (tested through simulated move messages).

## Previous full run (scripts/test-all.ps1 -Sandbox ..., 2026-11-19, after the fixes below): 159 pass, 0 fail, 2 intentional skips

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
