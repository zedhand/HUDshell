<#
HUDshell full test runner (Windows PowerShell 5.1 or PowerShell 7).

  scripts\test-all.ps1 [-Suite static,keys,width,hideshow,clickfocus,clipboard,autotest,ui,scrollback,ghost,attention,policy,release,config,soak,real | all]
                       [-Sandbox <folder both Claude Code and Codex already trust>] [-SoakSeconds 300]

What it touches (read before running):
  * `ui`, `clickfocus`, `autotest`, `clipboard`, `hideshow` send real hotkeys (Ctrl+Alt+<letter>) and some
    use real mouse/keyboard input, so do not use the keyboard or mouse while they run. The mouse is put back.
  * `clipboard` temporarily replaces the clipboard TEXT (and restores it afterwards).
  * `config` and `release` run the normal build against the real settings folder; an existing config.json is
    backed up and restored (or removed if none existed).
  * `real` sends real prompts through Claude Code and Codex in -Sandbox (uses a little of your usage).
Needs: `npm run build` done, harness build (`cargo build --features harness`) and a release build
(`scripts\build-release.ps1`). Results are printed as a table; exit code is the number of failures.
#>
param(
  [string[]]$Suite = @('all'),
  [string]$Sandbox = '',
  [int]$SoakSeconds = 300
)
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$dbg = Join-Path $root 'target\debug\hudshell.exe'
$rel = Join-Path $root 'target\release\hudshell.exe'
$log = Join-Path $root 'hudshell-test.log'
$prodLog = Join-Path $env:LOCALAPPDATA 'HUDshell\hudshell.log'
$cfgDir = Join-Path $env:APPDATA 'HUDshell'
$cfgPath = Join-Path $cfgDir 'config.json'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('hudshell-tests-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force $scratch | Out-Null
$env:HUDSHELL_TEST_DIR = $scratch
if ($Sandbox) { $env:HUDSHELL_SANDBOX = $Sandbox } else { $env:HUDSHELL_SANDBOX = $scratch }
$Suite = @($Suite | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$want = if ($Suite -contains 'all') { @('static', 'keys', 'width', 'hideshow', 'clickfocus', 'clipboard', 'autotest', 'ui', 'scrollback', 'ghost', 'attention', 'policy', 'release', 'config', 'soak') + $(if ($Sandbox) { 'real' } else { @() }) } else { $Suite }

$script:R = New-Object System.Collections.Generic.List[object]
function Check([string]$suite, [string]$name, [bool]$ok, [string]$detail = '') {
  $status = if ($ok) { 'PASS' } else { 'FAIL' }
  $script:R.Add([pscustomobject]@{ Suite = $suite; Check = $name; Status = $status; Detail = $detail })
  Write-Host ("[{0}] {1}: {2} {3}" -f $status, $suite, $name, $detail)
}
function Warn([string]$suite, [string]$name, [string]$detail = '') {
  $script:R.Add([pscustomobject]@{ Suite = $suite; Check = $name; Status = 'WARN'; Detail = $detail })
  Write-Host ("[WARN] {0}: {1} {2}" -f $suite, $name, $detail)
}

$drawingRef = if ($PSVersionTable.PSEdition -eq 'Core') { 'System.Drawing.Primitives' } else { 'System.Drawing' }
Add-Type -ReferencedAssemblies $drawingRef -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Drawing;
public static class GT {
  [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte sc, uint fl, UIntPtr ex);
  [DllImport("user32.dll")] static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte a, out uint fl);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static Point Saved;
  public static void SaveCursor() { POINT p; GetCursorPos(out p); Saved = new Point(p.X, p.Y); }
  public static void RestoreCursor() { SetCursorPos(Saved.X, Saved.Y); }
  public static void Hotkey(byte vk) { keybd_event(0x11,0,0,UIntPtr.Zero); keybd_event(0x12,0,0,UIntPtr.Zero); keybd_event(vk,0,0,UIntPtr.Zero); keybd_event(vk,0,2,UIntPtr.Zero); keybd_event(0x12,0,2,UIntPtr.Zero); keybd_event(0x11,0,2,UIntPtr.Zero); }
  public static void Key(byte vk) { keybd_event(vk,0,0,UIntPtr.Zero); keybd_event(vk,0,2,UIntPtr.Zero); }
  public static void MoveTo(int x, int y) { SetCursorPos(x, y); }
  public static void MouseDown() { mouse_event(0x2,0,0,0,UIntPtr.Zero); }
  public static void MouseUp() { mouse_event(0x4,0,0,0,UIntPtr.Zero); }
  public static Point Origin(IntPtr h) { RECT r; GetWindowRect(h, out r); return new Point(r.L, r.T); }
  public static IntPtr Find(uint pid) { IntPtr found = IntPtr.Zero; EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p==pid){ var sb=new StringBuilder(64); GetClassName(h,sb,64); if(sb.ToString().ToLower().Contains("hudshell")){found=h; return false;} } return true; }, IntPtr.Zero); return found; }
  public static int Alpha(IntPtr h) { uint k; byte a; uint f; GetLayeredWindowAttributes(h,out k,out a,out f); return a; }
  public static bool IsFg(IntPtr h) { return GetForegroundWindow() == h; }
}
'@

function Stop-HUDshell { Get-Process hudshell -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 400 }
function Read-Lines([string]$p) { if (Test-Path -LiteralPath $p) { [IO.File]::ReadAllLines($p, [Text.Encoding]::UTF8) } else { @() } }
function Cut([string]$p, [int]$from) { $a = Read-Lines $p; if ($a.Count -le $from) { return @() }; return $a[$from..($a.Count - 1)] }
function LineCount([string]$p) { (Read-Lines $p).Count }
function Strip($l) { $l -replace '^\d+ ', '' }

# Runs a harness scenario and returns the log lines it produced.
function Run-Scenario([string[]]$argv, [int]$timeoutMs = 120000) {
  Stop-HUDshell
  $n = LineCount $log
  $p = Start-Process $dbg -ArgumentList $argv -PassThru
  $null = $p.WaitForExit($timeoutMs)
  $timedOut = -not $p.HasExited
  if ($timedOut) { Stop-Process -Id $p.Id -Force }
  $run = @(Cut $log $n)
  if ($timedOut) { Warn 'harness' ("scenario " + ($argv -join ' ') + " did not finish in time") }
  return , $run
}
# Screen text captured by a "dump:<label>" action. The page answers asynchronously, so other log
# lines can come in between: take the rows after the first "terminal dump:" header that follows the request.
function Get-Dump($run, [string]$label) {
  $i = -1
  for ($k = 0; $k -lt $run.Count; $k++) { if ($run[$k] -match ("DUMP label=" + [regex]::Escape($label) + '(\s|$)')) { $i = $k; break } }
  if ($i -lt 0) { return @() }
  $h = -1
  for ($k = $i + 1; $k -lt $run.Count; $k++) { if ($run[$k] -match '^\d+ terminal dump:') { $h = $k; break } }
  if ($h -lt 0) { return @() }
  $out = New-Object System.Collections.Generic.List[string]
  for ($j = $h + 1; $j -lt $run.Count; $j++) {
    if ($run[$j] -match '^\d+ terminal dump:') { break }
    if ($run[$j] -match '^\d+   \| ?(.*)$') { $out.Add($Matches[1]) }
  }
  return $out.ToArray()
}function Dump-Has($run, [string]$label, [string]$text) { (@(Get-Dump $run $label) -join "`n") -match [regex]::Escape($text) }
function Dump-Count($run, [string]$label, [string]$text) { ([regex]::Matches((@(Get-Dump $run $label) -join "`n"), [regex]::Escape($text))).Count }

# ---------------------------------------------------------------------------------------------
function Suite-Static {
  Write-Host "=== static ==="
  $o = & cargo test 2>&1 | ForEach-Object { $_.ToString() }
  $line = ($o | Select-String 'test result:' | Select-Object -First 1)
  Check 'static' 'cargo unit tests' ($line -and $line.ToString() -match 'ok\. \d+ passed; 0 failed') ($line.ToString().Trim())
  Stop-HUDshell
  $b = [IO.File]::ReadAllBytes($rel)
  $ascii = [Text.Encoding]::ASCII.GetString($b); $uni = [Text.Encoding]::Unicode.GetString($b)
  function InExe($s) { $ascii.Contains($s) -or $uni.Contains($s) }
  Check 'static' 'release exe size under 3 MB' ($b.Length -lt 3MB) ("{0:N2} MB" -f ($b.Length / 1MB))
  foreach ($s in @('AUTOTEST', 'SELFTEST', 'dispatchKeyEvent', '--autotest', '--log-input', '--pty-flags', 'HUDSHELL_SANDBOX', 'hudshell-r1')) {
    Check 'static' "release exe has no test code string '$s'" (-not (InExe $s))
  }
  Check 'static' 'release exe carries no former project name' (-not ((InExe ('gl' + 'intty')) -or (InExe ('Gl' + 'intty'))))
  foreach ($s in @($env:USERNAME, $env:USERPROFILE, $root)) {
    if ($s) { Check 'static' "release exe has no build-machine string '$s'" (-not (InExe $s)) }
  }
  # Debug (harness) binary must have the harness; sanity check the feature split really differs.
  $db = [IO.File]::ReadAllBytes($dbg)
  Check 'static' 'harness build does contain the harness' ([Text.Encoding]::ASCII.GetString($db).Contains('AUTOTEST'))
  # Every UI file is embedded byte-for-byte (search the exe for the whole file, not just a prefix).
  $lat = [Text.Encoding]::GetEncoding(28591)
  $exeText = $lat.GetString($b)
  $dist = Join-Path $root 'web\dist'
  $bad = @()
  foreach ($f in 'index.html', 'app.js', 'xterm.js', 'xterm.css', 'addon-fit.js', 'addon-unicode11.js', 'addon-unicode-graphemes.js') {
    $bytes = [IO.File]::ReadAllBytes((Join-Path $dist $f))
    if (-not $exeText.Contains($lat.GetString($bytes))) { $bad += $f }
  }
  Check 'static' 'every UI file is embedded in the release exe byte-for-byte' ($bad.Count -eq 0) ($bad -join ',')
}
function Suite-Keys {
  Write-Host "=== keys ==="
  $run = Run-Scenario @('--term', '--keys', '--log-input', '--delay', '4') 70000
  $rows = @()
  foreach ($l in $run) {
    if ($l -match 'AUTOTEST t=\d+ms: key:(\S+)') { $rows += [pscustomobject]@{ Key = $Matches[1]; Bytes = '' } }
    elseif ($l -match 'PTY-IN \d+ byte\(s\): (.*)$' -and $rows.Count) { $rows[-1].Bytes = ($rows[-1].Bytes + ' | ' + $Matches[1]).Trim(' |') }
  }
  $want = [ordered]@{ 'Enter' = '0d'; 'Shift+Enter' = '1b 0d'; 'Backspace' = '7f'; 'Ctrl+C' = '03'; 'Esc' = '1b'; 'Up' = '1b 5b 41' }
  foreach ($k in $want.Keys) {
    $got = ($rows | Where-Object { $_.Key -eq $k } | Select-Object -First 1).Bytes
    Check 'keys' "bytes for $k" ($got -eq $want[$k]) "got '$got' want '$($want[$k])'"
  }
  Write-Host ("  observed table: " + (($rows | ForEach-Object { "$($_.Key)=$($_.Bytes)" }) -join '; '))
}

function Suite-Width {
  Write-Host "=== width ==="
  $run = Run-Scenario @('--term', '--widthtest', '--delay', '4') 70000
  $lines = @(Get-Dump $run 'width' | Where-Object { $_ -match '\|cl=\d+' -and $_ -notmatch 'PS ' })
  $ok = @($lines | Where-Object { $_ -match '^.+? memory runs XXXXXXXX \|cl=\d+\s*$' }).Count
  Check 'width' 'glyph/emoji lines stay aligned' ($lines.Count -ge 15 -and $ok -eq $lines.Count) "$ok of $($lines.Count)"
  Check 'width' 'UI served from the embedded copy' (@($run | Where-Object { $_ -match 'asset served from memory: /index.html -> 200' }).Count -ge 1)
  Check 'width' 'no navigation to anything outside the app' (@($run | Where-Object { $_ -match 'navigation blocked|new window blocked' }).Count -eq 0)
}

function Suite-HideShow {
  Write-Host "=== hideshow ==="
  $run = Run-Scenario @('--term', '--autotest', '--hideshow', '--delay', '4') 90000
  $nums = @(Get-Dump $run 'after-show' | Where-Object { $_ -match '^line \d+$' } | ForEach-Object { [int]($_ -replace 'line ', '') })
  Check 'hideshow' 'output produced while hidden is all there after show (last line 40 present)' ($nums -contains 40) "visible lines $($nums.Count)"
  $consecutive = $nums.Count -gt 5
  for ($k = 1; $k -lt $nums.Count; $k++) { if ($nums[$k] -ne $nums[$k - 1] + 1) { $consecutive = $false } }
  Check 'hideshow' 'lines are consecutive (nothing lost in the middle)' $consecutive
  Check 'hideshow' 'window was hidden then shown' (@($run | Where-Object { $_ -match 'visible = ' }).Count -ge 2)
}

function Suite-ClickFocus {
  Write-Host "=== clickfocus ==="
  GTSave
  try { $run = Run-Scenario @('--term', '--autotest', '--clickfocus', '--delay', '10') 120000 } finally { [GT]::RestoreCursor() }
  $h = @($run | Where-Object { $_ -match 'HIT-TEST' })
  Check 'clickfocus' 'hit tests (click-through vs interactive)' (@($h | Where-Object { $_ -match '-> PASS' }).Count -ge 4 -and @($h | Where-Object { $_ -match '-> FAIL' }).Count -eq 0) ("pass=" + @($h | Where-Object { $_ -match '-> PASS' }).Count + " fail=" + @($h | Where-Object { $_ -match '-> FAIL' }).Count)
  Check 'clickfocus' 'a real click on the interactive terminal took focus' (@($run | Where-Object { $_ -match 'click on the interactive terminal' }).Count -ge 1)
  Check 'clickfocus' 'grab/release cycles reached the foreground' (@($run | Where-Object { $_ -match 'foreground==HUDshell: true' }).Count -ge 2)
}
function GTSave { [GT]::SaveCursor() }

function Suite-Clipboard {
  Write-Host "=== clipboard ==="
  $had = $null
  try { $had = Get-Clipboard -Raw -ErrorAction Stop } catch { $had = $null }
  try {
    Set-Clipboard -Value 'echo CLIPBOARD-PASTE-OK'
    $run = Run-Scenario @('--term', '--autotest', '--clipboard', '--delay', '3') 60000
    Check 'clipboard' 'Ctrl+V pasted the clipboard text into the shell (typed + output)' ((Dump-Count $run 'after-paste' 'CLIPBOARD-PASTE-OK') -ge 2) ("occurrences " + (Dump-Count $run 'after-paste' 'CLIPBOARD-PASTE-OK'))
    $cp = @($run | Where-Object { $_ -match 'copy to clipboard' })
    Check 'clipboard' 'Ctrl+Shift+C reported a copy with no error' ($cp.Count -ge 1 -and ($cp -join ' ') -notmatch 'error')
    $now = ''
    try { $now = Get-Clipboard -Raw } catch { }
    Check 'clipboard' 'clipboard now holds the selected screen text' ($now -match 'CLIPBOARD-PASTE-OK')
  } finally {
    if ($null -ne $had) { Set-Clipboard -Value $had } else { Set-Clipboard -Value ' ' }
  }
}

function Suite-Autotest {
  Write-Host "=== autotest (typing, Claude, Codex start/exit, focus, click-through; about 80 s) ==="
  GTSave
  try { $run = Run-Scenario @('--term', '--autotest', '--delay', '3') 180000 } finally { [GT]::RestoreCursor() }
  Check 'autotest' 'typed echo reaches the shell' (Dump-Has $run 'kbd-result' 'KEYTEST-1')
  Check 'autotest' 'Backspace edits the line (abc, not abcXYZ)' ((Dump-Has $run 'kbd-result' 'abc') -and -not (Dump-Has $run 'kbd-result' 'abcX'))
  Check 'autotest' 'history recall (Up) re-runs the command' ((Dump-Count $run 'kbd-result' 'recall-me') -ge 3)
  Check 'autotest' 'single-line paste' (Dump-Has $run 'kbd-result' 'PASTE-SINGLE')
  Check 'autotest' 'multi-line paste runs both lines' ((Dump-Has $run 'kbd-result' 'ML1') -and (Dump-Has $run 'kbd-result' 'ML2'))
  Check 'autotest' 'Ctrl+C interrupts Start-Sleep and the prompt works after' (Dump-Has $run 'kbd-result' 'CTRLC-OK')
  foreach ($prog in 'claude', 'codex') {
    $skipped = @($run | Where-Object { $_ -match 'SKIP' -and $_ -match $prog }).Count -gt 0
    $start = (@(Get-Dump $run "$prog-start") -join "`n")
    if ($start.Trim().Length -lt 20) { Warn 'autotest' "$prog start screen empty (not installed?)"; continue }
    Check 'autotest' "$prog started and drew its screen" ($start -match $prog)
    Check 'autotest' "$prog exited back to the PowerShell prompt" (Dump-Has $run "$prog-exited" 'PS ') ''
    if ($skipped) { Warn 'autotest' "${prog}: a step was skipped (dialog or protocol guard)" }
  }
  foreach ($n in 1, 2, 3) { Check 'autotest' "grab cycle $n typed text" (Dump-Has $run 'after-grab-cycles' "GRAB-$n") }
  $h = @($run | Where-Object { $_ -match 'HIT-TEST' })
  Check 'autotest' 'click-through hit tests' ($h.Count -ge 3 -and @($h | Where-Object { $_ -match '-> FAIL' }).Count -eq 0) ("tests " + $h.Count)
  foreach ($ph in 'baseline', 'passive-keyboard', 'claude', 'codex') {
    $s = $run | Where-Object { $_ -match "phase ${ph}: \d+ foreground change" } | Select-Object -First 1
    if ($s -and $s -match '(\d+) to HUDshell') { Check 'autotest' "no focus stolen in phase '$ph'" ([int]$Matches[1] -eq 0) (Strip $s) }
    else { Check 'autotest' "no focus stolen in phase '$ph'" $true 'no foreground changes recorded' }
  }
}

function Suite-Ui {
  Write-Host "=== ui: opacity hotkeys, settings panel keys and mouse (do not touch mouse/keyboard) ==="
  Stop-HUDshell
  GTSave
  $p = $null
  try {
    $n = LineCount $log
    $p = Start-Process $dbg -ArgumentList '--idle' -PassThru
    Start-Sleep 6
    $h = [GT]::Find([uint32]$p.Id)
    Check 'ui' 'window found' ($h -ne [IntPtr]::Zero)
    Check 'ui' 'starts fully opaque' ([GT]::Alpha($h) -eq 255) ([GT]::Alpha($h))
    function Press($vk, $c) { for ($i = 0; $i -lt $c; $i++) { [GT]::Hotkey($vk); Start-Sleep -Milliseconds 400 } }
    $LB = 0xDB; $RB = 0xDD
    Press $LB 1; Check 'ui' 'Ctrl+Alt+[ lowers to 90%' ([GT]::Alpha($h) -eq 229) ([GT]::Alpha($h))
    Press $LB 3; Check 'ui' 'four presses give 60%' ([GT]::Alpha($h) -eq 153) ([GT]::Alpha($h))
    Press $RB 2; Check 'ui' 'Ctrl+Alt+] raises to 80%' ([GT]::Alpha($h) -eq 204) ([GT]::Alpha($h))
    Press $LB 12; Check 'ui' 'cannot go below the 20% floor' ([GT]::Alpha($h) -eq 51) ([GT]::Alpha($h))
    Press $RB 10; Check 'ui' 'back to 100%' ([GT]::Alpha($h) -eq 255) ([GT]::Alpha($h))
    Check 'ui' 'opacity changes never took focus' (-not [GT]::IsFg($h))
    [GT]::Hotkey(0x58); Start-Sleep -Milliseconds 500
    Check 'ui' 'Ctrl+Alt+X (ghost) dims to 40%' ([GT]::Alpha($h) -eq 102) ([GT]::Alpha($h))
    [GT]::Hotkey(0x58); Start-Sleep -Milliseconds 500
    Check 'ui' 'a second Ctrl+Alt+X restores 100% and never took focus' (([GT]::Alpha($h) -eq 255) -and -not [GT]::IsFg($h)) ([GT]::Alpha($h))
    [GT]::Hotkey(0x53); Start-Sleep -Milliseconds 1500
    Check 'ui' 'Ctrl+Alt+S opens the panel and takes keyboard focus' ([GT]::IsFg($h))
    [GT]::Key(0x25); [GT]::Key(0x25); Start-Sleep -Milliseconds 700
    Check 'ui' 'Left x2 lowers opacity by 10%' ([GT]::Alpha($h) -eq 229) ([GT]::Alpha($h))
    [GT]::Key(0x28); [GT]::Key(0x28); Start-Sleep -Milliseconds 700
    Check 'ui' 'Down x2 moves the colour theme two steps' (@(Cut $log $n | Where-Object { $_ -match 'colour theme -> .* \(settings panel\)' }).Count -eq 2)
    $o = [GT]::Origin($h); $ox = $o.X; $oy = $o.Y
    function Click($x, $y) { [GT]::MoveTo($ox + $x, $oy + $y); Start-Sleep -Milliseconds 250; [GT]::MouseDown(); Start-Sleep -Milliseconds 120; [GT]::MouseUp(); Start-Sleep -Milliseconds 600 }
    $cw = 8.4; $b0 = 327; $by = 82
    $before = [GT]::Alpha($h)
    Click ([int]($b0 + 4.5 * $cw)) $by
    $a1 = [GT]::Alpha($h)
    Check 'ui' 'mouse click on the opacity bar sets a lower value' ($a1 -lt $before -and $a1 -ge 51 -and $a1 -le 140) "$before -> $a1"
    Click ([int]($b0 + 16.5 * $cw)) $by
    $a2 = [GT]::Alpha($h)
    Check 'ui' 'mouse click further right raises it' ($a2 -gt $a1) "$a1 -> $a2"
    [GT]::MoveTo($ox + [int]($b0 + 2.5 * $cw), $oy + $by); Start-Sleep -Milliseconds 250; [GT]::MouseDown(); Start-Sleep -Milliseconds 150
    for ($i = 0; $i -le 12; $i++) { [GT]::MoveTo($ox + [int]($b0 + (2.5 + $i) * $cw), $oy + $by); Start-Sleep -Milliseconds 60 }
    [GT]::MouseUp(); Start-Sleep -Milliseconds 700
    $a3 = [GT]::Alpha($h)
    Check 'ui' 'dragging across the bar changes the value' ($a3 -gt 150) "after drag alpha $a3"
    $t0 = @(Cut $log $n | Where-Object { $_ -match 'colour theme ->' }).Count
    Click 459 101
    Check 'ui' 'clicking the colour arrow steps the theme' (@(Cut $log $n | Where-Object { $_ -match 'colour theme ->' }).Count -eq $t0 + 1)
    Click 315 173
    Check 'ui' 'clicking [close] closes the panel and returns focus' ((@(Cut $log $n | Where-Object { $_ -match 'settings panel closed' }).Count -ge 1) -and -not [GT]::IsFg($h))
    [GT]::Hotkey(0x53); Start-Sleep -Milliseconds 1200
    [GT]::Key(0x1B); Start-Sleep -Milliseconds 1000
    Check 'ui' 'Esc closes the panel and returns focus' (-not [GT]::IsFg($h))
    [GT]::Hotkey(0x51); $null = $p.WaitForExit(8000)
    Check 'ui' 'quit hotkey exits the app' $p.HasExited
  } finally {
    [GT]::RestoreCursor()
    if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  }
}

function Backup-Config { $script:cfgExtras = @(Get-ChildItem $cfgDir -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'config.json.*' } | ForEach-Object { $_.Name }); $script:hadCfg = Test-Path -LiteralPath $cfgPath; if ($script:hadCfg) { $script:cfgBak = Join-Path $scratch 'config.backup.json'; Copy-Item $cfgPath $script:cfgBak -Force; $script:cfgHash = (Get-FileHash $cfgPath).Hash } }
function Restore-Config {
  Stop-HUDshell
  if ($script:hadCfg) {
    # Put the user's file back, and remove what the tests themselves left beside it (.bad-N, .tmp) -
    # but never a file that was already there before the tests started.
    foreach ($f in Get-ChildItem $cfgDir -Force -ErrorAction SilentlyContinue) { if ($f.Name -like 'config.json.*' -and $script:cfgExtras -notcontains $f.Name) { Remove-Item -LiteralPath $f.FullName -Force } }
    Copy-Item $script:cfgBak $cfgPath -Force
  } else { foreach ($f in Get-ChildItem $cfgDir -Force -ErrorAction SilentlyContinue) { if ($f.Name -like 'config.json*') { Remove-Item -LiteralPath $f.FullName -Force } } }
}
function Start-Rel([string]$exe = $rel) { $p = Start-Process $exe -PassThru; Start-Sleep 7; return $p }
function Quit-Rel($p) { [GT]::Hotkey(0x51); $null = $p.WaitForExit(8000); if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }

function Suite-Release {
  Write-Host "=== release: normal build behaviour ==="
  Stop-HUDshell
  Backup-Config
  try {
    # Runs from an unrelated folder, ignores every test switch, logs to the app-data folder.
    $copy = Join-Path $scratch 'relcopy'; New-Item -ItemType Directory -Force $copy | Out-Null
    Copy-Item $rel (Join-Path $copy 'hudshell.exe') -Force
    Remove-Item -LiteralPath $prodLog -ErrorAction SilentlyContinue
    $p = Start-Process (Join-Path $copy 'hudshell.exe') -ArgumentList '--autotest', '--keys', '--soak', '5', '--real', '--log-input', '--pty-flags', '0x10', '--idle' -PassThru
    Start-Sleep 8
    Check 'release' 'runs from an unrelated folder with the UI embedded' (-not $p.HasExited)
    $l = Read-Lines $prodLog
    Check 'release' 'pty started a shell' (@($l | Where-Object { $_ -match 'pty: started .*pwsh' }).Count -ge 1)
    Check 'release' 'test switches are ignored (no scenario lines)' (@($l | Where-Object { $_ -match 'AUTOTEST|SELFTEST|SCENARIO' }).Count -eq 0)
    Check 'release' 'keystroke logging is off' (@($l | Where-Object { $_ -match 'PTY-IN' }).Count -eq 0)
    Check 'release' 'ConPTY flags stay 0x0' (@($l | Where-Object { $_ -match 'CreatePseudoConsole flags=0x0' }).Count -ge 1)
    Check 'release' 'tray icon added (normal mode)' (@($l | Where-Object { $_ -match 'tray icon added: true' }).Count -ge 1)
    Check 'release' 'window titles are not logged' (@($l | Where-Object { $_ -match "title='[^(]" }).Count -eq 0)
    Check 'release' 'DevTools/shortcuts/context menu disabled' (@($l | Where-Object { $_ -match 'DevTools, browser shortcuts, context menu, zoom and swipe disabled' }).Count -ge 1)
    # Second launch exits quickly (single instance).
    $p2 = Start-Process (Join-Path $copy 'hudshell.exe') -PassThru
    $null = $p2.WaitForExit(6000)
    Check 'release' 'a second launch exits (single instance)' $p2.HasExited
    if (-not $p2.HasExited) { Stop-Process -Id $p2.Id -Force }
    $h = [GT]::Find([uint32]$p.Id)
    Check 'release' 'window is click-through and passive by default' (-not [GT]::IsFg($h))
    Quit-Rel $p
    Check 'release' 'quit hotkey exits' $p.HasExited
    Check 'release' 'log stays under 1.5 MB' ((Get-Item $prodLog).Length -lt 1500000) ((Get-Item $prodLog).Length)
    $cfgOk = Test-Path $cfgPath
    Check 'release' 'first run wrote a settings file' $cfgOk
    if ($cfgOk) {
      $j = Get-Content $cfgPath -Raw | ConvertFrom-Json
      Check 'release' 'defaults: click_through on, opacity 100, theme midnight' ($j.click_through -eq $true -and $j.opacity -eq 100 -and $j.theme -eq 'midnight')
      Check 'release' 'config holds no secrets/personal text beyond geometry' (-not ((Get-Content $cfgPath -Raw) -match [regex]::Escape($env:USERNAME)))
    }
    # Opacity persists across a restart.
    $p = Start-Rel
    $h = [GT]::Find([uint32]$p.Id)
    for ($i = 0; $i -lt 4; $i++) { [GT]::Hotkey(0xDB); Start-Sleep -Milliseconds 400 }
    Check 'release' 'normal build: four presses give 60%' ([GT]::Alpha($h) -eq 153) ([GT]::Alpha($h))
    Quit-Rel $p
    Check 'release' 'opacity saved to config.json' ((Get-Content $cfgPath -Raw | ConvertFrom-Json).opacity -eq 60)
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    Check 'release' 'restart comes up at the saved 60%' ([GT]::Alpha($h) -eq 153) ([GT]::Alpha($h))
    # Click-through persists? Toggle and restart.
    Quit-Rel $p
  } finally { Restore-Config }
  if ($script:hadCfg) { Check 'release' 'your config.json was restored byte-for-byte' ((Get-FileHash $cfgPath).Hash -eq $script:cfgHash) } else { Check 'release' 'test config.json removed (none existed before)' (-not (Test-Path $cfgPath)) }
}

function Suite-Config {
  Write-Host "=== config: bad files, bad values, hotkey conflicts (normal build) ==="
  Stop-HUDshell
  Backup-Config
  try {
    function Fresh { Restore-Config; New-Item -ItemType Directory -Force $cfgDir | Out-Null; Remove-Item -LiteralPath $prodLog -ErrorAction SilentlyContinue }
    # 1. Invalid JSON is kept as .bad-1 and defaults are used.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{ this is not json')
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    Check 'config' 'invalid JSON: app still starts' (-not $p.HasExited)
    Check 'config' 'invalid JSON: bad file kept as config.json.bad-1' (Test-Path (Join-Path $cfgDir 'config.json.bad-1'))
    Check 'config' 'invalid JSON: defaults in use (opaque)' ([GT]::Alpha($h) -eq 255)
    Check 'config' 'invalid JSON: user is told (notice logged)' (@(Read-Lines $prodLog | Where-Object { $_ -match 'notice: Your settings file was invalid' }).Count -ge 1)
    Quit-Rel $p
    # 2. Out-of-range / wrong values are clamped or defaulted, never crash.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"opacity": 3, "font_size": 999, "theme": "nope", "hotkeys": {"quit": "Q"}, "click_through": true}')
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    Check 'config' 'opacity 3 is raised to the 20% floor' ([GT]::Alpha($h) -eq 51) ([GT]::Alpha($h))
    Check 'config' 'bad hotkey "Q" is replaced by the default and reported' (@(Read-Lines $prodLog | Where-Object { $_ -match "Hotkey 'Q' for quit is not valid" }).Count -ge 1)
    [GT]::Hotkey(0x51); $null = $p.WaitForExit(8000)
    Check 'config' 'default quit hotkey works after the fallback' $p.HasExited
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    # 3. Opacity 5000 clamps to 100.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"opacity": 5000}')
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    Check 'config' 'opacity 5000 is clamped to 100%' ([GT]::Alpha($h) -eq 255) ([GT]::Alpha($h))
    Quit-Rel $p
    # 4. Hotkey conflict: another program already owns one of ours.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"hotkeys": {"show_hide": "Ctrl+Alt+F8"}}')
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class HK { [DllImport("user32.dll")] public static extern bool RegisterHotKey(IntPtr h, int id, uint mod, uint vk); [DllImport("user32.dll")] public static extern bool UnregisterHotKey(IntPtr h, int id); }' -ErrorAction SilentlyContinue
    $blocked = [HK]::RegisterHotKey([IntPtr]::Zero, 77, 0x3, 0x77)
    $p = Start-Rel
    Check 'config' 'hotkey conflict: app still starts' (-not $p.HasExited)
    Check 'config' 'hotkey conflict is reported to the user' (@(Read-Lines $prodLog | Where-Object { $_ -match 'notice: Hotkey .* is already used by another program' }).Count -ge 1) "blocked=$blocked"
    if ($blocked) { [void][HK]::UnregisterHotKey([IntPtr]::Zero, 77) }
    Quit-Rel $p
    # 5. Unknown keys ignored, missing keys default.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"somethingNew": 1, "theme": "matrix"}')
    $p = Start-Rel
    Check 'config' 'unknown keys ignored; valid partial config accepted' ((-not $p.HasExited) -and -not (Test-Path (Join-Path $cfgDir 'config.json.bad-1')))
    Quit-Rel $p
    # 6. A file saved with a UTF-8 byte-order mark (older Notepad, PowerShell 5.1 Set-Content) is accepted.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"opacity": 40}', (New-Object System.Text.UTF8Encoding($true)))
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    Check 'config' 'UTF-8 BOM config is accepted (opacity 40 applied)' (([GT]::Alpha($h) -eq 102) -and -not (Test-Path (Join-Path $cfgDir 'config.json.bad-1'))) ([GT]::Alpha($h))
    Quit-Rel $p
    # 7. Ghost opacity is clamped, and the dimmed value is never written to the settings file.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"opacity": 80, "ghost_opacity": 5}')
    $p = Start-Rel; $h = [GT]::Find([uint32]$p.Id)
    [GT]::Hotkey(0x58); Start-Sleep -Milliseconds 600
    Check 'config' 'ghost_opacity 5 is raised to the 20% floor' ([GT]::Alpha($h) -eq 51) ([GT]::Alpha($h))
    [GT]::Hotkey(0x58); Start-Sleep -Milliseconds 600
    Check 'config' 'leaving ghost restores the configured 80%' ([GT]::Alpha($h) -eq 204) ([GT]::Alpha($h))
    [GT]::Hotkey(0x58); Start-Sleep -Milliseconds 600
    [GT]::Hotkey(0x51); $null = $p.WaitForExit(8000)
    Check 'config' 'quitting while ghosted does not save the dimmed opacity' ((Get-Content -LiteralPath $cfgPath -Raw) -match '"opacity":\s*80\b')
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    # 8. Unknown values for the new options fall back to the safe defaults.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"right_click": "banana", "osc52": "read", "copy_on_select": true}')
    $p = Start-Rel
    Check 'config' 'bad right_click / osc52 values: app starts and no .bad file is made' ((-not $p.HasExited) -and -not (Test-Path (Join-Path $cfgDir 'config.json.bad-1')))
    Quit-Rel $p
    $saved = Get-Content -LiteralPath $cfgPath -Raw
    Check 'config' 'bad values fell back to right_click=off, osc52=off; valid copy_on_select kept' (($saved -match '"right_click":\s*"off"') -and ($saved -match '"osc52":\s*"off"') -and ($saved -match '"copy_on_select":\s*true'))
  } finally { Restore-Config }
  if ($script:hadCfg) { Check 'config' 'your config.json was restored byte-for-byte' ((Get-FileHash $cfgPath).Hash -eq $script:cfgHash) } else { Check 'config' 'test config files removed (none existed before)' (@(Get-ChildItem $cfgDir -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'config.json*' }).Count -eq 0) }
}

# View position [top, bottom] from the dump header that follows "DUMP label=<label>".
function Get-View($run, [string]$label) {
  $i = -1
  for ($k = 0; $k -lt $run.Count; $k++) { if ($run[$k] -match ("DUMP label=" + [regex]::Escape($label) + '(\s|$)')) { $i = $k; break } }
  if ($i -lt 0) { return $null }
  for ($k = $i + 1; $k -lt $run.Count; $k++) { if ($run[$k] -match 'terminal dump:.* view=\[(\d+),(\d+)\]') { return @([int]$Matches[1], [int]$Matches[2]) } }
  return $null
}
function Get-DumpField($run, [string]$label, [string]$field) {
  $i = -1
  for ($k = 0; $k -lt $run.Count; $k++) { if ($run[$k] -match ("DUMP label=" + [regex]::Escape($label) + '(\s|$)')) { $i = $k; break } }
  if ($i -lt 0) { return $null }
  for ($k = $i + 1; $k -lt $run.Count; $k++) { if ($run[$k] -match ('terminal dump:.* ' + $field + '=(\S+)')) { return $Matches[1] } }
  return $null
}
# STATE lines logged by the ghost scenario, keyed by label.
function Get-States($run) {
  $h = @{}
  foreach ($l in $run) { if ($l -match 'STATE label=(\S+) (.*)$') { $o = @{}; foreach ($kv in ($Matches[2] -split ' ')) { $p = $kv -split '=', 2; if ($p.Count -eq 2) { $o[$p[0]] = $p[1] } }; $h[$Matches[1]] = $o } }
  return $h
}

function Suite-Scrollback {
  Write-Host "=== scrollback: keyboard scrolling of the history (no OS input) ==="
  $run = Run-Scenario @('--term', '--scrolltest', '--delay', '2') 60000
  $b = Get-View $run 'sb-bottom'
  Check 'scrollback' '200 lines produced scrollback and the view starts at the bottom' ($b -and $b[1] -gt 100 -and $b[0] -eq $b[1]) ("view " + ($b -join ','))
  $base = if ($b) { $b[1] } else { 0 }
  $v = Get-View $run 'sb-pageup';        Check 'scrollback' 'Shift+PageUp scrolls up by a page' ($v -and $v[0] -lt $base -and $v[0] -ge $base - 40) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-top';           Check 'scrollback' 'Ctrl+Shift+Home jumps to the top' ($v -and $v[0] -eq 0) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-line-down';     Check 'scrollback' 'Ctrl+Shift+Down moves one line' ($v -and $v[0] -eq 1) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-pagedown';      Check 'scrollback' 'Shift+PageDown scrolls down by a page' ($v -and $v[0] -gt 10 -and $v[0] -lt $base) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-end';           Check 'scrollback' 'Ctrl+Shift+End jumps back to the bottom' ($v -and $v[0] -eq $base) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-line-up';       Check 'scrollback' 'Ctrl+Shift+Up moves one line up' ($v -and $v[0] -eq $base - 1) ("view " + ($v -join ','))
  $v = Get-View $run 'sb-plain-pageup-does-not-scroll'; Check 'scrollback' 'a plain PageUp goes to the shell (typing returns the view to the bottom)' ($v -and $v[0] -eq $base) ("view " + ($v -join ','))
}

function Suite-Ghost {
  Write-Host "=== ghost: click-through + dim preset (host functions, no OS input) ==="
  $run = Run-Scenario @('--term', '--ghosttest', '--delay', '2') 60000
  $s = Get-States $run
  function St($k) { if ($s.ContainsKey($k)) { $s[$k] } else { @{} } }
  Check 'ghost' 'default ghost hotkey Ctrl+Alt+X registered' (@($run | Where-Object { $_ -match 'hotkey Ctrl\+Alt\+X \(ghost .*registered' }).Count -eq 1)
  $a = St 'start';      Check 'ghost' 'starts opaque, click-through on, not ghosted' ($a.pct -eq '100' -and $a.click_through -eq 'true' -and $a.ghost -eq 'false')
  $a = St 'ghost-on';   Check 'ghost' 'ghost dims to 40% and keeps click-through on' ($a.pct -eq '40' -and $a.click_through -eq 'true' -and $a.ghost -eq 'true') ("pct=" + $a.pct)
  Check 'ghost' 'ghost never changes the saved opacity' ($a.cfg_opacity -eq '100')
  $a = St 'ghost-off';  Check 'ghost' 'second press restores 100%' ($a.pct -eq '100' -and $a.ghost -eq 'false') ("pct=" + $a.pct)
  $a = St 'ghost-from-70'; Check 'ghost' 'from 70% it dims to 40% and the saved opacity stays 70' ($a.pct -eq '40' -and $a.cfg_opacity -eq '70')
  $a = St 'restored-70';   Check 'ghost' '... and restores exactly 70%' ($a.pct -eq '70')
  $a = St 'ghost-from-30-never-brightens'; Check 'ghost' 'when already dimmer than the ghost level (30%) it never brightens' ($a.pct -eq '30' -and $a.ghost -eq 'true') ("pct=" + $a.pct)
  $a = St 'restored-30';   Check 'ghost' '... and restores 30%' ($a.pct -eq '30')
  $a = St 'interactive';   Check 'ghost' 'precondition: click-through can be switched off' ($a.click_through -eq 'false')
  $a = St 'ghost-forces-click-through'; Check 'ghost' 'ghost forces click-through on even when it was off' ($a.click_through -eq 'true' -and $a.pct -eq '40')
  $a = St 'interactive-restored';       Check 'ghost' 'leaving ghost restores the previous (interactive) pointer mode and opacity' ($a.click_through -eq 'false' -and $a.pct -eq '100')
  $a = St 'ct-toggle-leaves-ghost';     Check 'ghost' 'toggling click-through while ghosted ends ghost and applies the toggle' ($a.ghost -eq 'false' -and $a.pct -eq '100' -and $a.click_through -eq 'false')
  $a = St 'opacity-change-leaves-ghost'; Check 'ghost' 'changing opacity while ghosted ends ghost, applies the new value and restores pointer mode' ($a.ghost -eq 'false' -and $a.pct -eq '60' -and $a.click_through -eq 'false')
}

function Suite-Attention {
  Write-Host "=== attention: bell / OSC 9 / OSC 777 cue while passive (no OS input) ==="
  $run = Run-Scenario @('--term', '--attention', '--delay', '2') 60000
  $att = @($run | Where-Object { $_ -match 'attention:' } | ForEach-Object { Strip $_ })
  Check 'attention' 'bell while passive pulses the window' ((($att -join '|') -match 'attention: bell -> border cue') -and (Get-DumpField $run 'cue-after-bell' 'cue') -eq 'true')
  Check 'attention' 'a second request within 3 s is rate-limited' (($att -join '|') -match 'attention: osc9 ignored \(rate limit\)')
  Check 'attention' 'the pulse ends by itself' ((Get-DumpField $run 'cue-faded' 'cue') -eq 'false')
  Check 'attention' 'OSC 9 notification pulses the window' ((($att -join '|') -match 'attention: osc9 -> border cue') -and (Get-DumpField $run 'cue-after-osc9' 'cue') -eq 'true')
  Check 'attention' 'OSC 9;4 progress and OSC 9;9 working-directory are not notifications' (((Get-DumpField $run 'no-cue-for-progress-and-cwd' 'cue') -eq 'false') -and @($att | Where-Object { $_ -match 'osc9' }).Count -eq 2) ("osc9 events " + @($att | Where-Object { $_ -match 'osc9' }).Count)
  Check 'attention' 'OSC 777;notify pulses the window' ((($att -join '|') -match 'attention: osc777 -> border cue') -and (Get-DumpField $run 'cue-after-osc777' 'cue') -eq 'true')
  Check 'attention' 'no cue while HUDshell holds the keyboard (simulated flag)' ((($att -join '|') -match 'while HUDshell has the keyboard: no cue') -and (Get-DumpField $run 'no-cue-while-focused' 'cue') -eq 'false')
  Check 'attention' 'while hidden a tray notification is requested instead' (($att -join '|') -match 'while hidden -> tray notification')
  Check 'attention' 'notification text never reaches the log' (@($run | Where-Object { $_ -match 'attention:|tray notification' -and $_ -match 'build finished|Body text|rate-limited$' }).Count -eq 0)
  $run2 = Run-Scenario @('--term', '--attention', '--no-cue', '--delay', '2') 60000
  Check 'attention' 'attention_cue=false suppresses the pulse' ((@($run2 | Where-Object { $_ -match 'cue disabled in config' }).Count -ge 1) -and (Get-DumpField $run2 'cue-after-bell' 'cue') -eq 'false')
}

function Suite-Policy {
  Write-Host "=== policy: OSC 52, OSC 8, copy-on-select, right-click (DevTools mouse events; temporarily uses the clipboard) ==="
  $had = $null
  try { $had = Get-Clipboard -Raw -ErrorAction Stop } catch { $had = $null }
  try {
    $sentinel = 'echo RCLICK-PASTE-OK'
    function ClipNow { try { (Get-Clipboard -Raw) } catch { '' } }
    # a) defaults: everything opt-in is off
    Set-Clipboard -Value $sentinel
    $run = Run-Scenario @('--term', '--policytest', '--delay', '2') 60000
    $applied = @($run | Where-Object { $_ -match 'terminal applied settings' }) -join ' '
    Check 'policy' 'defaults reported by the page: copyOnSelect off, rightClick off, osc52 off, link handler installed' ($applied -match '"copyOnSelect":false' -and $applied -match '"rightClick":"off"' -and $applied -match '"osc52":"off"' -and $applied -match '"linkHandler":true') ''
    Check 'policy' 'OSC 52 from a program is refused by default (clipboard untouched)' (((ClipNow).Trim() -eq $sentinel) -and @($run | Where-Object { $_ -match 'osc52-refused' }).Count -ge 1) ''
    Check 'policy' 'no OSC 52 write was logged' (@($run | Where-Object { $_ -match 'OSC 52 clipboard write' }).Count -eq 0)
    Check 'policy' 'OSC 8 link text is drawn (but never opened)' ((Dump-Has $run 'policy-screen' 'a link') -and @($run | Where-Object { $_ -match 'new window blocked|navigation blocked' }).Count -eq 0)
    Check 'policy' 'right-click with right_click=off does not paste' ((Dump-Count $run 'after-right-click-no-selection' 'RCLICK-PASTE-OK') -eq 0)
    Check 'policy' 'copy on select is off by default (a drag leaves the clipboard alone)' ((ClipNow).Trim() -eq $sentinel)
    # b) OSC 52 write allowed; reads never answered
    Set-Clipboard -Value $sentinel
    $run = Run-Scenario @('--term', '--policytest', '--osc52', 'write', '--log-input', '--delay', '2') 60000
    Check 'policy' 'osc52=write: a program can set the clipboard (hello)' (((ClipNow).Trim() -eq 'hello') -and @($run | Where-Object { $_ -match 'OSC 52 clipboard write from a program' }).Count -eq 1) ("clipboard '" + (ClipNow).Trim() + "'")
    Check 'policy' 'osc52=write: the read query (OSC 52;c;?) is not answered' (@($run | Where-Object { $_ -match 'PTY-IN' -and $_ -match ']52;' }).Count -eq 0)
    # c) copy on select
    Set-Clipboard -Value $sentinel
    $run = Run-Scenario @('--term', '--policytest', '--copy-on-select', '--delay', '2') 60000
    $now = (ClipNow).Trim()
    Check 'policy' 'copy-on-select: releasing a drag selection copies it' ($now.Length -gt 0 -and $now -ne $sentinel -and $now -ne 'hello') ("clipboard " + $now.Length + " chars")
    # d) right-click paste / copy
    Set-Clipboard -Value $sentinel
    $run = Run-Scenario @('--term', '--policytest', '--right-click', 'paste', '--delay', '2') 60000
    Check 'policy' 'right_click=paste: right-click with no selection pastes the clipboard' ((Dump-Count $run 'after-right-click-no-selection' 'RCLICK-PASTE-OK') -ge 1) ("occurrences " + (Dump-Count $run 'after-right-click-no-selection' 'RCLICK-PASTE-OK'))
    $took = @($run | Where-Object { $_ -match 'right-click paste: .*clipboard read took (\d+) ms' } | ForEach-Object { [int]$Matches[1] })
    Check 'policy' 'right_click=paste: the host clipboard read was quick (no prompt or stall in the way)' ($took.Count -ge 1 -and $took[0] -lt 1500) ("took " + ($took -join ',') + " ms")
    $now = (ClipNow).Trim()
    Check 'policy' 'right_click=paste: right-click with a selection copies it instead' ($now.Length -gt 0 -and $now -ne $sentinel) ("clipboard " + $now.Length + " chars")
  } finally {
    if ($null -ne $had) { Set-Clipboard -Value $had } else { Set-Clipboard -Value ' ' }
  }
}

function Tree-Memory([int]$rootPid) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
  $ids = New-Object System.Collections.Generic.HashSet[int]; [void]$ids.Add($rootPid)
  $grew = $true
  while ($grew) { $grew = $false; foreach ($x in $all) { if ($ids.Contains([int]$x.ParentProcessId) -and -not $ids.Contains([int]$x.ProcessId)) { [void]$ids.Add([int]$x.ProcessId); $grew = $true } } }
  $host_ = 0; $total = 0
  foreach ($id in $ids) { $pr = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($pr) { $total += $pr.PrivateMemorySize64; if ($id -eq $rootPid) { $host_ = $pr.PrivateMemorySize64 } } }
  return @($host_, $total)
}
function Suite-Soak {
  Write-Host "=== soak ($SoakSeconds s of continuous output, memory sampled every 15 s) ==="
  Stop-HUDshell
  $n = LineCount $log
  $p = Start-Process $dbg -ArgumentList @('--term', '--soak', "$SoakSeconds") -PassThru
  $samples = @(); $t0 = Get-Date
  while (-not $p.HasExited -and ((Get-Date) - $t0).TotalSeconds -lt ($SoakSeconds + 60)) {
    Start-Sleep 15
    if (-not $p.HasExited) { $m = Tree-Memory $p.Id; $samples += [pscustomobject]@{ T = [int]((Get-Date) - $t0).TotalSeconds; Host = $m[0]; Total = $m[1] } }
  }
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force; Warn 'soak' 'did not finish by itself' }
  Check 'soak' 'ran to completion with samples' ($samples.Count -ge 5) "$($samples.Count) samples"
  if ($samples.Count -ge 5) {
    $mid = $samples | Where-Object { $_.T -ge 60 } | Select-Object -First 1
    $last = $samples[-1]
    Write-Host ("  host private MB: first {0:N1}, last {1:N1}; whole tree MB: at {2}s {3:N0}, last {4:N0}" -f ($samples[0].Host / 1MB), ($last.Host / 1MB), $mid.T, ($mid.Total / 1MB), ($last.Total / 1MB))
    Check 'soak' 'host process stays small (under 40 MB private)' (($samples | Measure-Object Host -Maximum).Maximum -lt 40MB)
    Check 'soak' 'whole process tree grows less than 25% after the first minute' ($last.Total -lt ($mid.Total * 1.25)) ("{0:N0} -> {1:N0} MB" -f ($mid.Total / 1MB), ($last.Total / 1MB))
  }
  $run = Cut $log $n
  Check 'soak' 'no pty/WebView failure lines logged' (@($run | Where-Object { $_ -match 'FAILED|panicked' }).Count -eq 0)
}

function Suite-Real {
  Write-Host "=== real prompts in the sandbox (Claude Code and Codex; sends real prompts) ==="
  if (-not $Sandbox) { Warn 'real' 'skipped: pass -Sandbox <trusted folder>'; return }
  GTSave
  try { $run = Run-Scenario @('--term', '--autotest', '--real', '--no-grab', '--log-input', '--delay', '3') 600000 } finally { [GT]::RestoreCursor() }
  foreach ($l in $run | Where-Object { $_ -match 'ABORT|abort-unless-prompt' }) { Write-Host "  $(Strip $l)" }
  Check 'real' 'run finished and reached the end' (@($run | Where-Object { $_ -match 'AUTOTEST: finished' }).Count -ge 1)
  Check 'real' 'never aborted because a prompt was missing' (@($run | Where-Object { $_ -cmatch 'ABORT:' }).Count -eq 0)
  Write-Host '  (screen dumps for each step are in the log; review them for rendering)'
}

foreach ($s in $want) {
  switch ($s) {
    'static' { Suite-Static }
    'keys' { Suite-Keys }
    'width' { Suite-Width }
    'hideshow' { Suite-HideShow }
    'clickfocus' { Suite-ClickFocus }
    'clipboard' { Suite-Clipboard }
    'autotest' { Suite-Autotest }
    'ui' { Suite-Ui }
    'scrollback' { Suite-Scrollback }
    'ghost' { Suite-Ghost }
    'attention' { Suite-Attention }
    'policy' { Suite-Policy }
    'release' { Suite-Release }
    'config' { Suite-Config }
    'soak' { Suite-Soak }
    'real' { Suite-Real }
    default { Write-Host "unknown suite $s" }
  }
}
Stop-HUDshell
Write-Host ''
Write-Host '================ RESULTS ================'
$script:R | Format-Table Suite, Status, Check, Detail -AutoSize -Wrap | Out-String -Width 220 | Write-Host
$pass = @($script:R | Where-Object Status -eq 'PASS').Count
$fail = @($script:R | Where-Object Status -eq 'FAIL').Count
$warn = @($script:R | Where-Object Status -eq 'WARN').Count
Write-Host "PASS $pass   FAIL $fail   WARN $warn"
Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue
exit $fail
