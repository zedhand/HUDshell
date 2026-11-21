//! Test harness. Compiled ONLY with `--features harness`; a normal build contains none of this.
//! Scripted scenarios, DevTools-protocol key injection, real hotkey/mouse injectors, the
//! foreground watchdog and the developer command-line switches live here so they cannot ship
//! in a release build.
#![allow(dead_code)]

use super::*;
/// Scripted terminal scenario (`--term --selftest2`): (ms from start, action).
/// Actions: "w:<text>" write to the shell, "ctrlc", "dump:<label>", "resize:W,H" (96-dpi pixels), "quit".
const SCENARIO: &[(u64, &str)] = &[
    (3500, "w:claude --version\r"),
    (6500, "w:codex --version\r"),
    (10000, "w:Start-Sleep 30\r"),
    (11500, "ctrlc"),
    (13000, "w:echo AFTER_CTRL_C\r"),
    (14500, "dump:after-ctrl-c"),
    (14600, "resize:900,520"),
    (16500, "dump:after-resize"),
    (16600, "w:chcp; [Console]::InputEncoding.CodePage; [Console]::OutputEncoding.CodePage\r"),
    (18000, "w:Write-Output 'U1: ss \u{65E5}\u{672C}\u{8A9E} \u{1F600} \u{2500}\u{2502}\u{250C}\u{2510} \u{E9}'\r"),
    (19500, "dump:unicode-default-codepage"),
    (19600, "w:chcp 65001\r"),
    (21000, "w:Write-Output 'U2: ss \u{65E5}\u{672C}\u{8A9E} \u{1F600} \u{2500}\u{2502}\u{250C}\u{2510} \u{E9}'\r"),
    (22500, "dump:unicode-after-chcp-65001"),
    (22600, "w:1..3000 | ForEach-Object { 'line {0} {1}' -f $_, ('x'*80) }; 'FLOOD_DONE'\r"),
    (27500, "dump:after-flood"),
    (28000, "quit"),
];
static SCENARIO_IDX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static SCENARIO_T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub(crate) unsafe fn scenario_tick(hwnd: HWND) {
    let Some(t0) = SCENARIO_T0.get() else { return };
    let elapsed = t0.elapsed().as_millis() as u64;
    loop {
        let i = SCENARIO_IDX.load(Ordering::SeqCst);
        if i >= SCENARIO.len() || SCENARIO[i].0 > elapsed {
            break;
        }
        SCENARIO_IDX.store(i + 1, Ordering::SeqCst);
        let act = SCENARIO[i].1;
        log(&format!("SCENARIO t={}ms: {}", SCENARIO[i].0, act.replace('\r', "<CR>")));
        if let Some(text) = act.strip_prefix("w:") {
            pty::write(text.as_bytes().to_vec());
        } else if act == "ctrlc" {
            pty::write(vec![0x03]);
        } else if let Some(label) = act.strip_prefix("dump:") {
            log(&format!("SCENARIO dump label={label}"));
            wv::post("{\"t\":\"dump\"}");
        } else if let Some(dim) = act.strip_prefix("resize:") {
            let mut p = dim.split(',');
            let w: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(900);
            let h: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(520);
            let _ = unsafe {
                SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)
            };
        } else if act == "quit" {
            let _ = unsafe { DestroyWindow(hwnd) };
        }
    }
}

// ===================================================================================
// Full automated test: `--term --autotest [--delay SECONDS] [--no-grab]`
//  * typing/keys go through xterm.js via DevTools key events (no OS focus, nothing sent to the game)
//  * only the real hotkeys Ctrl+Alt+G / Ctrl+Alt+T use OS-level input (skipped by --no-grab)
//  * a watchdog logs every foreground-window change, labelled by phase
// Actions: phase:X fg cdp-focus type:T key:K paste:T w:RAW ctrlc dump:L resize:W,H label:L
//          skipif:a;b|label hotkey:G|T hit:through|self census summary quit
// ===================================================================================
const AUTOTEST: &[(u64, &str)] = &[
    (500, "phase:baseline"),
    (600, "fg"),
    (1500, "cdp-focus"),
    (1600, "phase:passive-keyboard"),
    (2500, "resize:900,520"),
    (3500, "type:cd $env:HUDSHELL_TEST_DIR"),
    (3900, "key:Enter"),
    (4600, "type:echo KEYTEST-1"),
    (4900, "key:Enter"),
    (5600, "type:echo abcXYZ"),
    (5900, "key:Backspace"),
    (6000, "key:Backspace"),
    (6100, "key:Backspace"),
    (6300, "key:Enter"),
    (7000, "type:echo recall-me"),
    (7300, "key:Enter"),
    (8000, "key:Up"),
    (8300, "key:Enter"),
    (9000, "paste:echo PASTE-SINGLE"),
    (9400, "key:Enter"),
    (10000, "paste:echo ML1\necho ML2"),
    (10500, "key:Enter"),
    (11200, "type:Start-Sleep 30"),
    (11500, "key:Enter"),
    (12500, "key:Ctrl+C"),
    (13300, "type:echo CTRLC-OK"),
    (13600, "key:Enter"),
    (14500, "dump:kbd-result"),
    (15000, "phase:claude"),
    (15100, "type:claude"),
    (15400, "key:Enter"),
    (22000, "dump:claude-start"),
    (22600, "skipif:trust this;do you trust;log in;sign in;login|claude-exit"),
    (22700, "resize:700,420"),
    (24500, "dump:claude-after-resize"),
    (24700, "resize:900,520"),
    (26000, "dump:claude-after-resize-back"),
    (26200, "census"),
    (26250, "skipunless:kitty|claude-exit"),
    (26300, "type:line1"),
    (26600, "key:Shift+Enter"),
    (26900, "type:line2"),
    (27500, "dump:claude-multiline"),
    (28000, "label:claude-exit"),
    (28100, "key:Ctrl+C"),
    (28800, "dump:claude-ctrlc-1"),
    (29100, "key:Ctrl+C"),
    (29400, "key:Ctrl+C"),
    (31500, "dump:claude-exited"),
    (31900, "dump:claude-exited2"),
    (32000, "phase:codex"),
    (32100, "type:codex"),
    (32400, "key:Enter"),
    (40000, "dump:codex-start"),
    (40600, "skipif:trust;sign in;log in;login;update available;press enter|codex-exit"),
    (40650, "skipunless:kitty|codex-exit"),
    (40700, "type:line1"),
    (41000, "key:Shift+Enter"),
    (41300, "type:line2"),
    (42300, "dump:codex-multiline"),
    (42600, "label:codex-exit"),
    (42700, "key:Ctrl+C"),
    (43400, "dump:codex-ctrlc-1"),
    (43700, "key:Ctrl+C"),
    (44000, "key:Ctrl+C"),
    (46000, "dump:codex-exited"),
    (46400, "dump:codex-exited2"),
    (47000, "phase:grab-cycles"),
    (47100, "fg"),
    (48500, "hotkey:G"),
    (49500, "fg"),
    (49700, "type:echo GRAB-1"),
    (50000, "key:Enter"),
    (51000, "hotkey:G"),
    (51700, "fg"),
    (52500, "hotkey:G"),
    (53500, "fg"),
    (53700, "type:echo GRAB-2"),
    (54000, "key:Enter"),
    (55000, "hotkey:G"),
    (55700, "fg"),
    (56500, "hotkey:G"),
    (57500, "fg"),
    (57700, "type:echo GRAB-3"),
    (58000, "key:Enter"),
    (59000, "hotkey:G"),
    (59700, "fg"),
    (60000, "dump:after-grab-cycles"),
    (60500, "phase:click-through"),
    (60600, "hit:through"),
    (61300, "hotkey:T"),
    (62000, "hit:self"),
    (62300, "hotkey:T"),
    (62900, "hit:through"),
    (63000, "phase:end"),
    (62900, "census"),
    (63000, "summary"),
    (63500, "quit"),
];
/// `--keys`: safe key-encoding table. Keys are sent at an EMPTY PowerShell prompt only (no CLI is
/// started); with `--log-input` each key's exact bytes are logged ("PTY-IN").
const KEYS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "key:Enter"),
    (3000, "key:Shift+Enter"),
    (3500, "key:Ctrl+Enter"),
    (4000, "key:Alt+Enter"),
    (4500, "key:Backspace"),
    (5000, "key:Ctrl+Backspace"),
    (5500, "key:Tab"),
    (6000, "key:Shift+Tab"),
    (6500, "key:Esc"),
    (7000, "key:Up"),
    (7500, "key:Down"),
    (8000, "key:Left"),
    (8500, "key:Right"),
    (9000, "key:Ctrl+Left"),
    (9500, "key:Ctrl+Right"),
    (10000, "key:Home"),
    (10500, "key:End"),
    (11000, "key:Delete"),
    (11500, "key:PageUp"),
    (12000, "key:PageDown"),
    (12500, "key:Ctrl+J"),
    (13000, "key:Ctrl+C"),
    (13500, "key:Ctrl+A"),
    (14000, "key:Alt+B"),
    (14500, "key:Ctrl+C"),
    (15000, "w:[Console]::Write(\"$([char]27)[>0q$([char]27)[>1u$([char]27)[?u\")\r"),
    (15600, "dump:after-kitty-cmd"),
    (16000, "key:Shift+Enter"),
    (16250, "key:Ctrl+C"),
    (16500, "w:[Console]::Write(\"$([char]27)[<u\")\r"),
    (17000, "key:Ctrl+C"),
    (17500, "key:Shift+Enter"),
    (18200, "census"),
    (18500, "quit"),
];

/// `--widthtest`: for each glyph print "<glyph> memory runs off-plan", read conhost's cursor column,
/// move back 8 columns by that column and overwrite with XXXXXXXX. If xterm.js and ConPTY agree on the
/// glyph width every line ends "... runs XXXXXXXX |cl=N"; a disagreement shifts the X's.
const WIDTH: &[(u64, &str)] = &[
    (2000, "resize:900,520"),
    (3000, "w:[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $g=@([string][char]0x2728,[string][char]0x2713,[string][char]0x25C6,[string][char]0x25CB,[string][char]0x276F,[string][char]0x25CF,[string][char]0x273B,[string][char]0x23F5,[string][char]0x26A0,[string][char]0x25D0,[string][char]0x2733,[string][char]0x2026,[string][char]0x2014,[char]::ConvertFromUtf32(0x1F600),[string][char]0x2500,[string][char]0xE9,[string][char]0x65E5,([string][char]0x26A0+[char]0xFE0F),([string][char]0x2764+[char]0xFE0F)); foreach($x in $g){ [Console]::Write($x+' memory runs off-plan'); $l=[Console]::CursorLeft; [Console]::SetCursorPosition($l-8,[Console]::CursorTop); [Console]::Write('XXXXXXXX'); [Console]::WriteLine(' |cl='+$l) }\r"),
    (7500, "dump:width"),
    (8000, "quit"),
];

/// `--probe`: start claude only, log every byte sent to it (use with --log-input), then exit.
const PROBE: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "type:cd $env:HUDSHELL_TEST_DIR"),
    (2800, "key:Enter"),
    (3500, "type:claude"),
    (3800, "key:Enter"),
    (12000, "dump:claude-start"),
    (12500, "census"),
    (13000, "key:Ctrl+C"),
    (13700, "key:Ctrl+C"),
    (14000, "key:Ctrl+C"),
    (16000, "quit"),
];

/// `--probe2`: guarded newline-key test inside a live Claude Code. The child shell gets a dead API
/// base URL and a dead proxy (process-local env only), so even a mistaken submit cannot reach the API.
/// Each candidate newline key is typed between two short markers and the screen is dumped.
const PROBE2: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "w:$env:ANTHROPIC_BASE_URL='http://127.0.0.1:9'; $env:HTTPS_PROXY='http://127.0.0.1:9'; $env:HTTP_PROXY='http://127.0.0.1:9'; cd $env:HUDSHELL_TEST_DIR\r"),
    (3500, "type:claude"),
    (3800, "key:Enter"),
    (12000, "dump:claude-start"),
    (12500, "type:AAA1"),
    (12900, "key:Ctrl+J"),
    (13300, "type:BBB1"),
    (14200, "dump:after-ctrl-j"),
    (14500, "key:Ctrl+C"),
    (15300, "dump:after-clear-1"),
    (15600, "type:AAA2"),
    (16000, "key:Alt+Enter"),
    (16400, "type:BBB2"),
    (17300, "dump:after-esc-cr"),
    (17600, "key:Ctrl+C"),
    (18400, "dump:after-clear-2"),
    (18700, "type:AAA3"),
    (19100, "key:Shift+Enter"),
    (19500, "type:BBB3"),
    (20400, "dump:after-shift-enter"),
    (20700, "key:Ctrl+C"),
    (21500, "key:Ctrl+C"),
    (21900, "key:Ctrl+C"),
    (22300, "key:Ctrl+C"),
    (24500, "w:Remove-Item Env:ANTHROPIC_BASE_URL,Env:HTTPS_PROXY,Env:HTTP_PROXY\r"),
    (25500, "dump:exited"),
    (26000, "census"),
    (26500, "quit"),
];

/// `--clickfocus`: pointer-mode and focus behaviour with the built-in defaults. Starts click-through
/// (default); switches to interactive; a REAL click on the terminal must request keyboard focus;
/// Ctrl+Alt+G must make a click-through window temporarily clickable while focused and restore it.
const CLICKFOCUS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "phase:default-state"),
    (2600, "fg"),
    (2700, "hit:through"),
    (3500, "phase:click-to-focus"),
    (3600, "hotkey:T"),
    (4300, "hit:self"),
    (4600, "fg"),
    (4800, "realclick"),
    (5800, "fg"),
    (6000, "type:echo CLICKFOCUS-OK"),
    (6300, "key:Enter"),
    (7300, "dump:after-click-focus"),
    (7600, "hotkey:G"),
    (8600, "fg"),
    (8800, "hit:self"),
    (9300, "phase:grab-from-click-through"),
    (9400, "hotkey:T"),
    (10100, "hit:through"),
    (10400, "hotkey:G"),
    (11400, "hit:self"),
    (11600, "fg"),
    (11800, "hotkey:G"),
    (12800, "hit:through"),
    (13000, "fg"),
    (13400, "phase:end"),
    (13500, "summary"),
    (14000, "quit"),
];

/// `--hideshow`: output keeps flowing into a HIDDEN window and is all there after it is shown again.
const HIDESHOW: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "resize:900,520"),
    (3000, "type:1..40 | ForEach-Object { 'line ' + $_; Start-Sleep -Milliseconds 150 }"),
    (3400, "key:Enter"),
    (4200, "hotkey:V"),
    (5000, "phase:hidden"),
    (11000, "hotkey:V"),
    (11800, "phase:shown"),
    (12500, "dump:after-show"),
    (13000, "census"),
    (13500, "quit"),
];

/// `--clipboard`: real Ctrl+V paste from the Windows clipboard (needs the clipboard pre-filled by the
/// caller with 'echo CLIPBOARD-PASTE-OK') and Ctrl+Shift+C copy of the screen selection.
const CLIPBOARD: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "hotkey:G"),
    (3600, "fg"),
    (3800, "realkey:Ctrl+V"),
    (4800, "key:Enter"),
    (5800, "dump:after-paste"),
    (6000, "select-all"),
    (6500, "key:Ctrl+Shift+C"),
    (7000, "key:Ctrl+Shift+I"),
    (7500, "hotkey:G"),
    (8500, "quit"),
];

/// `--scrolltest`: keyboard scrollback. 200 numbered lines, then Shift+PageUp / Ctrl+Shift+Home / Down /
/// Shift+PageDown / End / Up; each step dumps the viewport position `view=[top, bottom]`.
const SCROLLBACK: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "w:1..200 | ForEach-Object { 'sb-line-' + $_ }\r"),
    (6000, "dump:sb-bottom"),
    (6500, "key:Shift+PageUp"),
    (7000, "dump:sb-pageup"),
    (7500, "key:Ctrl+Shift+Home"),
    (8000, "dump:sb-top"),
    (8500, "key:Ctrl+Shift+Down"),
    (9000, "dump:sb-line-down"),
    (9500, "key:Shift+PageDown"),
    (10000, "dump:sb-pagedown"),
    (10500, "key:Ctrl+Shift+End"),
    (11000, "dump:sb-end"),
    (11500, "key:Ctrl+Shift+Up"),
    (12000, "dump:sb-line-up"),
    (12500, "key:PageUp"),
    (13000, "dump:sb-plain-pageup-does-not-scroll"),
    (13500, "quit"),
];

/// Next session id the harness hands out (session 1 always exists).
static NEXT_TEST_SESSION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// `--sessiontest`: several shells in one window. Isolation between sessions, which one the host writes to,
/// restart after a shell exits, and closing a session.
const SESSIONS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "w:echo SESSION-ONE-ALIVE\r"),
    (3500, "session:new"),
    (6500, "wto:2:echo SESSION-TWO-ALIVE\r"),
    (7500, "dumps:1:s1"),
    (8000, "dumps:2:s2"),
    (8500, "session:show:2"),
    (9000, "w:echo ACTIVE-WRITE-WENT-TO-TWO\r"),
    (10000, "dumps:1:s1b"),
    (10500, "dumps:2:s2b"),
    (11000, "wto:2:exit\r"),
    (13000, "dumps:2:s2-exited"),
    (13500, "key:Enter"),
    (16500, "wto:2:echo AFTER-RESTART\r"),
    (17500, "dumps:2:s2-restarted"),
    (18000, "session:close:2"),
    (19000, "w:echo BACK-ON-ONE\r"),
    (20000, "dumps:1:s1-final"),
    (20500, "quit"),
];

/// `--shellstest` (run with `--test-profiles`): tabs, profiles, switching, marks, titles, closing.
const SHELLS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "w:echo ONE-ONE\r"),
    (3500, "dumps:1:single"),
    (4000, "shell:new"),
    (7500, "dumps:2:two"),
    (8000, "shell:new:work"),
    (11500, "w:(Get-Location).Path\r"),
    (12500, "dumps:3:work"),
    (13000, "wto:3:$Host.UI.RawUI.WindowTitle='TITLE-TEST'\r"),
    (14000, "shell:new:cmd"),
    (18500, "dumps:4:cmd"),
    (19000, "shell:jump:1"),
    (19500, "dumps:1:jump1"),
    (20000, "wto:2:echo BG-OUTPUT\r"),
    (20800, "dumps:1:marks"),
    (21200, "shell:attention"),
    (22000, "dumps:2:attn"),
    (22500, "shell:prev"),
    (23000, "shell:prev"),
    (23500, "dumps:4:wrapback"),
    (24000, "shell:next"),
    (24500, "dumps:1:wrapfwd"),
    (25000, "js:document.querySelectorAll('.tab')[2].dispatchEvent(new MouseEvent('mousedown',{bubbles:true}))"),
    (26000, "dumps:3:clicked"),
    (26500, "shell:list"),
    (26900, "dumps:3:listed"),
    (28000, "shell:close"),
    (28400, "dumps:3:confirm"),
    (28800, "shell:close"),
    (29800, "dumps:4:closed"),
    (30200, "shell:picker"),
    (30700, "dumps:4:picker"),
    (31000, "key:2"),
    (34500, "dumps:5:picked"),
    (35000, "shell:close"),
    (35400, "shell:close"),
    (36500, "shell:close"),
    (36900, "shell:close"),
    (38000, "shell:close"),
    (38400, "shell:close"),
    (39500, "dumps:1:one-left"),
    (40000, "shell:close"),
    (40400, "shell:close"),
    (44000, "w:echo AFTER-LAST-CLOSE\r"),
    (45000, "dumps:1:lastclose"),
    (45500, "quit"),
];

/// `--tabmodes` (run once each with `--tabs auto|always|overlay`): when the shell list shows.
const TABMODES: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "dumps:1:mode-one"),
    (2500, "shell:new"),
    (6000, "dumps:2:mode-two"),
    (6500, "shell:next"),
    (7000, "dumps:1:mode-next"),
    (10000, "dumps:1:mode-later"),
    (10500, "quit"),
];

/// `--shellsfull`: the window holds nine shells and refuses the tenth.
const SHELLSFULL: &[(u64, &str)] = &[
    (2000, "shell:new"),
    (2800, "shell:new"),
    (3600, "shell:new"),
    (4400, "shell:new"),
    (5200, "shell:new"),
    (6000, "shell:new"),
    (6800, "shell:new"),
    (7600, "shell:new"),
    (12000, "dumps:9:nine"),
    (12500, "shell:new"),
    (13200, "dumps:9:tenth-refused"),
    (13700, "quit"),
];

/// `--layouttest`: split panes through the host functions (no OS input). Presets in hotkey order, new shells
/// for empty panes, which pane gets typing, zoom, jumping to a shell already on screen, the fallback to fewer
/// panes in a small window, closing a shell that is on screen, a new shell filling the empty pane.
const LAYOUT: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (2500, "w:echo PANE-ONE\r"),
    (3500, "dumps:1:l-single"),
    (4000, "layout:next"),
    (8000, "wto:2:echo PANE-TWO\r"),
    (9000, "dumps:1:l-side"),
    (9500, "dumps:2:l-side2"),
    (10000, "layout:next"),
    (11000, "dumps:1:l-stacked"),
    (11500, "layout:next"),
    (16000, "dumps:1:l-grid"),
    (16500, "pane:next"),
    (17000, "w:echo TYPED-IN-PANE-TWO\r"),
    (18000, "dumps:2:l-focus2"),
    (18500, "pane:zoom"),
    (19000, "dumps:2:l-zoom"),
    (19500, "pane:zoom"),
    (20000, "dumps:2:l-unzoom"),
    (20500, "shell:jump:1"),
    (21000, "dumps:1:l-jump"),
    (21500, "resize:480,300"),
    (23000, "dumps:1:l-small"),
    (23500, "resize:900,520"),
    (25000, "dumps:1:l-big"),
    (25500, "shell:close"),
    (26000, "shell:close"),
    (27500, "dumps:2:l-closed"),
    (28000, "shell:new"),
    (32000, "dumps:5:l-refill"),
    (32500, "layout:next"),
    (33500, "dumps:5:l-single-again"),
    (34000, "quit"),
];

/// `--labeltest` (run once each with `--tab-program known|all|off`): the tab shows the program's whole title, and the
/// program running in the shell. `codex.exe` here is a renamed copy of cmd.exe that only pings localhost, so no real
/// program is started and nothing is sent anywhere.
const LABELS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "w:$Host.UI.RawUI.WindowTitle='HUDshell multi-shell phase 4 labels'\r"),
    (3000, "dumps:1:p-title"),
    (3500, "w:Copy-Item \"$env:SystemRoot\\System32\\cmd.exe\" \"$env:HUDSHELL_TEST_DIR\\codex.exe\"\r"),
    (4500, "w:& \"$env:HUDSHELL_TEST_DIR\\codex.exe\" /c \"ping -n 9 127.0.0.1 >nul\"\r"),
    (8500, "dumps:1:p-codex"),
    (15500, "dumps:1:p-after"),
    (16000, "w:ping -n 9 127.0.0.1\r"),
    (20000, "dumps:1:p-ping"),
    (27500, "dumps:1:p-after2"),
    (28000, "quit"),
];

/// `--layoutown` (run with `--pane-shells own`): each pane has its own list of shells. New shells join the focused
/// pane, switching and jumping stay inside it, closing refills from its own list, fewer panes merge the lists.
const LAYOUTOWN: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (2500, "w:echo OWN-ONE\r"),
    (3500, "layout:next"),
    (8000, "dumps:2:o-side"),
    (8500, "shell:new"),
    (12500, "dumps:3:o-joined"),
    (13000, "shell:prev"),
    (13800, "dumps:1:o-prev"),
    (14300, "pane:next"),
    (14800, "shell:jump:2"),
    (15300, "shell:jump:1"),
    (15800, "dumps:2:o-jump"),
    (16300, "shell:new"),
    (20300, "dumps:4:o-second-tab"),
    (20800, "shell:close"),
    (21200, "shell:close"),
    (22500, "dumps:2:o-closed"),
    (23000, "shell:close"),
    (23400, "shell:close"),
    (24500, "dumps:1:o-last-closed"),
    (25000, "layout:set:single"),
    (26000, "dumps:1:o-merged"),
    (26500, "quit"),
];

/// `--layoutempty` (run with `--layout-fill empty`): a layout leaves panes empty; a new shell goes into the
/// focused empty pane.
const LAYOUTEMPTY: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (2500, "layout:set:grid"),
    (3500, "dumps:1:e-grid"),
    (4000, "pane:next"),
    (4500, "dumps:1:e-focus-empty"),
    (5000, "shell:new"),
    (9000, "dumps:2:e-filled"),
    (9500, "quit"),
];

/// `--layoutstart` (run with `--layout side-by-side --split 60,50 --dividers`): the start layout, an uneven
/// split, and dragging the divider (DevTools mouse events inside the page, no OS input).
const LAYOUTSTART: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (6500, "dumps:1:st-start"),
    (7000, "js:(()=>{const d=document.querySelector('.divider.v');const r=document.getElementById('term').getBoundingClientRect();const y=r.top+20;d.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,clientX:r.left+r.width*0.6,clientY:y}));window.dispatchEvent(new MouseEvent('mousemove',{clientX:r.left+r.width*0.7,clientY:y}));window.dispatchEvent(new MouseEvent('mouseup',{clientX:r.left+r.width*0.7,clientY:y}));})()"),
    (8500, "dumps:1:st-dragged"),
    (9000, "quit"),
];

/// `--placetest`: corner docking, size presets, font size and magnetic snapping through the host functions.
const PLACE: &[(u64, &str)] = &[
    (1500, "resize:900,520"),
    (2000, "state:start"),
    (2300, "corner:next"),
    (2800, "state:c1"),
    (3100, "corner:next"),
    (3600, "state:c2"),
    (3900, "corner:next"),
    (4400, "state:c3"),
    (4700, "corner:next"),
    (5200, "state:c4"),
    (5500, "corner:prev"),
    (6000, "state:back"),
    (6300, "size"),
    (6800, "state:size1"),
    (7100, "size"),
    (7600, "state:size2"),
    (7900, "size"),
    (8400, "state:size3"),
    (8600, "dump:font-base"),
    (8700, "font:+"),
    (9000, "font:+"),
    (9300, "state:font-up"),
    (9900, "dump:font-up"),
    (10200, "font:-"),
    (10500, "state:font-down"),
    (11000, "dump:font-down"),
    (11300, "moving:5,7"),
    (11600, "moving:400,300"),
    (12000, "quit"),
];

/// `--idletest` (run with `--opacity 80 --active-opacity 100 --idle-dim 3 --idle-opacity 40`): the window dims
/// after 3 s of shell silence, brightens on output, and the active opacity wins while the keyboard is held.
const IDLE: &[(u64, &str)] = &[
    (7000, "state:idle"),
    (7500, "focus-flag:1"),
    (7900, "state:active"),
    (8200, "focus-flag:0"),
    (9300, "state:passive-again"),
    (9600, "w:echo wake\r"),
    (10600, "state:woke"),
    (15500, "state:idle-again"),
    (16000, "ghost"),
    (16300, "state:ghost-over-idle"),
    (16600, "ghost"),
    (16900, "state:idle-after-ghost"),
    (17300, "quit"),
];

/// `--starttest` (run with `--start-hidden --start-ghost`): the window starts hidden and in ghost mode.
const START: &[(u64, &str)] = &[
    (2000, "state:started"),
    (2500, "toggle-show"),
    (3000, "state:shown"),
    (3500, "ghost"),
    (4000, "state:ghost-off"),
    (4300, "quit"),
];

/// `--autostarttest`: the Start with Windows entry is written and removed (under a test-only value name). It is left
/// switched on at the end so the caller can read the stored value; the caller removes it.
const AUTOSTART: &[(u64, &str)] = &[
    (1500, "autostart:off"),
    (2000, "autostart:on"),
    (2500, "autostart:on"),
    (3000, "autostart:off"),
    (3500, "autostart:off"),
    (4000, "autostart:on"),
    (4500, "quit"),
];

/// `--ghosttest`: the ghost preset through the host functions (no OS hotkeys). `state` logs the real
/// window alpha, click-through, ghost and the saved opacity.
const GHOST: &[(u64, &str)] = &[
    (1500, "state:start"),
    (2000, "ghost"),
    (2300, "state:ghost-on"),
    (3000, "ghost"),
    (3300, "state:ghost-off"),
    (4000, "opacity:70"),
    (4300, "state:opacity-70"),
    (4600, "ghost"),
    (4900, "state:ghost-from-70"),
    (5200, "ghost"),
    (5500, "state:restored-70"),
    (6000, "opacity:30"),
    (6300, "ghost"),
    (6600, "state:ghost-from-30-never-brightens"),
    (6900, "ghost"),
    (7200, "state:restored-30"),
    (7500, "opacity:100"),
    (7800, "ct:off"),
    (8100, "state:interactive"),
    (8400, "ghost"),
    (8700, "state:ghost-forces-click-through"),
    (9000, "ghost"),
    (9300, "state:interactive-restored"),
    (9600, "ghost"),
    (9900, "toggle-ct"),
    (10200, "state:ct-toggle-leaves-ghost"),
    (10500, "ghost"),
    (10800, "opacity:60"),
    (11100, "state:opacity-change-leaves-ghost"),
    (11500, "quit"),
];

/// `--attention`: bell / OSC 9 / OSC 777 while HUDshell is passive. No `cdp-focus`, so nothing in this
/// scenario gives the page the keyboard. `focus-flag` only simulates the host's "terminal has the
/// keyboard" flag (the real grab is covered by the focus suites).
const ATTENTION: &[(u64, &str)] = &[
    (2000, "w:[Console]::Write([char]7)\r"),
    (2600, "dump:cue-after-bell"),
    (2900, "w:[Console]::Write(\"$([char]27)]9;rate-limited$([char]27)\\\")\r"),
    (6500, "dump:cue-faded"),
    (6800, "w:[Console]::Write(\"$([char]27)]9;build finished$([char]27)\\\")\r"),
    (7400, "dump:cue-after-osc9"),
    (10500, "w:[Console]::Write(\"$([char]27)]9;4;3;50$([char]27)\\\")\r"),
    (11200, "w:[Console]::Write(\"$([char]27)]9;9;C:\\temp$([char]27)\\\")\r"),
    (12000, "dump:no-cue-for-progress-and-cwd"),
    (12300, "w:[Console]::Write(\"$([char]27)]777;notify;Title;Body text$([char]27)\\\")\r"),
    (12900, "dump:cue-after-osc777"),
    (16000, "focus-flag:1"),
    (16100, "w:[Console]::Write([char]7)\r"),
    (16800, "dump:no-cue-while-focused"),
    (17000, "focus-flag:0"),
    (17100, "toggle-show"),
    (17600, "w:[Console]::Write([char]7)\r"),
    (18300, "toggle-show"),
    (18800, "quit"),
];

/// `--policytest`: clipboard policies driven through DevTools mouse events (the page sees real mouse
/// events; no OS input). The caller pre-fills the clipboard with a sentinel and reads it back afterwards.
const POLICY: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "w:Write-Host 'COPYSEL-LINE-OF-TEXT-FOR-SELECTION'\r"),
    (4000, "w:[Console]::Write(\"$([char]27)]52;c;aGVsbG8=$([char]7)\")\r"),
    (4600, "w:[Console]::Write(\"$([char]27)]52;c;?$([char]7)\")\r"),
    (5200, "w:[Console]::Write(\"$([char]27)]8;;https://example.invalid/$([char]27)\\a link$([char]27)]8;;$([char]27)\\\")\r"),
    (6000, "dump:policy-screen"),
    (6500, "rclick"),
    (8000, "dump:after-right-click-no-selection"),
    (8500, "ldrag"),
    (9200, "rclick"),
    (10200, "dump:after-drag-and-right-click"),
    (10500, "census"),
    (10900, "quit"),
];

/// `--probe4`: how does PowerShell's prompt (PSReadLine) react to ESC CR (Alt+Enter)? Clean line first.
const PROBE4: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "key:Ctrl+C"),
    (3000, "type:echo one"),
    (3400, "key:Alt+Enter"),
    (3800, "type:echo two"),
    (4300, "dump:alt-enter-in-pwsh"),
    (4600, "key:Enter"),
    (5600, "dump:after-enter"),
    (6000, "quit"),
];

/// `--probe3`: guarded live test of Codex in an already-trusted folder. Process-local dead proxy and
/// base URL so nothing can reach the API; no Enter is ever sent; dialogs are never answered.
const PROBE3: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2500, "w:$env:HTTPS_PROXY='http://127.0.0.1:9'; $env:HTTP_PROXY='http://127.0.0.1:9'; $env:ALL_PROXY='http://127.0.0.1:9'; $env:OPENAI_BASE_URL='http://127.0.0.1:9'; cd $env:HUDSHELL_SANDBOX\r"),
    (3500, "type:codex"),
    (3800, "key:Enter"),
    (13000, "dump:codex-start"),
    (13500, "census"),
    (13800, "skipif:trust this;trust the contents;do you trust;sign in;log in;update available;folder access|codex-exit"),
    (14000, "resize:700,420"),
    (15500, "dump:codex-resized"),
    (15700, "resize:900,520"),
    (17000, "dump:codex-resized-back"),
    (17300, "type:AAA1"),
    (17700, "key:Shift+Enter"),
    (18100, "type:BBB1"),
    (19000, "dump:codex-shift-enter"),
    (19300, "key:Ctrl+C"),
    (20300, "dump:codex-after-clear"),
    (20600, "type:AAA2"),
    (21000, "key:Ctrl+J"),
    (21400, "type:BBB2"),
    (22300, "dump:codex-ctrl-j"),
    (22600, "key:Ctrl+C"),
    (23600, "dump:codex-after-clear-2"),
    (23900, "type:AAA3"),
    (24300, "key:Alt+Enter"),
    (24700, "type:BBB3"),
    (25600, "dump:codex-alt-enter"),
    (26000, "label:codex-exit"),
    (26100, "key:Ctrl+C"),
    (26900, "key:Ctrl+C"),
    (27700, "key:Ctrl+C"),
    (28500, "key:Ctrl+C"),
    (30500, "w:Remove-Item Env:HTTPS_PROXY,Env:HTTP_PROXY,Env:ALL_PROXY,Env:OPENAI_BASE_URL\r"),
    (31500, "dump:exited"),
    (32000, "census"),
    (32500, "quit"),
];

/// `--real`: real prompts in the scratch folder named by HUDSHELL_SANDBOX (a folder both CLIs already trust) for Claude and Codex.
/// Trust dialogs for that folder and permission prompts for actions inside it are accepted with Enter
/// (option 1); nothing else is answered. Built at runtime so waits and approval cycles can be looped.
fn build_real() -> &'static [(u64, &'static str)] {
    static S: std::sync::OnceLock<&'static [(u64, &'static str)]> = std::sync::OnceLock::new();
    struct Sc {
        t: u64,
        v: Vec<(u64, String)>,
    }
    impl Sc {
        fn at(&mut self, dt: u64, a: &str) {
            self.t += dt;
            self.v.push((self.t, a.to_string()));
        }
        // Look at the screen, accept a permission prompt (option 1) if one is showing, repeat.
        fn watch(&mut self, label: &str, cycles: u32) {
            for i in 0..cycles {
                self.at(3200, &format!("dump:{label}-{i}"));
                self.at(900, "pressif:do you want to;would you like to run;would you like to make|Enter");
            }
        }
    }
    S.get_or_init(|| {
        let mut s = Sc { t: 2500, v: Vec::new() };
        let trust = "pressif:trust this folder;do you trust;trust the files;folder access;trust and continue|Enter";
        s.at(0, "cdp-focus");
        s.at(500, "w:cd $env:HUDSHELL_SANDBOX\r");
        for (agent, cmd, c1, c2) in
            [("claude", "claude", 6u32, 3u32), ("codex", "codex", 9u32, 5u32)]
        {
            s.at(1500, &format!("phase:{agent}-real"));
            s.at(500, &format!("type:{cmd}"));
            s.at(300, "key:Enter");
            s.at(9000, &format!("dump:{agent}-start"));
            s.at(900, trust);
            s.at(4500, &format!("dump:{agent}-ready"));
            // Turn 1: a tool-using task (file write + run) with permission prompts.
            s.at(600, "type:Create a file named hello.py that prints hello hudshell and run it. Keep it brief.");
            s.at(400, "key:Enter");
            s.watch(&format!("{agent}-turn1"), c1);
            s.at(1500, &format!("dump:{agent}-turn1-done"));
            // Turn 2: a two-line prompt typed with Shift+Enter, then submitted.
            s.at(800, "type:Write two short lines about terminals.");
            s.at(400, "key:Shift+Enter");
            s.at(400, "type:The second line must contain the word hudshell.");
            s.at(1500, &format!("dump:{agent}-multiline-input"));
            s.at(500, "key:Enter");
            s.watch(&format!("{agent}-turn2"), c2);
            // Turn 3: a LONG streaming reply (about a minute), resized mid-stream, interrupted with Esc.
            s.at(800, "type:Write the numbers from 1 to 3000, one per line, nothing else.");
            s.at(400, "key:Enter");
            s.at(2500, "resize:700,420");
            s.at(2500, &format!("dump:{agent}-stream-narrow"));
            s.at(600, "resize:900,520");
            s.at(1500, &format!("dump:{agent}-stream-wide"));
            s.at(500, "key:Esc");
            s.at(2500, &format!("dump:{agent}-after-esc"));
            // Leave the program with its own /exit command, then make SURE the shell is back before
            // anything else is typed: retry Ctrl+C only while the screen is not a PowerShell prompt,
            // and abort the whole run (typing nothing further) if it never comes back.
            s.at(800, "type:/exit");
            s.at(400, "key:Enter");
            for i in 0..5 {
                s.at(2500, &format!("dump:{agent}-leave-{i}"));
                s.at(800, "ctrlc-unless-prompt");
            }
            s.at(2500, &format!("dump:{agent}-exited"));
            s.at(800, "abort-unless-prompt|end");
        }
        s.at(0, "label:end");
        s.at(1000, "census");
        s.at(500, "quit");
        let leaked: Vec<(u64, &'static str)> =
            s.v.into_iter().map(|(t, a)| (t, &*Box::leak(a.into_boxed_str()))).collect();
        &*Box::leak(leaked.into_boxed_slice())
    })
}

/// `--soak SECONDS`: every 4 s the shell prints 300 lines for the given duration (default 300 s);
/// an external sampler records memory so growth over time is visible.
fn build_soak() -> &'static [(u64, &'static str)] {
    static S: std::sync::OnceLock<&'static [(u64, &'static str)]> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let args: Vec<String> = std::env::args().collect();
        let secs: u64 = args
            .iter()
            .position(|a| a == "--soak")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let mut v: Vec<(u64, &'static str)> = vec![(2500, "cdp-focus")];
        let mut t = 4000u64;
        while t < secs * 1000 {
            v.push((t, "w:1..300 | ForEach-Object { 'soak line {0} {1}' -f $_, ('x'*60) }\r"));
            t += 4000;
        }
        v.push((t + 1500, "census"));
        v.push((t + 2000, "quit"));
        &*Box::leak(v.into_boxed_slice())
    })
}

/// `--texttest` (run with `--text-hotkeys all`, `--text-scale N`, `--ui-scale N`, `--line-height N`, `--letter-spacing N` as
/// needed): each terminal's own text size, the hotkeys' scope, reset, and the scales. Every dump lists all sizes.
const TEXT: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "dumps:1:t-base"),
    (3500, "shell:new"),
    (7000, "font:+"),
    (7300, "font:+"),
    (8000, "dumps:2:t-two-big"),
    (8600, "shell:jump:1"),
    (9000, "font:-"),
    (9800, "dumps:1:t-one-small"),
    (10200, "shell:jump:2"),
    (10800, "dumps:2:t-two-kept"),
    (11200, "font:reset"),
    (12000, "dumps:2:t-reset"),
    (12500, "quit"),
];

/// `--sessionsave` / `--sessionload` (run with `--test-profiles --restore-session shells --session-file <path>`): the first run opens shells
/// from three profiles and a 2 x 2 layout and quits; the second starts again and must come back with the same shells and layout.
const SESSION_SAVE: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "shell:new:work"),
    (7000, "shell:new:cmd"),
    (11000, "layout:set:grid"),
    (16000, "pane:next"),
    (17000, "dumps:1:ss-end"),
    (17500, "quit"),
];
const SESSION_LOAD: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (20000, "dumps:1:sl-restored"),
    (20500, "quit"),
];

/// `--windowstest`: shells are moved into windows of their own (detach), the shells keep running there, a window closes with its last
/// shell, the cap of four windows is enforced, and the windows snap to each other and move as a group. See Suite-Windows.
const WINDOWS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:700,420"),
    (3000, "shell:new"),
    (7000, "wto:1:echo AAA-ONE-BEFORE\r"),
    (7200, "wto:2:echo BBB-TWO-BEFORE\r"),
    (9000, "dumps:1:w-before-1"),
    (9300, "dumps:2:w-before-2"),
    (10000, "winstate:before"),
    (10500, "window:detach:2"),
    (17000, "winstate:after-detach"),
    (17500, "inwin:2:dumps:2:w-moved"),
    (18000, "dumps:1:w-rest"),
    (19000, "wto:2:echo AFTER-DETACH-TWO\r"),
    (19200, "wto:1:echo AFTER-DETACH-ONE\r"),
    (21000, "inwin:2:dumps:2:w-two-after"),
    (21500, "dumps:1:w-one-after"),
    (22500, "window:next"),
    (22800, "winstate:after-next"),
    (23500, "window:detach:1"),
    (24500, "winstate:refused-only-shell"),
    (25000, "inwin:2:shell:close"),
    (25600, "inwin:2:shell:close"),
    (27000, "winstate:after-close"),
    // the cap: three more windows are fine, the fifth is refused
    (28000, "shell:new"),
    (31000, "shell:new"),
    (34000, "shell:new"),
    (37000, "shell:new"),
    (40000, "winstate:four-shells"),
    // the first of the three goes the way a Shift+click on a tab does: a message from the page itself
    (40500, "js:window.chrome.webview.postMessage(JSON.stringify({t:'detach',s:5}))"),
    (46000, "window:detach:4"),
    (52000, "window:detach:3"),
    (58000, "winstate:four-windows"),
    (58500, "window:detach:6"),
    (59500, "winstate:cap-refused"),
    // the hotkeys act on the active window (the newest one), the tray's "all windows" items on every window
    (60000, "winstate:pre-hotkey"),
    (60200, "hk:22"),
    (63000, "winstate:post-hotkey"),
    (63500, "ghost:all"),
    (64000, "winstate:ghost-all"),
    (64500, "ghost:all"),
    (65000, "winstate:ghost-off"),
    (65500, "showhide:all"),
    (66000, "winstate:hidden-all"),
    (66500, "showhide:all"),
    (67000, "winstate:shown-all"),
    // the same two through their hotkeys
    (67500, "hk:29"),
    (68000, "winstate:hk-ghost"),
    (68500, "hk:29"),
    (69000, "winstate:hk-ghost-off"),
    (69500, "hk:39"),
    (70000, "winstate:hk-hidden"),
    (70500, "hk:39"),
    (71000, "winstate:hk-shown"),
    (71500, "quit"),
];

/// `--recovertest`: two shells; the runner kills the web view's browser process after the first dump (see Suite-Recover). The shells must
/// survive, and what is typed into them afterwards must show up in the new page.
const RECOVER: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "shell:new"),
    (9000, "w:echo BEFORE-CRASH\r"),
    (10000, "dumps:1:rc-before"),
    (40000, "shell:jump:1"),
    (40500, "w:echo AFTER-CRASH-ONE\r"),
    (42000, "dumps:1:rc-one"),
    (42500, "wto:2:echo AFTER-CRASH-TWO\r"),
    (44500, "dumps:2:rc-two"),
    (45000, "quit"),
];

/// `--startfailtest`: the tab says why when its shell could not start, and Enter then starts one.
const STARTFAIL: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (4000, "startfail:CreateProcess: the system cannot find the file specified"),
    (5000, "dumps:1:sf-message"),
    (5500, "key:Enter"),
    (8000, "w:echo STARTED-AFTER-ENTER\r"),
    (9000, "dumps:1:sf-after"),
    (9500, "quit"),
];

/// `--reloadtest`: settings reloaded while running. Each `reload:` is a whole settings text (on top of the defaults), fed to the same code
/// as an edited config.json; the dumps and states show what really changed. A bad text must change nothing.
const RELOAD: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:900,520"),
    (3000, "dumps:1:r-base"),
    (3500, "reload:{\"text_scale\":150}"),
    (4500, "dumps:1:r-text"),
    (5000, "reload:{\"text_scale\":150,\"ui_scale\":150,\"tabs\":\"always\"}"),
    (6000, "dumps:1:r-ui"),
    (6500, "reload:{\"text_scale\":150,\"ui_scale\":150,\"tabs\":\"always\",\"opacity\":60}"),
    (7500, "state:r-opacity"),
    (8000, "reload:{\"line_height\":150,\"letter_spacing\":3}"),
    (9000, "dumps:1:r-spacing"),
    (9500, "reload:{ this is not json"),
    (10000, "state:r-bad"),
    (10500, "reload:{\"shell\":\"C:\\\\nowhere\\\\sh.exe\",\"line_height\":150,\"letter_spacing\":3}"),
    (11200, "quit"),
];

/// `--dpitest` (run with `--at X,Y --cross CX,CY`): the window starts at X,Y, moves to CX,CY (another monitor, normally with
/// a different scaling), docks in a corner there, and comes back. State and page dumps at every step.
fn build_dpitest() -> &'static [(u64, &'static str)] {
    static S: std::sync::OnceLock<&'static [(u64, &'static str)]> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let args: Vec<String> = std::env::args().collect();
        let pos = |name: &str| -> (i32, i32) {
            let v = args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
            let n: Vec<i32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            (n.first().copied().unwrap_or(200), n.get(1).copied().unwrap_or(200))
        };
        let (cx, cy) = pos("--cross");
        let (hx, hy) = pos("--at");
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let v: Vec<(u64, &'static str)> = vec![
            (1500, "cdp-focus"),
            (2000, "resize:700,420"),
            (3000, "state:a"),
            (3100, "dump:a"),
            (4000, leak(format!("moveto:{cx},{cy}"))),
            (6000, "state:b"),
            (6100, "dump:b"),
            (7000, "corner:next"),
            (8000, "state:c"),
            (8100, "dump:c"),
            (9000, leak(format!("moveto:{hx},{hy}"))),
            (11000, "state:d"),
            (11100, "dump:d"),
            (12000, "quit"),
        ];
        &*Box::leak(v.into_boxed_slice())
    })
}

/// `--costwindows`: four shells in one window, then the same four shells spread over four windows; the runner measures memory and idle CPU
/// of the process tree in both states (Suite-CostWindows).
const COST_WINDOWS: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:700,420"),
    (3000, "shell:new"),
    (6000, "shell:new"),
    (9000, "shell:new"),
    (20000, "winstate:one-window"),
    (50000, "window:detach:2"),
    (58000, "window:detach:3"),
    (66000, "window:detach:4"),
    (80000, "winstate:four-windows"),
    (100000, "quit"),
];

/// `--windowsmove`: a shell moves into another existing window (a window's only shell takes its window with it), the colour theme reaches every
/// window, and a window that is off every screen after a display change comes back. See Suite-WindowsMove.
const WINDOWS_MOVE: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:700,420"),
    (3000, "shell:new"),
    (6000, "window:detach:2"),
    (12000, "theme:next"),
    (12600, "js:window.chrome.webview.postMessage(JSON.stringify({t:'note',k:'bg',v:getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()}))"),
    (12800, "inwin:2:js:window.chrome.webview.postMessage(JSON.stringify({t:'note',k:'bg',v:getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()}))"),
    (14000, "inwin:2:shell:new:work"),
    (18000, "winstate:before-move"),
    (18500, "inwin:2:shell:move"),
    (22000, "winstate:after-move"),
    (22200, "spawnprofile:3"),
    (22500, "wto:3:echo MOVED-THREE\r"),
    (24000, "dumps:3:moved-three"),
    (24500, "inwin:2:shell:move"),
    (28000, "winstate:after-last-move"),
    (28500, "wto:2:echo MOVED-TWO\r"),
    (30000, "dumps:2:moved-two"),
    (30500, "win:offscreen"),
    (32000, "winstate:after-offscreen"),
    (32500, "quit"),
];

/// `--windowssave` / `--windowsload` (run with `--test-profiles --restore-session shells --session-file <path>`): the first run builds three windows
/// (two shells, two shells, one shell) and quits; the second starts again and must come back with the same windows. See Suite-WindowsRestore.
const WINDOWS_SAVE: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:700,420"),
    (3000, "shell:new"),
    (7000, "shell:new:work"),
    (11000, "shell:new:cmd"),
    (15000, "window:detach:3"),
    (22000, "window:detach:4"),
    (29000, "inwin:2:shell:new:cmd"),
    (34000, "winstate:saved"),
    (34500, "quit"),
];
const WINDOWS_LOAD: &[(u64, &str)] = &[
    (1500, "cdp-focus"),
    (2000, "resize:700,420"),
    (30000, "winstate:restored"),
    (30500, "quit"),
];

/// `--windowsnap` (run with `--interactive --at X,Y`): two windows (the second made by detaching a shell), and every second the state of all
/// windows is logged for the runner, which drags them with the real mouse (Suite-WindowSnap).
fn build_windowsnap() -> &'static [(u64, &'static str)] {
    static S: std::sync::OnceLock<&'static [(u64, &'static str)]> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let args: Vec<String> = std::env::args().collect();
        let size = args.iter().position(|a| a == "--size").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "420,260".into());
        let resize: &'static str = Box::leak(format!("resize:{size}").into_boxed_str());
        let mut v: Vec<(u64, &'static str)> = vec![(1500, "cdp-focus"), (2000, resize), (3000, "shell:new"), (6000, "window:detach:2")];
        let mut t = 11000u64;
        while t <= 100000 {
            v.push((t, "winstate:snap"));
            t += 1000;
        }
        v.push((101500, "quit"));
        &*Box::leak(v.into_boxed_slice())
    })
}

/// `--inputtest` (run with `--interactive --layout side-by-side --dividers --tabs always`): no script of its own. Every
/// second it dumps the page (with the position of the tabs and dividers) and the window state, so the runner can aim
/// real mouse and keyboard input at what is really on screen and read the result.
fn build_inputtest() -> &'static [(u64, &'static str)] {
    static S: std::sync::OnceLock<&'static [(u64, &'static str)]> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        // --size W,H (96-dpi pixels) sets the window size the script starts with (default 900,520).
        let args: Vec<String> = std::env::args().collect();
        let size = args.iter().position(|a| a == "--size").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "900,520".into());
        let resize: &'static str = Box::leak(format!("resize:{size}").into_boxed_str());
        let mut v: Vec<(u64, &'static str)> = vec![(1500, "cdp-focus"), (2000, resize)];
        let mut t = 6000u64;
        while t <= 200000 {
            v.push((t, "state:m"));
            v.push((t + 100, "dump:m"));
            t += 1000;
        }
        v.push((201500, "quit"));
        &*Box::leak(v.into_boxed_slice())
    })
}

fn scen() -> &'static [(u64, &'static str)] {
    match SCEN_SEL.load(Ordering::SeqCst) {
        42 => COST_WINDOWS,
        41 => WINDOWS_MOVE,
        40 => WINDOWS_LOAD,
        39 => WINDOWS_SAVE,
        38 => build_windowsnap(),
        37 => WINDOWS,
        36 => RECOVER,
        35 => STARTFAIL,
        34 => SESSION_LOAD,
        33 => SESSION_SAVE,
        32 => RELOAD,
        31 => TEXT,
        30 => build_dpitest(),
        29 => build_inputtest(),
        28 => LABELS,
        27 => LAYOUTOWN,
        26 => LAYOUTSTART,
        25 => LAYOUTEMPTY,
        24 => LAYOUT,
        23 => SHELLSFULL,
        22 => TABMODES,
        21 => SHELLS,
        20 => SESSIONS,
        17 => IDLE,
        18 => START,
        19 => AUTOSTART,
        16 => PLACE,
        12 => SCROLLBACK,
        13 => GHOST,
        14 => ATTENTION,
        15 => POLICY,
        7 => build_real(),
        8 => CLICKFOCUS,
        9 => HIDESHOW,
        10 => CLIPBOARD,
        11 => build_soak(),
        1 => KEYS,
        2 => WIDTH,
        3 => PROBE,
        4 => PROBE2,
        5 => PROBE3,
        6 => PROBE4,
        _ => AUTOTEST,
    }
}
static SCEN_SEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static AT_IDX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static AT_T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static NO_GRAB: AtomicBool = AtomicBool::new(false);
static PHASE: Mutex<String> = Mutex::new(String::new());
static FG_CHANGES: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
static LAST_FG: AtomicIsize = AtomicIsize::new(0);

fn cur_phase() -> String {
    PHASE.lock().map(|p| p.clone()).unwrap_or_default()
}

unsafe fn send_hotkey(vk: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };
    let k = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [k(0x11, false), k(0x12, false), k(vk, false), k(vk, true), k(0x12, true), k(0x11, true)];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

pub(crate) static OLD_CURSOR_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub(crate) static OLD_CURSOR_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Test only: one real left click at the centre of the HUDshell window. Refuses to click unless
/// HUDshell itself is the window under that point (so the click can never land in a game), and
/// puts the user's mouse cursor back afterwards (timer 31).
unsafe fn real_click_center(hwnd: HWND) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
        MOUSE_EVENT_FLAGS,
    };
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    let pt = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
    let under = unsafe { WindowFromPoint(pt) };
    let root = unsafe { GetAncestor(under, GA_ROOT) };
    if root != hwnd {
        log(&format!(
            "REAL-CLICK ABORTED: HUDshell is not the window under ({}, {}) - found {}",
            pt.x,
            pt.y,
            unsafe { describe(under) }
        ));
        return;
    }
    let mut old = POINT::default();
    let _ = unsafe { GetCursorPos(&mut old) };
    OLD_CURSOR_X.store(old.x, Ordering::SeqCst);
    OLD_CURSOR_Y.store(old.y, Ordering::SeqCst);
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let nx = ((pt.x - vx) as i64 * 65535 / ((vw - 1).max(1) as i64)) as i32;
    let ny = ((pt.y - vy) as i64 * 65535 / ((vh - 1).max(1) as i64)) as i32;
    let mk = |flags: MOUSE_EVENT_FLAGS| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: nx, dy: ny, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    };
    let inputs = [
        mk(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
        mk(MOUSEEVENTF_LEFTDOWN),
        mk(MOUSEEVENTF_LEFTUP),
    ];
    log(&format!("REAL-CLICK at ({}, {}) on HUDshell (cursor will be restored afterwards)", pt.x, pt.y));
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    let _ = unsafe { SetTimer(Some(hwnd), 31, 500, None) };
}

unsafe fn hit_test(hwnd: HWND, expect_self: bool) {
    use windows::Win32::Foundation::POINT;
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    let pt = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
    let w = unsafe { WindowFromPoint(pt) };
    let root = unsafe { GetAncestor(w, GA_ROOT) };
    let is_self = root == hwnd;
    log(&format!(
        "HIT-TEST at ({}, {}): window under point = {} (root is HUDshell: {}); expected {} -> {}",
        pt.x,
        pt.y,
        unsafe { describe(w) },
        is_self,
        if expect_self { "HUDshell" } else { "something else (click-through)" },
        if is_self == expect_self { "PASS" } else { "FAIL" }
    ));
}

/// True if the last non-empty line of the latest screen dump is a PowerShell prompt.
fn at_shell_prompt() -> bool {
    wv::last_dump()
        .lines()
        .rev()
        .map(|l| l.trim_end())
        .find(|l| !l.trim().is_empty())
        .map(|l| l.starts_with("PS ") && l.ends_with('>'))
        .unwrap_or(false)
}

unsafe fn run_action(hwnd: HWND, act: &str) {
    if let Some(p) = act.strip_prefix("phase:") {
        if let Ok(mut ph) = PHASE.lock() {
            *ph = p.to_string();
        }
        log(&format!("===== PHASE {p} ====="));
        // Cumulative census at every phase boundary, so each program's sequences can be told apart.
        for l in pty::census_report().lines() {
            log(&format!("   [census at start of {p}] {}", l.trim()));
        }
    } else if act == "fg" {
        unsafe { foreground_note(&format!("[{}]", cur_phase())) };
    } else if act == "cdp-focus" {
        wv::cdp("Emulation.setFocusEmulationEnabled", "{\"enabled\":true}");
        wv::post("{\"t\":\"focus\"}");
    } else if let Some(t) = act.strip_prefix("type:") {
        wv::type_text(t);
    } else if let Some(k) = act.strip_prefix("key:") {
        wv::send_key(k);
    } else if let Some(t) = act.strip_prefix("paste:") {
        wv::paste_text(t);
    } else if let Some(t) = act.strip_prefix("w:") {
        pty::write(t.as_bytes().to_vec());
    } else if act == "ctrlc" {
        pty::write(vec![0x03]);
    } else if let Some(label) = act.strip_prefix("dump:") {
        log(&format!("DUMP label={label}"));
        wv::post("{\"t\":\"dump\"}");
    } else if let Some(dim) = act.strip_prefix("resize:") {
        let mut p = dim.split(',');
        let w: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(900);
        let h: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(520);
        // W,H are 96-dpi pixels: the same logical size on a monitor with another scaling (identical at 100%).
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
        let (w, h) = (w * dpi / 96, h * dpi / 96);
        let _ = unsafe {
            SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)
        };
    } else if act.starts_with("label:") {
        // marker only
    } else if let Some(rest) = act.strip_prefix("skipif:") {
        let (alts, label) = rest.split_once('|').unwrap_or((rest, ""));
        let dump = wv::last_dump().to_lowercase();
        if let Some(hit) = alts.split(';').find(|a| dump.contains(&a.to_lowercase()))
            && let Some(i) = scen().iter().position(|(_, a)| *a == format!("label:{label}")) {
                log(&format!("SKIP: screen contains '{hit}' -> jumping to label {label} (nothing was answered)"));
                AT_IDX.store(i, Ordering::SeqCst);
            }
    } else if act == "ctrlc-unless-prompt" {
        if at_shell_prompt() {
            log("already back at the PowerShell prompt");
        } else {
            log("not at the PowerShell prompt yet: sending Ctrl+C");
            wv::send_key("Ctrl+C");
        }
    } else if let Some(label) = act.strip_prefix("abort-unless-prompt|") {
        if at_shell_prompt() {
            log("confirmed: back at the PowerShell prompt");
        } else if let Some(i) = scen().iter().position(|(_, a)| *a == format!("label:{label}")) {
            log("ABORT: still inside the program; refusing to type anything further");
            AT_IDX.store(i, Ordering::SeqCst);
        }
    } else if let Some(rest) = act.strip_prefix("pressif:") {
        // Answer a dialog with a key only if the latest screen dump shows one of the phrases.
        let (alts, key) = rest.split_once('|').unwrap_or((rest, "Enter"));
        let dump = wv::last_dump().to_lowercase();
        if let Some(hit) = alts.split(';').find(|a| dump.contains(&a.to_lowercase())) {
            log(&format!("PRESS {key}: screen shows '{hit}'"));
            wv::send_key(key);
        } else {
            log("PRESS skipped: no matching dialog on screen");
        }
    } else if let Some(label) = act.strip_prefix("skipunless:kitty|") {
        // Shift+Enter is only distinguishable from Enter once the program negotiated the kitty
        // keyboard protocol. Otherwise it would submit the prompt, so refuse to send it.
        if pty::kitty_pushes() == 0 {
            if let Some(i) = scen().iter().position(|(_, a)| *a == format!("label:{label}")) {
                log("SKIP: program has not negotiated the kitty keyboard protocol; not sending Shift+Enter (it would act as Enter)");
                AT_IDX.store(i, Ordering::SeqCst);
            }
        } else {
            log(&format!("kitty keyboard protocol negotiated ({} push(es)): Shift+Enter is safe", pty::kitty_pushes()));
        }
    } else if let Some(k) = act.strip_prefix("hotkey:") {
        if NO_GRAB.load(Ordering::SeqCst) {
            log(&format!("SKIP (--no-grab): hotkey {k}"));
        } else {
            // Single letter -> its virtual-key code (A-Z / 0-9 equal their ASCII upper-case value).
            let vk = k.bytes().next().map(|b| b.to_ascii_uppercase() as u16).unwrap_or(0x47);
            log(&format!("sending real hotkey Ctrl+Alt+{k}"));
            unsafe { send_hotkey(vk) };
        }
    } else if let Some(label) = act.strip_prefix("state:") {
        let mut a = 0u8;
        let _ = unsafe { GetLayeredWindowAttributes(hwnd, None, Some(&mut a), None) };
        let pct = (a as u32 * 100 + 127) / 255;
        let mut wr = RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut wr) };
        let wk = unsafe { work_area(hwnd) };
        log(&format!(
            "STATE label={label} rect={},{},{},{} work={},{},{},{} font={} dpi={} alpha={a} pct={pct} click_through={} ghost={} focus={} visible={} cfg_opacity={}",
            wr.left, wr.top, wr.right, wr.bottom, wk.l, wk.t, wk.r, wk.b, font_size_of(shells_lock().active()), unsafe { GetDpiForWindow(hwnd) },
            CLICK_THROUGH.load(Ordering::SeqCst),
            GHOST_ON.load(Ordering::SeqCst),
            TERMINAL_FOCUS.load(Ordering::SeqCst),
            VISIBLE.load(Ordering::SeqCst),
            cfg().opacity
        ));
    } else if let Some(id) = act.strip_prefix("window:detach:") {
        detach_shell(id.parse().unwrap_or(0));
    } else if act == "window:next" {
        next_window();
    } else if let Some(id) = act.strip_prefix("spawnprofile:") {
        // which profile a shell would restart from (nothing about its content)
        let id: u32 = id.parse().unwrap_or(0);
        let name = SPAWN_PROFILES.lock().ok().and_then(|l| l.iter().find(|(i, _)| *i == id).map(|(_, p)| p.name.clone()));
        log(&format!("SPAWNPROFILE id={id} name={}", name.unwrap_or_else(|| "none".into())));
    } else if act == "shell:move" {
        move_active_shell_to_next_window();
    } else if act == "theme:next" {
        cycle_theme();
    } else if act == "win:offscreen" {
        // the window is put far off every screen, then Windows' display-change message is delivered as after a monitor was unplugged
        let _ = unsafe { SetWindowPos(hwnd, None, -30000, -30000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
        let _ = unsafe { SendMessageW(hwnd, WM_DISPLAYCHANGE, Some(WPARAM(32)), Some(LPARAM(0))) };
    } else if let Some(id) = act.strip_prefix("hk:") {
        // a hotkey as the first window receives it (what RegisterHotKey delivers), without pressing any key
        let _ = unsafe { SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(id.parse().unwrap_or(0))), Some(LPARAM(0))) };
    } else if act == "ghost:all" {
        unsafe { ghost_all() };
    } else if act == "showhide:all" {
        unsafe { show_hide_all() };
    } else if let Some(label) = act.strip_prefix("winstate:") {
        // one line per window: serial, rectangle, shells, active shell, visible
        let mut parts: Vec<String> = Vec::new();
        for (serial, h) in perwin::all() {
            let _w = perwin::enter(serial);
            let mut wr = RECT::default();
            let _ = unsafe { GetWindowRect(HWND(h as *mut std::ffi::c_void), &mut wr) };
            let ids: Vec<String> = shells_lock().ids().iter().map(|i| i.to_string()).collect();
            parts.push(format!(
                "{serial}:rect={},{},{},{}:shells={}:active={}:visible={}:ghost={}",
                wr.left, wr.top, wr.right, wr.bottom, ids.join(","), shells_lock().active(), VISIBLE.load(Ordering::SeqCst),
                GHOST_ON.load(Ordering::SeqCst)
            ));
        }
        log(&format!(
            "WINDOWS label={label} n={} current_window={} | {}",
            perwin::count(),
            ACTIVE_WINDOW.load(Ordering::SeqCst),
            parts.join(" | ")
        ));
    } else if let Some(rest) = act.strip_prefix("inwin:") {
        // inwin:<window serial>:<action>: run an action as if it came from that window
        if let Some((n, inner)) = rest.split_once(':') { match n.parse::<u32>().ok().and_then(|s| perwin::hwnd_of(s).map(|h| (s, h))) {
            Some((serial, h)) => {
                let _w = perwin::enter(serial);
                unsafe { run_action(HWND(h as *mut std::ffi::c_void), inner) };
            }
            None => log(&format!("inwin: there is no window {n}")),
        } }
    } else if act == "shell:new" {
        new_shell(None);
    } else if let Some(name) = act.strip_prefix("shell:new:") {
        new_shell(Some(name));
    } else if act == "shell:next" {
        shell_step(1);
    } else if act == "shell:prev" {
        shell_step(-1);
    } else if let Some(n) = act.strip_prefix("shell:jump:") {
        shell_jump(n.parse().unwrap_or(1));
    } else if act == "shell:close" {
        close_active_shell();
    } else if act == "shell:list" {
        wv::post("{\"t\":\"overlay\"}");
    } else if act == "shell:picker" {
        // Opens the picker in the page only; the real hotkey also grabs the keyboard, which tests avoid.
        push_tabs();
        wv::post("{\"t\":\"picker\"}");
    } else if let Some(text) = act.strip_prefix("startfail:") {
        // The message the host sends when a shell cannot start (a real failure cannot be forced: other shells are tried first).
        wv::post(&serde_json::json!({ "t": "startfail", "s": 1, "msg": text }).to_string());
    } else if let Some(text) = act.strip_prefix("reload:") {
        match config::try_parse(text) {
            Ok(c) => unsafe { reload_with(hwnd, c, false) },
            Err(e) => log(&format!("config reload: the settings file is not valid ({e}); keeping the settings in use")),
        }
    } else if act == "layout:next" {
        layout_cycle();
    } else if let Some(name) = act.strip_prefix("layout:set:") {
        let p = crate::layout::Preset::parse(name).unwrap_or(crate::layout::Preset::Single);
        let (order, active) = {
            let sh = shells_lock();
            (sh.ids(), sh.active())
        };
        let empty = layout_lock().set_preset(p, &order, active);
        log(&format!("layout -> {} ({} panes, {} empty)", p.name(), p.panes(), empty));
        after_layout_change(empty, active);
    } else if act == "pane:next" {
        pane_step(1);
    } else if act == "pane:prev" {
        pane_step(-1);
    } else if act == "pane:zoom" {
        pane_zoom();
    } else if let Some(i) = act.strip_prefix("pane:focus:") {
        on_pane_focus(i.parse().unwrap_or(0));
    } else if act == "shell:attention" {
        wv::post("{\"t\":\"attentionnext\"}");
    } else if let Some(expr) = act.strip_prefix("js:") {
        wv::cdp("Runtime.evaluate", &serde_json::json!({ "expression": expr }).to_string());
    } else if act == "session:new" {
        // The host creates a session; the page builds its view and answers "ready", which starts the shell.
        let id = NEXT_TEST_SESSION.fetch_add(1, Ordering::SeqCst);
        log(&format!("SESSION open {id}"));
        // Like a new shell: the layout shows it first, so its view is created on screen at a real size.
        layout_lock().place_new(id);
        push_layout();
        wv::post(&serde_json::json!({ "t": "open", "s": id }).to_string());
        pty::set_active(id);
    } else if let Some(v) = act.strip_prefix("session:show:") {
        let id: u32 = v.parse().unwrap_or(1);
        log(&format!("SESSION show {id}"));
        layout_lock().show(id);
        push_layout();
        wv::post(&serde_json::json!({ "t": "show", "s": id }).to_string());
        pty::set_active(id);
    } else if let Some(v) = act.strip_prefix("session:close:") {
        let id: u32 = v.parse().unwrap_or(1);
        log(&format!("SESSION close {id}"));
        pty::close(id);
        let others: Vec<u32> = pty::session_ids().into_iter().filter(|s| *s != id).collect();
        layout_lock().remove(id, &others, 0);
        push_layout();
        wv::post(&serde_json::json!({ "t": "close", "s": id }).to_string());
        if pty::active() == id {
            pty::set_active(pty::session_ids().first().copied().unwrap_or(1));
        }
    } else if let Some(rest) = act.strip_prefix("wto:") {
        // wto:<session>:<text> writes to one named session regardless of which is on screen.
        if let Some((id, text)) = rest.split_once(':') {
            pty::write_to(id.parse().unwrap_or(1), text.as_bytes().to_vec());
        }
    } else if let Some(rest) = act.strip_prefix("dumps:") {
        // dumps:<session>:<label> dumps one session's screen.
        if let Some((id, label)) = rest.split_once(':') {
            log(&format!("DUMP label={label}"));
            wv::post(&serde_json::json!({ "t": "dump", "s": id.parse::<u32>().unwrap_or(1) }).to_string());
        }
    } else if let Some(v) = act.strip_prefix("corner:") {
        unsafe { move_to_corner(hwnd, if v == "prev" { -1 } else { 1 }) };
    } else if act == "size" {
        unsafe { cycle_size(hwnd) };
    } else if let Some(v) = act.strip_prefix("font:") {
        match v {
            "reset" => reset_font(),
            "-" => change_font(-1),
            _ => change_font(1),
        }
    } else if let Some(v) = act.strip_prefix("moving:") {
        // Simulate a drag: a rectangle with the window's size at x,y goes through the real WM_MOVING handler.
        let mut p = v.split(',');
        let x: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let y: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        // x,y are relative to the top-left of the work area of the monitor the window is on, so the same script
        // works on any monitor (on the primary one the work area starts at 0,0).
        let work = unsafe { work_area(hwnd) };
        *lock_or_recover(&SNAP_DRAG) = None; // each simulated message stands alone (no drag state between them)
        let mut cur = RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut cur) };
        let mut r = RECT { left: x + work.l, top: y + work.t, right: x + work.l + (cur.right - cur.left), bottom: y + work.t + (cur.bottom - cur.top) };
        let res = unsafe { SendMessageW(hwnd, WM_MOVING, Some(WPARAM(0)), Some(LPARAM(&mut r as *mut RECT as isize))) };
        log(&format!("MOVING in={x},{y} out={},{},{},{} handled={}", r.left - work.l, r.top - work.t, r.right - work.l, r.bottom - work.t, res.0));
    } else if let Some(v) = act.strip_prefix("moveto:") {
        // Move the window (keeping its size) to absolute screen coordinates, e.g. onto another monitor.
        let mut p = v.split(',');
        let x: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let y: i32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let _ = unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
    } else if act == "ghost" {
        unsafe { toggle_ghost(hwnd) };
    } else if let Some(v) = act.strip_prefix("opacity:") {
        unsafe { set_opacity(hwnd, v.parse().unwrap_or(100), false) };
    } else if let Some(v) = act.strip_prefix("ct:") {
        CLICK_THROUGH.store(v == "on", Ordering::SeqCst);
        unsafe { refresh_pointer_style(hwnd) };
    } else if act == "toggle-ct" {
        unsafe { toggle_click_through(hwnd) };
    } else if act == "toggle-show" {
        unsafe { toggle_show(hwnd) };
    } else if let Some(v) = act.strip_prefix("focus-flag:") {
        // Simulation of the host flag only (nothing is activated); see the ATTENTION scenario note.
        TERMINAL_FOCUS.store(v == "1", Ordering::SeqCst);
        unsafe { refresh_opacity(hwnd) };
    } else if let Some(v) = act.strip_prefix("autostart:") {
        match v {
            "on" => {
                set_autostart(true);
            }
            "off" => {
                set_autostart(false);
            }
            _ => {}
        }
        log(&format!("AUTOSTART name={} enabled={}", autostart_name(), autostart_enabled()));
    } else if act == "rclick" || act == "ldrag" {
        // Mouse events delivered to the page through the DevTools protocol (no OS mouse input).
        let ev = |t: &str, x: i32, y: i32, button: &str, buttons: u32| {
            wv::cdp(
                "Input.dispatchMouseEvent",
                &serde_json::json!({ "type": t, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": 1 }).to_string(),
            );
        };
        if act == "rclick" {
            ev("mousePressed", 200, 120, "right", 2);
            ev("mouseReleased", 200, 120, "right", 0);
        } else {
            ev("mousePressed", 12, 34, "left", 1);
            ev("mouseMoved", 150, 34, "left", 1);
            ev("mouseMoved", 320, 34, "left", 1);
            ev("mouseReleased", 320, 34, "left", 0);
        }
    } else if act == "select-all" {
        wv::post("{\"t\":\"selectall\"}");
    } else if let Some(spec) = act.strip_prefix("realkey:") {
        // Real OS-level key (Ctrl+<letter>) - only ever sent while HUDshell itself is the foreground
        // window, so it can never reach another application.
        let fg = unsafe { GetForegroundWindow() };
        if fg != hwnd {
            log(&format!("SKIP realkey {spec}: HUDshell is not the foreground window"));
        } else if let Some(letter) = spec.strip_prefix("Ctrl+").and_then(|s| s.bytes().next()) {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
                VIRTUAL_KEY,
            };
            let k = |vk: u16, up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: 0,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let v = letter.to_ascii_uppercase() as u16;
            let inputs = [k(0x11, false), k(v, false), k(v, true), k(0x11, true)];
            log(&format!("sending real key {spec} to the focused HUDshell"));
            unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        }
    } else if act == "realclick" {
        if NO_GRAB.load(Ordering::SeqCst) {
            log("SKIP (--no-grab): real click");
        } else {
            unsafe { real_click_center(hwnd) };
        }
    } else if let Some(m) = act.strip_prefix("hit:") {
        if NO_GRAB.load(Ordering::SeqCst) {
            log("SKIP (--no-grab): hit test");
        } else {
            unsafe { hit_test(hwnd, m == "self") };
        }
    } else if act == "census" {
        for l in pty::census_report().lines() {
            log(l);
        }
    } else if act == "summary" {
        log("===== FOREGROUND WATCHDOG SUMMARY =====");
        if let Ok(v) = FG_CHANGES.lock() {
            let mut phases: Vec<String> = Vec::new();
            for (p, _) in v.iter() {
                if !phases.contains(p) {
                    phases.push(p.clone());
                }
            }
            for p in phases {
                let total = v.iter().filter(|(q, _)| *q == p).count();
                let to_self = v.iter().filter(|(q, s)| *q == p && *s).count();
                log(&format!("  phase {p}: {total} foreground change(s), {to_self} to HUDshell"));
            }
            if v.is_empty() {
                log("  no foreground changes at all");
            }
        }
    } else if act == "quit" {
        log("AUTOTEST: finished, closing");
        if perwin::count() == 0 {
            let _ = unsafe { DestroyWindow(hwnd) };
        }
        unsafe { quit_all() };
    }
}

pub(crate) unsafe fn at_tick(hwnd: HWND) {
    let Some(t0) = AT_T0.get() else { return };
    let now = std::time::Instant::now();
    if now < *t0 {
        return;
    }
    let elapsed = (now - *t0).as_millis() as u64;
    loop {
        let i = AT_IDX.load(Ordering::SeqCst);
        let sc = scen();
        if i >= sc.len() || sc[i].0 > elapsed {
            break;
        }
        AT_IDX.store(i + 1, Ordering::SeqCst);
        let act = sc[i].1;
        log(&format!("AUTOTEST t={}ms: {}", sc[i].0, act.replace('\r', "<CR>").replace('\n', "<LF>")));
        unsafe { run_action(hwnd, act) };
    }
}

pub(crate) unsafe fn fg_watchdog(hwnd: HWND) {
    let fg = unsafe { GetForegroundWindow() };
    let prev = LAST_FG.swap(fg.0 as isize, Ordering::SeqCst);
    if prev != fg.0 as isize {
        let to_self = fg == hwnd;
        let ph = cur_phase();
        log(&format!("FG-CHANGE [{ph}] -> {}", unsafe { describe(fg) }));
        if let Ok(mut v) = FG_CHANGES.lock() {
            v.push((ph, to_self));
        }
    }
}

/// Any of these switches turns on test mode: built-in default settings, the real config is never
/// read or written, and no tray icon is added.
/// Every scripted-scenario switch. ONE list, used both to arm the scenario and to decide test mode, so a
/// new scenario can never run against the user's real settings by forgetting to register itself.
const SCENARIO_FLAGS: &[&str] = &[
    "--autotest", "--keys", "--widthtest", "--probe", "--probe2", "--probe3", "--probe4", "--real",
    "--clickfocus", "--hideshow", "--clipboard", "--soak", "--scrolltest", "--ghosttest", "--attention",
    "--policytest", "--placetest", "--idletest", "--starttest", "--autostarttest", "--sessiontest", "--shellstest",
    "--tabmodes", "--shellsfull", "--layouttest", "--layoutempty", "--layoutstart", "--layoutown", "--labeltest", "--inputtest", "--dpitest", "--texttest", "--reloadtest", "--sessionsave", "--sessionload", "--startfailtest", "--recovertest", "--windowstest", "--windowsnap", "--windowssave", "--windowsload", "--windowsmove", "--costwindows",
];

pub(crate) fn is_test_mode() -> bool {
    const OTHER: &[&str] = &["--selftest", "--selftest2", "--idle"];
    std::env::args().any(|a| SCENARIO_FLAGS.contains(&a.as_str()) || OTHER.contains(&a.as_str()))
}

/// Timers owned by the harness (selftest 10-12, scenario 20, autotest 21, foreground watchdog 30,
/// cursor restore 31).
pub(crate) fn handles_timer(id: usize) -> bool {
    matches!(id, 10 | 11 | 12 | 20 | 21 | 30 | 31)
}

pub(crate) unsafe fn on_timer(hwnd: HWND, id: usize) {
    match id {
        20 => unsafe { scenario_tick(hwnd) },
        21 => unsafe { at_tick(hwnd) },
        30 => unsafe { fg_watchdog(hwnd) },
        31 => {
            let _ = unsafe { KillTimer(Some(hwnd), 31) };
            let _ = unsafe {
                SetCursorPos(OLD_CURSOR_X.load(Ordering::SeqCst), OLD_CURSOR_Y.load(Ordering::SeqCst))
            };
            log("test cursor position restored");
        }
        _ => {
            let _ = unsafe { KillTimer(Some(hwnd), id) };
            match id {
                10 => {
                    log("SELFTEST: writing 'echo HUDSHELL_SELFTEST_OK' to the shell");
                    pty::write(b"echo HUDSHELL_SELFTEST_OK; $PSVersionTable.PSVersion.ToString()\r".to_vec());
                }
                11 => {
                    log("SELFTEST: requesting screen dump from the page");
                    wv::post("{\"t\":\"dump\"}");
                }
                _ => {
                    log("SELFTEST: done, closing");
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
            }
        }
    }
}

/// Developer switches: `--pty-flags N` (ConPTY dwFlags), `--log-input` (log the bytes sent to the
/// shell) and `--unicode` / `--shift-enter` overrides of the configured values.
pub(crate) fn apply_cli_options(loaded: &config::Config) {
    let args: Vec<String> = std::env::args().collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    wv::set_options(
        &opt("--unicode").unwrap_or_else(|| loaded.unicode.clone()),
        &opt("--shift-enter").unwrap_or_else(|| loaded.shift_enter.clone()),
    );
    if let Some(f) = args.iter().position(|a| a == "--pty-flags").and_then(|i| args.get(i + 1)) {
        let v = f
            .strip_prefix("0x")
            .map(|h| u32::from_str_radix(h, 16).unwrap_or(0))
            .unwrap_or_else(|| f.parse().unwrap_or(0));
        pty::PTY_FLAGS.store(v, Ordering::SeqCst);
    }
    if args.iter().any(|a| a == "--log-input") {
        pty::LOG_INPUT.store(true, Ordering::SeqCst);
    }
}

/// Called once the web view is ready: starts whichever scripted run was asked for on the command line.
pub(crate) unsafe fn arm(hwnd: HWND) {
    let any_scen = |n: &str| std::env::args().any(|a| a == n);
    if SCENARIO_FLAGS.iter().any(|f| any_scen(f)) {
        let args: Vec<String> = std::env::args().collect();
        SCEN_SEL.store(
            if any_scen("--costwindows") { 42 } else if any_scen("--windowsmove") { 41 } else if any_scen("--windowsload") { 40 } else if any_scen("--windowssave") { 39 } else if any_scen("--windowsnap") { 38 } else if any_scen("--windowstest") { 37 } else if any_scen("--recovertest") { 36 } else if any_scen("--startfailtest") { 35 } else if any_scen("--sessionload") { 34 } else if any_scen("--sessionsave") { 33 } else if any_scen("--reloadtest") { 32 } else if any_scen("--texttest") { 31 } else if any_scen("--dpitest") { 30 } else if any_scen("--inputtest") { 29 } else if any_scen("--labeltest") { 28 } else if any_scen("--layoutown") { 27 } else if any_scen("--layoutstart") { 26 } else if any_scen("--layoutempty") { 25 } else if any_scen("--layouttest") { 24 } else if any_scen("--shellsfull") { 23 } else if any_scen("--tabmodes") { 22 } else if any_scen("--shellstest") { 21 } else if any_scen("--sessiontest") { 20 } else if any_scen("--autostarttest") { 19 } else if any_scen("--starttest") { 18 } else if any_scen("--idletest") { 17 } else if any_scen("--placetest") { 16 } else if any_scen("--policytest") { 15 } else if any_scen("--attention") { 14 } else if any_scen("--ghosttest") { 13 } else if any_scen("--scrolltest") { 12 } else if any_scen("--soak") { 11 } else if any_scen("--clipboard") { 10 } else if any_scen("--hideshow") { 9 } else if any_scen("--clickfocus") { 8 } else if any_scen("--real") { 7 } else if any_scen("--probe4") { 6 } else if any_scen("--probe3") { 5 } else if any_scen("--probe2") { 4 } else if any_scen("--probe") { 3 } else if any_scen("--widthtest") { 2 } else if any_scen("--keys") { 1 } else { 0 },
            Ordering::SeqCst,
        );
        let delay = args
            .iter()
            .position(|a| a == "--delay")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        NO_GRAB.store(args.iter().any(|a| a == "--no-grab"), Ordering::SeqCst);
        log(&format!("AUTOTEST armed: starts in {delay}s, no-grab={}", NO_GRAB.load(Ordering::SeqCst)));
        let _ = AT_T0.set(std::time::Instant::now() + std::time::Duration::from_secs(delay));
        let _ = unsafe { SetTimer(Some(hwnd), 21, 150, None) };
        let _ = unsafe { SetTimer(Some(hwnd), 30, 300, None) };
    } else if any_scen("--selftest2") {
        log("SELFTEST2 armed: scripted scenario");
        let _ = SCENARIO_T0.set(std::time::Instant::now());
        let _ = unsafe { SetTimer(Some(hwnd), 20, 200, None) };
    } else if any_scen("--selftest") {
        log("SELFTEST armed: type via pty at +4s, dump at +7s, quit at +9s");
        let _ = unsafe { SetTimer(Some(hwnd), 10, 4000, None) };
        let _ = unsafe { SetTimer(Some(hwnd), 11, 7000, None) };
        let _ = unsafe { SetTimer(Some(hwnd), 12, 9000, None) };
    }
}
/// Test-only overrides of the (default) test configuration: `--osc52 write`, `--right-click paste`,
/// `--copy-on-select`, `--no-cue`, `--no-toast`, `--ghost-opacity N`.
pub(crate) fn override_config(mut c: config::Config) -> config::Config {
    let args: Vec<String> = std::env::args().collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if let Some(v) = opt("--osc52") {
        c.osc52 = v;
    }
    if let Some(v) = opt("--right-click") {
        c.right_click = v;
    }
    if args.iter().any(|a| a == "--copy-on-select") {
        c.copy_on_select = true;
    }
    if args.iter().any(|a| a == "--no-cue") {
        c.attention_cue = false;
    }
    if args.iter().any(|a| a == "--no-toast") {
        c.attention_toast = false;
    }
    if let Some(v) = opt("--ghost-opacity").and_then(|v| v.parse().ok()) {
        c.ghost_opacity = v;
    }
    if let Some(v) = opt("--opacity").and_then(|v| v.parse().ok()) {
        c.opacity = v;
    }
    if let Some(v) = opt("--active-opacity").and_then(|v| v.parse().ok()) {
        c.active_opacity = Some(v);
    }
    if let Some(v) = opt("--idle-dim").and_then(|v| v.parse().ok()) {
        c.idle_dim_seconds = v;
    }
    if let Some(v) = opt("--idle-opacity").and_then(|v| v.parse().ok()) {
        c.idle_opacity = v;
    }
    if args.iter().any(|a| a == "--start-hidden") {
        c.start_hidden = true;
    }
    if args.iter().any(|a| a == "--start-ghost") {
        c.start_ghost = true;
    }
    if let Some(v) = opt("--tabs") {
        c.tabs = v;
    }
    let pair = |v: String| -> Option<[u32; 2]> {
        let (a, b) = v.split_once(',')?;
        Some([a.trim().parse().ok()?, b.trim().parse().ok()?])
    };
    if let Some(v) = opt("--layout") {
        c.layout = v;
    }
    if let Some(v) = opt("--layout-fill") {
        c.layout_fill = v;
    }
    if let Some(v) = opt("--pane-shells") {
        c.pane_shells = v;
    }
    // --at x,y[,w,h]: where the window starts, in physical pixels (to start it on another monitor).
    if let Some(v) = opt("--at") {
        let n: Vec<i32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if n.len() >= 2 {
            c.window = Some(config::Geometry { x: n[0], y: n[1], w: n.get(2).copied().unwrap_or(560), h: n.get(3).copied().unwrap_or(300) });
        }
    }
    if let Some(v) = opt("--tab-program") {
        c.tab_program = v;
    }
    if let Some(v) = opt("--restore-session") {
        c.restore_session = v;
    }
    // --session-file <path>: where the session is kept in a test run
    if let Some(v) = opt("--session-file") {
        *lock_or_recover(&SESSION_FILE) = Some(std::path::PathBuf::from(v));
    }
    if let Some(v) = opt("--text-scale").and_then(|v| v.parse().ok()) {
        c.text_scale = v;
    }
    if let Some(v) = opt("--ui-scale").and_then(|v| v.parse().ok()) {
        c.ui_scale = v;
    }
    if let Some(v) = opt("--line-height").and_then(|v| v.parse().ok()) {
        c.line_height = v;
    }
    if let Some(v) = opt("--letter-spacing").and_then(|v| v.parse().ok()) {
        c.letter_spacing = v;
    }
    if let Some(v) = opt("--text-hotkeys") {
        c.text_scale_hotkeys = v;
    }
    if let Some(v) = opt("--split").and_then(pair) {
        c.split = v;
    }
    if let Some(v) = opt("--pane-min").and_then(pair) {
        c.pane_min = v;
    }
    if args.iter().any(|a| a == "--dividers") {
        c.pane_dividers = true;
    }
    if args.iter().any(|a| a == "--test-profiles") {
        c.profiles = vec![
            config::Profile::default(),
            config::Profile { name: "work".into(), dir: Some("%TEMP%".into()), color: Some("#12ab34".into()), ..config::Profile::default() },
            config::Profile { name: "cmd".into(), command: Some("echo PROFILE-COMMAND-RAN".into()), ..config::Profile::default() },
        ];
    }
    c
}
