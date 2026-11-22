# Test results

Machine: Windows 11 build 26300, WebView2 Runtime 157.x, Rust 1.99 (MSVC), PowerShell 7.7, Claude Code 2.1.328 (2.1.329 on 2026-11-20),
Codex CLI 0.170.0, three displays (all at 100% scaling). Latest full regression: 2026-11-21 with `scripts/test-all.ps1`.
"Tested" means observed in a log, a cell-text dump or a screenshot; anything not listed is in [known-gaps.md](known-gaps.md).

## The finale (2026-11-22, `-Suite finale`): Chinese and Korean input methods, a second game with a controller, real sleep

One sitting run by the script: 36 checks passed, 2 failed, 2 warnings. Both failures were test faults (explained below), not HUDshell faults. Not run: the real-prompt suite (skipped on purpose), and nothing else of the full suite
(this was a targeted sitting; the last full run is the one further down).

- **Chinese and Korean input methods.** The language features were installed through one Windows prompt and removed again (the language list was back to what it was, and the removal was confirmed). Real key presses
  gave the characters Windows lists for the Russian, Swedish, Japanese, Chinese and Korean layouts. **Korean IME composition passed** (romaji-style keys arrived as Hangul in the terminal). **Chinese (Pinyin) and Japanese
  composition were not tested this time**: both IMEs stayed in direct input mode (plain letters arrived, also after the mode key), so the test warned. (Japanese composition had passed in an earlier sitting.)
  The keyboard layout and the layout list were restored (checked).
- **A second real game (borderless, a different one from the first sitting; its name is not recorded).** Passed: the game kept the keyboard when HUDshell appeared; HUDshell was drawn over it; show / hide and the
  click-through hotkey worked with the game in front (the click-through check that had failed through a test fault in the first sitting now passed); Ctrl+Alt+G took the keyboard and the second press gave it back to the
  same game window; the click-through hotkey twice with the game in front.
- **Controller (read-only XInput polling; nothing was sent to the controller or the game).** With HUDshell shown over the game and again while HUDshell held the keyboard, another program could still read the
  controller (both triggers were seen). The person said the game reacted to the controller while HUDshell was shown, and again after HUDshell took and gave back the keyboard. **Limits of that evidence:** the answers
  are the person's, and at the second grab the game window was not the foreground window (the terminal this session ran in was; the test did not check this, so the answers are not tied to the game having the
  keyboard). HUDshell gave the keyboard back to the window that had it, as designed (log: previous foreground was the terminal), which is why the check "the keyboard went back to the game window" failed: **a test
  fault** (it assumed the game was in front). The test now checks the foreground before each controller step and warns when it is not. The controller steps have **not** been re-run with the game verified in front.
  Controllers that Windows does not show as XInput pads were not tested.
- **Frame times (PresentMon 2.6.0, game not capped at 30 fps this time).** Without HUDshell: 3409 frames, 55 fps mean, 1% low 8.7 fps (a stall in that minute). HUDshell shown and idle: 3481 frames, 58 fps, 1% low 49.9 fps.
  HUDshell shown with a shell printing flat out: 3481 frames, 58 fps, 1% low 50.3 fps. **What this does and does not say:** HUDshell showed no frame-rate cost in these three minutes, but the measurement is not a clean
  comparison: the baseline minute had a stall of unknown cause, the two HUDshell runs have the identical frame count and mean (which looks like the game's own frame limit at about 58 fps), and it is one scene on one
  machine. It does not prove a cost of exactly zero. The game window may not have been in front the whole time (see above).
- **Real sleep.** Windows logged a real sleep and resume (Kernel-Power event 42; sleep 00:28:52, wake 00:29:06 local time): **14 seconds**, not the 2 minutes planned, because the network adapter woke the computer, not the
  wake-up timer. Through it: HUDshell kept running, both windows were still open and on a screen, both shells' heartbeats continued (window 1 from beat 19 to 82), and no web view failure was logged.
  The check "the clock jumped by more than 40 s" failed (34 s) because the sleep was short: **a test threshold fault**, now replaced by a check of Windows' own sleep record. A long sleep (minutes) was **not** tested.

## CI check of the commits after the last full run (2026-11-21)

Commit 7411ade (live settings reload, session restore, web view recovery, start-failure message, AltGr warning, panel rows) was pushed before any
suite had been run on it. CI (the `static` suite on GitHub's `windows-latest`) failed once: one of the new unit tests, written by me, expected the
wrong pane order. After the fix (00765f1) CI passed: **66 unit tests pass** and the release exe is 1.19 MB. That was **all** that had run for
these features at the time; the desktop suites were run afterwards (see the latest full run below).

## What was left over (2026-11-21, `-Suite remaining`): taskbar on the right, another screen at other scalings

Run by the script, nothing set by hand. **Taskbar on the right edge** (registry, restored byte for byte): `maxwork` 3, `place` 28 and the full real-mouse `snap` on all three monitors (64) passed;
this is also the first complete multi-monitor `snap` run after the corner-snap fix. **The left screen (150%) set to 125% and to 175%**: `dpi` 9 and `place` 28 each passed (the window keeps its logical size between a
100% and a 125% / 175% screen, the page follows, the web view covers the window, docking uses the scaled margin); at 100% `place` passed and `dpi` correctly reported that there was no second scaling to compare.
The screen's original 150% was put back and checked each time. The third (480 px high) screen offers no scaling above 100%.
**Not run:** the Chinese and Korean input methods (the elevated prompt for the language install was declined or not answered, so nothing was installed or changed), the WoW click-through re-check
(the game was not in front), and real sleep (the streaming program still blocks it).

## The rest of the manual tests (2026-11-21): layouts, IME, scalings, taskbar edges, a real game, sleep, real prompts

**Final full run after all of it (`scripts/test-all.ps1`, no sandbox): 539 pass, 0 fail, 3 warnings** (the two intentional `autotest` skips and the AltGr+Q limit; 80 unit tests; clippy clean; the taskbar was
visible at the bottom and the scaling 100% / 150% during the run). Per suite: static 16, keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 22, ui 33, scrollback 8, ghost 14, attention 10, policy 12,
place 28, dpi 9, text 13, reload 9, restore 7, recover 6, windows 19, winrestore 9, winmove 11, wvbounds 3, altmove 3, gamepos 8, tabsedit 6, tabsui 4, tileoff 2, costwin 5, qol 16, sessions 12, shells 34, layout 35,
input 26, snap 64, windowsnap 10, kbd 14, cost 5, release 20, config 24, soak 4.

One sitting run by the script (`-Suite everything`): it set the display scaling and the taskbar edge, loaded keyboard layouts and installed the Japanese language feature
itself, and put everything back (each restore was checked). The person only accepted Windows prompts and brought the game to the front.

- **Keyboard layouts (`kbdany`).** Russian (the Cyrillic keys typed the characters Windows lists for them) and Swedish passed; so did the Japanese keyboard layout. The Chinese and Korean layouts could not be
  switched on in the test window without changing settings (warnings). The layouts the test loaded were unloaded again.
- **Japanese IME.** With the Japanese language feature installed through an elevated prompt, typing `nihongo` and converting arrived in the terminal as the three characters of the word for Japanese
  (nihongo): real IME composition works in HUDshell. The language list was restored exactly (checked: en-CA before and after) and the feature removed through a second prompt (the removal itself was not
  separately verified). Chinese and Korean input methods: not tested.
- **Scalings set by the script (main screen at 175%, 200%, 225%; Windows offers nothing above 225% for it).** `place` passed (11 checks each, including the size hotkey that had failed at 200%) and `dpi` passed
  (9 checks each). The main screen was put back at 100% and checked each time. (A first pass of `dpi` and `snap` did not run: see the script defect below.)
- **Taskbar on the left and on the top (set through the registry; this Windows build honours it).** `maxwork` (3) and `place` (11) passed in both. `snap` passed 62 checks in each and failed 2 in each: a real
  defect, below. The original taskbar setting was restored and checked (byte for byte) both times.
- **A real game, borderless, found in front by itself (its name is not recorded).** Passed: HUDshell did not take the keyboard from the game when it appeared; it is drawn over the game (its area
  differs from the same area with it hidden); the show / hide hotkey works with the game in front; Ctrl+Alt+G takes the keyboard (typing reaches the terminal) and the second press gives it back to the very same game
  window; the click-through hotkey works twice with the game in front. One check failed because of my test (it started the window interactive, where click-through is off by design); it was fixed but **not re-run
  against the game** (the person was away). **Frame times** (PresentMon 2.6.0, signed by Intel; the scene was capped at 30 fps): without HUDshell 1940 frames, mean 33.33 ms (30.0 fps), 1% low 27.2 fps; HUDshell shown
  and idle 1740 frames, 33.33 ms, 1% low 27.1 fps; HUDshell shown with a shell printing flat out 1740 frames, 33.33 ms, 1% low 26.6 fps. No difference is visible, but a frame cap hides small costs.
- **Sleep and resume: not proven.** The request returned success but Windows entered away mode instead of sleeping: `powercfg /requests` (read only, elevated) showed a streaming program holding display, system
  and away-mode requests, and a USB audio stream in use. The shells and windows stayed healthy through that non-sleep (this says nothing about real sleep). The test now calls the power API with force; it needs
  that program closed and no audio.
- **Real prompts through Claude Code and Codex (`real`): 2 of 2 checks pass** (the run reached the end; no prompt was missing). The screen dumps were not re-read line by line.

**Defects found by this sitting.**
1. *Corner snapping pulled a window back when it was dragged past an edge to another screen (real, in HUDshell).* The corner zone was measured one-sidedly, so a window far beyond an edge (up and to the left of a monitor,
   on its way to the monitor next to it) still counted as near that corner and was pulled flush into it; and the screen used for the snap was chosen from where the window was, not from where the drag was heading. Seen as
   the window stuck in the top-left corner of the strip monitor when dragged from it to the primary monitor, with the taskbar visible at the bottom (the earlier runs with the taskbar hidden did not hit it). Fixed
   (distances are absolute, the screen is picked from the unsnapped position; unit-tested); `snap` on every monitor and `input` pass after it (the two remaining failures were test faults, item 3).
2. *A comparison in the `place` test aborted the rest of each scaling / taskbar state* (a script defect: it compared arrays at scalings other than 100%). That is why `dpi` and `snap` seemed to be missing in the first
   pass; fixed, and every suite in a state now runs on its own.
3. *Two test expectations were wrong:* the `input` check on the 150% monitor assumed the window keeps the 8 px margin at the left edge, but with the taskbar visible the window can be near the bottom and is then pulled into
   the corner as designed (the check now accepts that); the `snap` check that a window dropped on an edge with another monitor beside it is not tiled compared sizes, but the window can land on the other monitor and take its
   scaling (it now checks that nothing was tiled).

## Hardware session (2026-11-21): taskbar, scaling, real unplug; three defects found and fixed

One manual sitting (`scripts	est-all.ps1 -Suite hardware`, the person changing the taskbar and scaling in Windows' settings and unplugging a cable; the
taskbar-edge and sleep parts were skipped: this Windows cannot move the taskbar, and the script was not allowed to put the computer to sleep).
**55 pass, 7 fail**, all 7 in one state, explained below.

- **Taskbar visible at the bottom: 14 pass** (`maxwork` 3, `place` 11). With `maximize_area = work`, dragging to the top edge fills the work area and stops at the
  taskbar, and both half tiles are halves of the work area. Corner docking and size presets respect the work area.
- **Main screen at 125%: 19 pass** (`dpi`, `place`).
- **Main screen at 200%: 12 pass, 7 fail** (`place`; `dpi` passed). Six of the failures were a **test fault**: the test assumed the first corner press goes to the top-left,
  but with a window that fills most of a 200% screen the nearest corner is another one (now the test works out the nearest corner). One was a **real defect**:
  the largest size preset (1440 px at 200%) is cut to a work area that is 1344 px high when the taskbar is visible, and the size hotkey no longer recognised the
  window as being at that preset, so it never cycled on. Fixed (`place::fit_sizes`: presets are cut to the work area first and duplicates dropped), covered
  by a unit test. **Not re-run on a 200% screen**; `place` passes at 100% (40 pass with `qol`).
- **A real unplug of the 150% monitor: 10 pass** (`monitorunplug`). The windows that had been on it moved onto the primary at their logical size (700x420) and
  staggered (the fix from the first unplug), the window on the primary stayed, HUDshell kept running, and all windows were on screen after the cable went back.

**Black strips along two edges of a window (reported by the owner from a screenshot, 2026-11-21).** After a window was created (a detach, or a restore
opening several windows), the window that was already open could have its web view shrunk by 14 pixels in both directions, leaving black strips along its
right and bottom edges until the next resize. Reproduced by a new self-check that compares the web view's bounds with the window: "bounds 626x346 were not the
window's 640x360" in window 1 right after window 2 was created. **Cause (introduced with several windows):** while `CreateWindowEx` runs, Windows sends the new
window its first resize and frame messages, but the window was only added to the per-window table after the call returned, so those messages were handled as the
window that happened to be current: the new window's momentary client size (626x346) was applied to the other window's web view. **Fix:** a window is registered
at its first message (`WM_NCCREATE`). Two safety nets were added: the web view's bounds are compared with the window every 2 seconds and corrected (and logged) if
they differ, and `wvbounds` (resizes, docking, layouts, detach in two windows) checks 14 states and that the self-check never had to correct anything: 3 pass.
The earlier scaling-change theory did not reproduce (`dpi` now also checks the web view covers the window in every state: pass).

## Scrollback on moves, tab names and order, window places per program (2026-11-21): full run 528 pass, 0 fail, 3 warnings

Full run (`scripts/test-all.ps1`, no sandbox) on the build with these three features: **528 pass, 0 fail, 3 warnings** (the two intentional `autotest`
skips and the AltGr+Q limit). New or changed suites: tabsedit 6, tabsui 4, gamepos 8, winmove 11, costwin 5, tileoff 2; 77 unit tests; `cargo clippy -D warnings`
clean for both builds. The `real` suite was not re-run after these three features.

- **A moved shell takes its screen and scrollback along (`winmove`):** 80 numbered lines were printed in a shell before it was detached into a new window,
  and the new window had at least 50 lines of scrollback with the last line on screen; the same for a shell moved into an existing window. This uses
  the official `@xterm/addon-serialize` 0.14.0 (MIT, from the xterm.js project; pinned exactly, integrity hash in `package-lock.json`, the only change to the
  lockfile). Not tried: a full-screen program (alternate screen) running in the moved shell; a very large scrollback (the limit is the `scrollback` setting).
- **Tabs named by hand and moved (`tabsedit`, no OS input; `tabsui`, real mouse and keys):** a name given to a tab shows instead of the program's title; an empty
  name brings the automatic label back; control characters are dropped and the length is capped at 40; names and tab order are saved in the session and
  come back at the next start on the same tabs. With the real mouse, dragging the third tab onto the first moved it to the front, and a double-click on a
  tab followed by typing a name and Enter renamed it. **Two defects were found by the real-mouse test and fixed:** the browser never delivered a
  double-click because the tab strip is drawn again after the first press (now detected by hand), and the terminal took the keyboard back as soon as the
  click grabbed it, closing the name box (now it leaves an open name box alone). With per-pane shell lists dragging a tab does nothing (it says so).
- **Window places per program (`gamepos`, the program in front is faked; no OS input):** with `game_positions` on, a place given while program A was in front
  is applied again when A comes back (300,300), B has its own (900,500), a program with no place and the desktop shell leave the window alone, no program
  name is written to the log, and with the setting off (the default) nothing is remembered or moved. The real lookup of the program in front was checked for its
  shape only (a lower-case executable name without a folder). Not tested with a real game; only the first window is moved.
- **A real monitor unplug (`monitorunplug`, manual, 2026-11-21, 8 pass):** with three HUDshell windows open (two on the left 150% monitor, one on the primary), the
  person unplugged the left monitor's cable. HUDshell kept running, Windows reported one monitor fewer, both windows that had been on it were moved onto the primary
  monitor (the log says so for both), the window on the primary did not move, and after the cable was plugged back in all three windows were still on screen.
  **Found by that run and then changed:** the two rescued windows landed at exactly the same place and kept their physical size (1050x630 pixels from the 150%
  monitor, which is 1.5 times bigger on a 100% monitor). They now keep their logical size and sit along a diagonal (unit-tested in `place.rs`); **that change has not
  been run against a real unplug yet.** After the cable was plugged back the windows stayed where they had been put (they do not jump back).
- **A shell in the alternate screen (`altmove`, no OS input):** a full-screen program's screen (alternate buffer) and text survive a detach into a new window.
- **Cost of the extra window** (`costwin`) and **`edge_tiling = false`** (`tileoff`) are described above.

## Extras after several windows (2026-11-21): earlier full run 501 pass, 0 fail, 3 warnings

Built and tested after the several-windows work, then one full run on the final build (`scripts/test-all.ps1`, no sandbox): **501 pass, 0 fail,
3 warnings** (the two intentional `autotest` skips and the AltGr+Q limit). Per suite: static 16 (72 unit tests), keys 6, width 3, hideshow 3,
clickfocus 3, clipboard 3, autotest 22 (+2 skips), ui 33, scrollback 8, ghost 14, attention 10, policy 12, place 24, dpi 8, text 13, reload 9,
restore 7, recover 6, windows 19, winrestore 9, winmove 9, qol 16, sessions 12, shells 34, layout 35, input 26, snap 64, windowsnap 10, kbd 14, cost 5,
release 20, config 24, soak 4. Three displays, the second at 150%.

- **Hotkeys for "ghost all windows" and "show / hide all windows"** (`windows` suite): every window goes ghost / hidden and back.
- **Moving a shell into another existing window** (`winmove`): the active shell leaves window 2 for window 1 and keeps running (typed output shows
  there); a window's only shell takes its window with it; the shell keeps its start profile (a defect found by the code review below).
- **A window off every screen after a display change** (`winmove`, by a simulated `WM_DISPLAYCHANGE`): it comes back onto a screen. **Not tested with a
  real monitor unplugged.** A saved window whose place is on no screen (`winrestore`, a hand-edited session file) opens on a screen with its shells.
- **The colour theme reaches every window** (`winmove`): both pages report the same new background.
- **Holding Shift while dragging a window of a touching pair** (`windowsnap`, real mouse): only the dragged window moves.
- **Half-screen tiling** (`snap`, real mouse, every monitor): with the cursor dropped on the left or right edge of a screen the window fills that half
  of the monitor edge to edge with square corners, and dragging it away restores its size; on an edge where another monitor continues, nothing is
  tiled. Screenshots were looked at (kept local, not committed). `edge_tiling = false` turns it off (`tileoff`, real mouse, 2 checks: the window dropped on an outer screen edge keeps its size and nothing is tiled).
- **Settings-panel rows** (`ui`, real key presses): ghost opacity `[` `]`, tab style `t`, line height `;` `'`, letter spacing `n` `m`; each logs and
  saves. Their `[-]` / `[+]` buttons were not clicked by a test.
- **`maximize_area = work` cannot be told apart from `monitor` on this machine:** the work area equals the full monitor on all three displays (the taskbar is hidden), so only its settings parsing is tested.
- **`cargo clippy -D warnings`** is clean for the normal and the harness build (tests included) and now runs in CI.
- **Code review** (the `/code-review` skill at high effort, on the changes): one real defect and four smaller ones, all fixed: a shell that was
  detached or moved lost its start profile; secondary windows overwrote the first window's saved position; "on screen" was judged against the
  bounding box of all monitors; tiling happened at an edge where another monitor continues; windows dragged along as a group stayed displaced when the
  drop became a tile. One finding was not acted on: the `ui` checks send US-layout key codes (as the older checks already do).
- **Real prompts (`real`, run once after the full run, sandbox folder):** real prompts went through Claude Code and Codex CLI in HUDshell. The suite's own two checks passed (the run reached the end; it never aborted for a missing prompt). That run was started on the build with several windows, tiling and the new panel rows, so the two CLIs ran in a build with those features; the screen dumps were not re-read line by line this time, so only what the suite checks is claimed.
- **What several windows cost (`costwin`, no OS input, 2026-11-21):** the same four shells in one window: the process tree (HUDshell, its web view
  processes and the shells) holds **296 MB** (HUDshell itself 4.9 MB) and idle CPU is 1.4% of one core over 10 s; spread over four windows it
  holds **392 MB** (HUDshell 5.1 MB) and 1.7%. That is about 32 MB per extra window (each has a web view of its own). One run on one machine, in
  the test build; shells printing nothing. Frame-time impact next to a running game was not measured.
- **A test fault found on the way:** the `--inputtest` scenario quit by itself after 80 s, in the middle of the longer `snap` run; it now runs for 200 s.

## Several windows (2026-11-21, built and tested step by step)

Built in six steps; the full suite (`scripts/test-all.ps1`, no sandbox, real-prompt suite not run) was run after steps 1 and 2, and a final full run
after step 6. The second display was at 150% for all of it, so mixed scaling was covered again.

- **Step 1, per-window state (no visible change): 438 pass, 0 fail, 3 warnings.** Every per-window static in `main.rs` became a `PerWin` value
  (see design.md). The three warnings are the two intentional `autotest` skips and the known AltGr+Q limit.
- **Step 2, a web view per window and output routing: 434 pass, 4 fail.** The four were `sessions` checks: the test opens sessions that no window
  lists, and the new routing sent a window only the output of shells it lists. Fixed (such sessions go to the first window as before); `sessions`
  passes again.
- **Steps 3-6 (detach, active window and tray, snapping and groups, restore of all windows), final full run: 468 pass, 4 fail, 3 warnings.**
  Per suite: static 16 (72 unit tests), keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 22 (+2 skips), ui 28, scrollback 8, ghost 14,
  attention 10, policy 12, place 24, dpi 8, text 13, reload 9, restore 7, recover 6, **windows 17, winrestore 8**, qol 16, sessions 12, shells 34,
  layout 35, input 26, snap 52, **windowsnap 9**, kbd 11, cost 5, release 20, config 24, soak 4. The four failures:
  - **kbd (3 checks)**: the person running the test accidentally switched keyboard focus to another window while `kbd` was typing, so the typed
    characters went elsewhere. Re-run on its own with nothing touched: `kbd` passed in full (the keyboard layout was restored and checked).
  - **ui (1 check, "Ctrl+Alt+W needs a second press")**: a real regression of mine. The close code was split so a shell can leave a window without
    being killed, and the log line the test looks for (`shell 1 closed`) was reworded. It failed again on a clean re-run, so it was not interference.
    Fixed by restoring the line; `ui`, `windows`, `shells` and `sessions` then passed together (92 checks).
- **After a last change** (the colour theme, text and UI scale and reloaded settings are now sent to every window, not only the one they were made
  in): unit tests 72/0, and `static`, `reload`, `text`, `windows`, `winrestore`, `sessions`, `ui`, `config` re-run: 128 pass, 0 fail. This is **not** a
  fourth full run: the other suites were last run before that one change.

What the new suites showed (all measured, none assumed):
- **`windows` (17, no OS input):** detaching shell 2 gives two windows (shell 1 / shell 2); the moved shell keeps running and its earlier output is
  on screen in the new window (repaint worked); each shell's new output appears in its own window only; the next-window action works; a window's only
  shell cannot be detached; closing the last shell of window 2 closes it; with five shells four windows can be made and a fifth is refused; one of
  those detaches is started by a message from the page itself (the path a Shift+click on a tab takes); the new-shell hotkey adds a shell to the
  active window only; "ghost all" and "show / hide all" reach all four windows and undo.
- **`windowsnap` (9, real mouse):** a window dragged to 8 px from another ends flush against it (its left edge on the other's right edge, tops equal,
  size unchanged); dragging window 1 by 120,90 moves it by that amount and the touching window by the same amount (log: "1 touching window(s) move
  along"); dragging window 2 afterwards moves both again.
- **`winrestore` (8):** three windows (shells 1,2 / 3,5 / 4; profiles pwsh,pwsh / work,cmd / cmd) were saved with a position and size each, came back
  at the next start with 2, 2 and 1 shells and the same profiles in the same order, each within 2 px of where it was; with `restore_session` off one
  window with one shell starts.

**A defect the owner found by watching:** during the first `winrestore` run one restored window stayed a blank white rectangle and the run found
two windows instead of three. The log showed the window was created but its web view never finished starting: it was being created from inside
the first page's own message callback, and WebView2 cannot create a web view there. Fixed by opening restored windows (and windows detached by a
tab click) from a posted message; after that `winrestore` passed. Not tested: holding Shift while dragging to move a single window out of a group
(the code path exists), a monitor that is gone at restore, theme or text-size changes reaching a second window (the code sends them to every
window; only the one-window result was observed), and more than four windows.

## Previous full run (scripts/test-all.ps1, no sandbox, 2026-11-21, after the seven features, corner snapping and the maximize option): 406 pass, 0 fail, 6 warnings

Fresh release and harness builds, `scripts/test-all.ps1` without a sandbox; the real-prompt suite was **not** run. Per suite: static 16 (67 unit
tests; release exe 1.19 MB), keys 6, width 3, hideshow 3, clickfocus 3, clipboard 3, autotest 22 (+2 skips), ui 29, scrollback 8, ghost 14,
attention 10, policy 12, place 13, dpi (no check ran, see below), text 13, reload 9, restore 7, recover 6, qol 16, sessions 12, shells 34,
layout 35, input 13, snap 52, kbd 14 (+1 warning), cost 5, release 20, config 24, soak 4. Warnings: the two intentional `autotest` skips, the
known AltGr+Q limit reported by `kbd`, and three "no monitor with a different scaling found" (`place`, `dpi`, `input`).
**The machine now reports no monitor at a different scaling** (the earlier 150% monitor was not found), so this run did not test mixed scaling at
all and `place` / `input` ran on one scaling only; the mixed-scaling results in the section below are from the previous run and have not been repeated.

**Mixed scaling re-run the same day** (second display set back to 150%; `place`, `dpi`, `input`, `snap` only, not a full run): **110 pass, 0 fail,
0 warnings** (dpi 8, input 26, place 24, snap 52). The window keeps its logical size between the 100% and 150% monitors (700x420 becomes
1050x630), and the real-mouse corner drags, maximize, restore and double-click checks pass on all three monitors, including the 150% one.
Screenshots of the corners and the maximized window on the 150% monitor show right-angle corners and a window filling the monitor (kept local).

**The features pushed untested in 7411ade, now run:** live settings reload (`reload` 9/9: text and UI scale, tabs mode, opacity, line height and
letter spacing take effect, an invalid file keeps the settings in use, a start-only setting reports a restart notice), session restore (`restore`
7/7: four shells from three profiles and the 2 x 2 layout with the same panes and focus come back; off by default and leaves the file alone; a
damaged file is ignored), web view recovery (`recover` 6/6: this instance's browser process was killed, the web view was created again, and both
shells kept running with output shown in the new page), the start-failure message (`sessions`), the AltGr start notice (`kbd`: names Ctrl+Alt+Q
with a German layout active, silent otherwise) and the settings-panel rows (`ui`: the click on `[close]` works). The keyboard layout in use
before the run was checked to be back after `kbd`.

**Defects and test faults found, with what they were.** (1) *Test fault:* `restore` expected the panes in order 1,2,3,4, but the first run had
saved 3,1,2,4 (the focused shell takes the first pane) and the restore reproduced exactly that; the expectation was wrong, the product was right.
(2) *Test fault:* the `[close]` click position in `ui` was a guess (two lines added; there are four); corrected to the measured line. (3) *Product
change, not a defect:* see below.

**Corner snapping, square corners and maximize over the taskbar (2026-11-21).** The owner reported that a small window did not snap into corners
and that the corners stayed rounded when snapped or maximized. Cause: snapping stopped at the 8 px margin (reach 16 px), so a window never
reached the corner, and Windows 11 rounds the window's corners all the time. Now a drop within 3 x `snap_distance` + `snap_margin` of a corner
goes flush into it, the corners become square while the window touches a corner of the screen's usable area or is maximized (rounded again
after), and maximizing fills the whole monitor over the taskbar (`maximize_area = monitor`, default; `work` keeps the taskbar). Checked by `snap`
with real mouse drags into all four corners of each of the three monitors (flush within 1 px; the log reports "square"), a top-edge drag to
maximize (fills the monitor; square; rounded again after the restore), and by looking at screenshots of every corner and the maximized window,
including 8 x zooms of the corner pixels: the corners are right angles. The screenshots were kept local and are not part of the repository.
The `place` and `input` checks that expected the old 8 px gap in a corner were updated to expect no gap. **Not tested:** `maximize_area = work`
(only its settings parsing is unit-tested), the taskbar on another edge or auto-hidden, and a very small window that is nearly as wide as the zone.

## Previous full run (scripts/test-all.ps1, no sandbox, 2026-11-20, mixed scaling, text scaling, snapping and maximize): 394 pass, 0 fail, 3 warnings

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
