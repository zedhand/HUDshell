//! Test harness. Compiled ONLY with `--features harness`; a normal build contains none of this.
//! Scripted scenarios, DevTools-protocol key injection, real hotkey/mouse injectors, the
//! foreground watchdog and the developer command-line switches live here so they cannot ship
//! in a release build.
#![allow(dead_code)]

use super::*;
/// Scripted terminal scenario (`--term --selftest2`): (ms from start, action).
/// Actions: "w:<text>" write to the shell, "ctrlc", "dump:<label>", "resize:W,H", "quit".
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
            s.at(400, "type:The second line must contain the word glint.");
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

fn scen() -> &'static [(u64, &'static str)] {
    match SCEN_SEL.load(Ordering::SeqCst) {
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
        let _ = unsafe {
            SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)
        };
    } else if act.starts_with("label:") {
        // marker only
    } else if let Some(rest) = act.strip_prefix("skipif:") {
        let (alts, label) = rest.split_once('|').unwrap_or((rest, ""));
        let dump = wv::last_dump().to_lowercase();
        if let Some(hit) = alts.split(';').find(|a| dump.contains(&a.to_lowercase())) {
            if let Some(i) = scen().iter().position(|(_, a)| *a == format!("label:{label}")) {
                log(&format!("SKIP: screen contains '{hit}' -> jumping to label {label} (nothing was answered)"));
                AT_IDX.store(i, Ordering::SeqCst);
            }
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
        let _ = unsafe { DestroyWindow(hwnd) };
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
pub(crate) fn is_test_mode() -> bool {
    const FLAGS: &[&str] = &[
        "--autotest", "--keys", "--widthtest", "--probe", "--probe2", "--probe3", "--probe4",
        "--real", "--clickfocus", "--selftest", "--selftest2", "--hideshow", "--clipboard", "--soak",
        "--idle",
    ];
    std::env::args().any(|a| FLAGS.contains(&a.as_str()))
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
    if any_scen("--autotest") || any_scen("--keys") || any_scen("--widthtest") || any_scen("--probe") || any_scen("--probe2") || any_scen("--probe3") || any_scen("--probe4") || any_scen("--real") || any_scen("--clickfocus") || any_scen("--hideshow") || any_scen("--clipboard") || any_scen("--soak") {
        let args: Vec<String> = std::env::args().collect();
        SCEN_SEL.store(
            if any_scen("--soak") { 11 } else if any_scen("--clipboard") { 10 } else if any_scen("--hideshow") { 9 } else if any_scen("--clickfocus") { 8 } else if any_scen("--real") { 7 } else if any_scen("--probe4") { 6 } else if any_scen("--probe3") { 5 } else if any_scen("--probe2") { 4 } else if any_scen("--probe") { 3 } else if any_scen("--widthtest") { 2 } else if any_scen("--keys") { 1 } else { 0 },
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