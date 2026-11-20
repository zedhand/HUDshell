<#
HUDshell full test runner (Windows PowerShell 5.1 or PowerShell 7).

  scripts\test-all.ps1 [-Suite static,keys,width,hideshow,clickfocus,clipboard,autotest,ui,scrollback,ghost,attention,policy,place,qol,sessions,shells,layout,input,cost,release,config,soak,real | all]
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
$want = if ($Suite -contains 'all') { @('static', 'keys', 'width', 'hideshow', 'clickfocus', 'clipboard', 'autotest', 'ui', 'scrollback', 'ghost', 'attention', 'policy', 'place', 'qol', 'sessions', 'shells', 'layout', 'input', 'cost', 'release', 'config', 'soak') + $(if ($Sandbox) { 'real' } else { @() }) } else { $Suite }

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
  public static void HotkeyShift(byte vk) { keybd_event(0x11,0,0,UIntPtr.Zero); keybd_event(0x12,0,0,UIntPtr.Zero); keybd_event(0x10,0,0,UIntPtr.Zero); keybd_event(vk,0,0,UIntPtr.Zero); keybd_event(vk,0,2,UIntPtr.Zero); keybd_event(0x10,0,2,UIntPtr.Zero); keybd_event(0x12,0,2,UIntPtr.Zero); keybd_event(0x11,0,2,UIntPtr.Zero); }
  public static void Move(int x, int y) { SetCursorPos(x, y); mouse_event(0x1,0,0,0,UIntPtr.Zero); }
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
    $uS = Get-Ui $run "$prog-start"; $uE = Get-Ui $run "$prog-exited2"
    Check 'autotest' "$prog (the real program): its tab is labelled '$prog' while it runs" ($uS -and $uS.progs -ceq $prog) ("progs=[" + $uS.progs + "] label=" + $uS.labels[0])
    Check 'autotest' "$prog (the real program): the label is gone once it has exited" ($uE -and $uE.progs -ceq '') ("progs=[" + $uE.progs + "]")
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
    $o0 = [GT]::Origin($h)
    [GT]::Hotkey(0xBE); Start-Sleep -Milliseconds 600; $o1 = [GT]::Origin($h)
    [GT]::Hotkey(0xBE); Start-Sleep -Milliseconds 600; $o2 = [GT]::Origin($h)
    [GT]::Hotkey(0xBC); Start-Sleep -Milliseconds 600; $o3 = [GT]::Origin($h)
    Check 'ui' 'Ctrl+Alt+. moves the window to a corner, again to the next one' (($o1.X -ne $o0.X -or $o1.Y -ne $o0.Y) -and ($o2.X -ne $o1.X -or $o2.Y -ne $o1.Y)) ("{0},{1} -> {2},{3} -> {4},{5}" -f $o0.X, $o0.Y, $o1.X, $o1.Y, $o2.X, $o2.Y)
    Check 'ui' 'Ctrl+Alt+, goes back to the previous corner, and the window never took focus' (($o3.X -eq $o1.X -and $o3.Y -eq $o1.Y) -and -not [GT]::IsFg($h)) ("{0},{1}" -f $o3.X, $o3.Y)
    [GT]::Hotkey(0xBB); Start-Sleep -Milliseconds 500; [GT]::Hotkey(0xBD); Start-Sleep -Milliseconds 500
    Check 'ui' 'Ctrl+Alt+= and Ctrl+Alt+- change the font size' (@(Cut $log $n | Where-Object { $_ -match 'font size -> (15|14)' }).Count -ge 2)
    $nn = LineCount $log
    [GT]::Hotkey(0x4E); Start-Sleep -Milliseconds 2500
    [GT]::Hotkey(0x21); Start-Sleep -Milliseconds 700
    [GT]::Hotkey(0x22); Start-Sleep -Milliseconds 700
    [GT]::Hotkey(0x31); Start-Sleep -Milliseconds 700
    [GT]::Hotkey(0x57); Start-Sleep -Milliseconds 500
    [GT]::Hotkey(0x57); Start-Sleep -Milliseconds 1500
    $sl = @(Cut $log $nn)
    Check 'ui' 'Ctrl+Alt+N opens a second shell' (@($sl | Where-Object { $_ -match 'new shell 2 from profile' }).Count -eq 1)
    Check 'ui' 'Ctrl+Alt+PageUp / PageDown / 1 switch between shells' ((@($sl | Where-Object { $_ -match 'shell 1 is now active \(step -1\)' }).Count -eq 1) -and (@($sl | Where-Object { $_ -match 'shell 2 is now active \(step 1\)' }).Count -eq 1) -and (@($sl | Where-Object { $_ -match 'jumped to shell 1' }).Count -eq 1))
    Check 'ui' 'Ctrl+Alt+W needs a second press: the first asks, the second closes' ((@($sl | Where-Object { $_ -match 'close shell 1: press again to confirm' }).Count -eq 1) -and (@($sl | Where-Object { $_ -match 'shell 1 closed' }).Count -eq 1) -and -not [GT]::IsFg($h))
    $nl = LineCount $log
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 2500
    [GT]::Hotkey(0x4F); Start-Sleep -Milliseconds 600
    [GT]::Hotkey(0x5A); Start-Sleep -Milliseconds 600
    [GT]::Hotkey(0x5A); Start-Sleep -Milliseconds 600
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 600
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 3000
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 800
    $ll = @(Cut $log $nl)
    Check 'ui' "Ctrl+Alt+' steps through the layouts (side by side, stacked, grid, single)" (((@($ll | Where-Object { $_ -match 'layout -> (side-by-side|stacked|grid|single) ' }) | ForEach-Object { if ($_ -match 'layout -> (\S+)') { $Matches[1] } }) -join ',') -eq 'side-by-side,stacked,grid,single')
    Check 'ui' 'Ctrl+Alt+O moves the keyboard to the next pane' (@($ll | Where-Object { $_ -match 'pane focus -> pane 2' }).Count -ge 1)
    Check 'ui' 'Ctrl+Alt+Z zooms and unzooms, and the window never took focus' ((@($ll | Where-Object { $_ -match 'zoom on' }).Count -eq 1) -and (@($ll | Where-Object { $_ -match 'zoom off' }).Count -eq 1) -and -not [GT]::IsFg($h))
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
    # Second launch exits quickly (single instance) and brings a hidden first window forward.
    [GT]::Hotkey(0x56); Start-Sleep -Milliseconds 600
    $p2 = Start-Process (Join-Path $copy 'hudshell.exe') -PassThru
    $null = $p2.WaitForExit(6000)
    Check 'release' 'a second launch exits (single instance)' $p2.HasExited
    if (-not $p2.HasExited) { Stop-Process -Id $p2.Id -Force }
    Start-Sleep -Milliseconds 800
    Check 'release' 'the second launch brought the hidden first window forward' (@(Read-Lines $prodLog | Where-Object { $_ -match 'second launch: brought the running window forward \(was hidden: true\)' }).Count -ge 1)
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
    # 9. Layout settings: bad values fall back; the layout is saved only when remember_layout is on.
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"layout": "weird", "layout_fill": "x", "pane_shells": "x", "tab_program": "x", "split": [1, 99]}')
    $p = Start-Rel
    Check 'config' 'bad layout values: app starts and no .bad file is made' ((-not $p.HasExited) -and -not (Test-Path (Join-Path $cfgDir 'config.json.bad-1')))
    Quit-Rel $p
    $saved = Get-Content -LiteralPath $cfgPath -Raw
    Check 'config' 'bad layout values fell back to single / new / shared / known and the split was clamped to 20..80' (($saved -match '"layout":\s*"single"') -and ($saved -match '"layout_fill":\s*"new"') -and ($saved -match '"pane_shells":\s*"shared"') -and ($saved -match '"tab_program":\s*"known"') -and ($saved -match '"split":\s*\[\s*20,\s*80\s*\]'))
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"layout": "single"}')
    $p = Start-Rel
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 3000
    Quit-Rel $p
    Check 'config' 'remember_layout off (default): a layout change is not saved' ((Get-Content -LiteralPath $cfgPath -Raw) -match '"layout":\s*"single"')
    Fresh
    [IO.File]::WriteAllText($cfgPath, '{"layout": "single", "remember_layout": true}')
    $p = Start-Rel
    [GT]::Hotkey(0xDE); Start-Sleep -Milliseconds 3000
    Quit-Rel $p
    Check 'config' 'remember_layout on: the chosen layout is saved' ((Get-Content -LiteralPath $cfgPath -Raw) -match '"layout":\s*"side-by-side"')
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

function Suite-Place {
  Write-Host "=== place: corner docking, size presets, font size, magnetic snap (host functions, no OS input) ==="
  $run = Run-Scenario @('--term', '--placetest', '--delay', '2') 60000
  $s = Get-States $run
  function Rc($k) { $v = (($s[$k].rect) -split ','); $w = (($s[$k].work) -split ','); @{ l = [int]$v[0]; t = [int]$v[1]; r = [int]$v[2]; b = [int]$v[3]; wl = [int]$w[0]; wt = [int]$w[1]; wr = [int]$w[2]; wb = [int]$w[3]; dpi = [int]$s[$k].dpi } }
  foreach ($k in 'c1', 'c2', 'c3', 'c4', 'back', 'size1', 'size2', 'size3') { if (-not $s.ContainsKey($k)) { Check 'place' "state $k logged" $false; return } }
  $m = [int][math]::Round(8 * (Rc 'c1').dpi / 96.0)
  $a = Rc 'c1'; Check 'place' 'first press docks into the top-left corner with the margin' (($a.l - $a.wl) -eq $m -and ($a.t - $a.wt) -eq $m) "margin $m"
  $a = Rc 'c2'; Check 'place' 'next: top-right' (($a.wr - $a.r) -eq $m -and ($a.t - $a.wt) -eq $m)
  $a = Rc 'c3'; Check 'place' 'next: bottom-right' (($a.wr - $a.r) -eq $m -and ($a.wb - $a.b) -eq $m)
  $a = Rc 'c4'; Check 'place' 'next: bottom-left' (($a.l - $a.wl) -eq $m -and ($a.wb - $a.b) -eq $m)
  $a = Rc 'back'; Check 'place' 'previous goes counter-clockwise (bottom-right)' (($a.wr - $a.r) -eq $m -and ($a.wb - $a.b) -eq $m)
  $c1 = Rc 'c1'; $c3 = Rc 'c3'
  Check 'place' 'docking keeps the window size' ((($c1.r - $c1.l) -eq ($c3.r - $c3.l)) -and (($c1.b - $c1.t) -eq ($c3.b - $c3.t)))
  $d = [double](Rc 'size1').dpi / 96
  $a = Rc 'size1'; Check 'place' 'size hotkey: 900x520 -> 1280x720 (scaled), bottom-right anchored' ((($a.r - $a.l) -eq [int](1280 * $d)) -and (($a.b - $a.t) -eq [int](720 * $d)) -and ($a.wr - $a.r) -eq $m -and ($a.wb - $a.b) -eq $m) ("{0}x{1}" -f ($a.r - $a.l), ($a.b - $a.t))
  $a = Rc 'size2'; Check 'place' 'size hotkey wraps to the smallest preset (640x360)' ((($a.r - $a.l) -eq [int](640 * $d)) -and (($a.b - $a.t) -eq [int](360 * $d)) -and ($a.wr - $a.r) -eq $m) ("{0}x{1}" -f ($a.r - $a.l), ($a.b - $a.t))
  $a = Rc 'size3'; Check 'place' 'and on to 900x520 again' ((($a.r - $a.l) -eq [int](900 * $d)) -and (($a.b - $a.t) -eq [int](520 * $d)))
  Check 'place' 'font hotkeys: 14 -> 16 -> 15' (($s['font-up'].font -eq '16') -and ($s['font-down'].font -eq '15')) ("up=" + $s['font-up'].font + " down=" + $s['font-down'].font)
  $fsB = Get-DumpField $run 'font-base' 'fs'; $fsU = Get-DumpField $run 'font-up' 'fs'; $fsD = Get-DumpField $run 'font-down' 'fs'
  Check 'place' 'the page applied the font size (14 -> 16 -> 15)' (($fsB -eq '14') -and ($fsU -eq '16') -and ($fsD -eq '15')) "$fsB,$fsU,$fsD"
  $colsOf = { param($label) $h = Get-DumpField $run $label 'buffer'; $null; $i = -1; for ($k = 0; $k -lt $run.Count; $k++) { if ($run[$k] -match ("DUMP label=" + $label + '(\s|$)')) { $i = $k; break } }; for ($k = $i + 1; $k -lt $run.Count; $k++) { if ($run[$k] -match 'terminal dump: (\d+)x(\d+)') { return [int]$Matches[1] } } }
  $cb = & $colsOf 'font-base'; $cu = & $colsOf 'font-up'
  Check 'place' 'a bigger font gives fewer columns (the shell is told the new size)' ($cb -gt 0 -and $cu -gt 0 -and $cu -lt $cb) "$cb -> $cu columns"
  $mv = @($run | Where-Object { $_ -match 'MOVING in=' } | ForEach-Object { Strip $_ })
  Check 'place' 'dragging near the top-left corner snaps flush to the margin' ($mv.Count -ge 1 -and $mv[0] -match ("in=5,7 out=$m,$m,\d+,\d+ handled=1")) ($mv[0])
  Check 'place' 'dragging far from every edge is left alone' ($mv.Count -ge 2 -and $mv[1] -match 'in=400,300 out=400,300,\d+,\d+ handled=0') ($mv[1])
}

function Suite-Qol {
  Write-Host "=== qol: idle dim, active opacity, start options, Start with Windows (no OS input) ==="
  # 1. idle dim + active opacity + ghost on top
  $run = Run-Scenario @('--term', '--idletest', '--opacity', '80', '--active-opacity', '100', '--idle-dim', '3', '--idle-opacity', '60', '--delay', '2') 60000
  $s = Get-States $run
  function Pct($k) { if ($s.ContainsKey($k)) { $s[$k].pct } else { '?' } }
  Check 'qol' 'after 3 s of shell silence the window dims to the idle opacity (60%, from 80%)' ((Pct 'idle') -eq '60') ("pct=" + (Pct 'idle'))
  Check 'qol' 'holding the keyboard uses the active opacity (100%) and ignores idle dimming' ((Pct 'active') -eq '100') ("pct=" + (Pct 'active'))
  Check 'qol' 'releasing it returns to the dimmed state while the shell is still silent' ((Pct 'passive-again') -eq '60') ("pct=" + (Pct 'passive-again'))
  Check 'qol' 'new shell output brightens it back to 80% at once' ((Pct 'woke') -eq '80') ("pct=" + (Pct 'woke'))
  Check 'qol' 'it dims again after another quiet spell' ((Pct 'idle-again') -eq '60') ("pct=" + (Pct 'idle-again'))
  Check 'qol' 'ghost goes below idle (40%) and, when left, idle dimming resumes (60%)' (((Pct 'ghost-over-idle') -eq '40') -and ((Pct 'idle-after-ghost') -eq '60')) ("ghost=" + (Pct 'ghost-over-idle') + " after=" + (Pct 'idle-after-ghost'))
  Check 'qol' 'the dimmed values never overwrite the saved opacity (80)' ($s['idle'].cfg_opacity -eq '80' -and $s['ghost-over-idle'].cfg_opacity -eq '80')
  # 2. start options
  $run = Run-Scenario @('--term', '--starttest', '--start-hidden', '--start-ghost', '--delay', '2') 60000
  $s = Get-States $run
  Check 'qol' 'start_hidden: the window starts hidden' (($s['started'].visible -eq 'false') -and @($run | Where-Object { $_ -match 'start_hidden' }).Count -ge 1)
  Check 'qol' 'start_ghost: it starts in ghost mode, dimmed and click-through' (($s['started'].ghost -eq 'true') -and ($s['started'].pct -eq '40') -and ($s['started'].click_through -eq 'true'))
  Check 'qol' 'the show/hide toggle reveals it' ($s['shown'].visible -eq 'true')
  Check 'qol' 'leaving ghost restores full opacity' (($s['ghost-off'].ghost -eq 'false') -and ($s['ghost-off'].pct -eq '100'))
  # 3. Start with Windows (HKCU Run), under the test-only value name
  $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
  $realBefore = (Get-ItemProperty -Path $runKey -Name 'HUDshell' -ErrorAction SilentlyContinue).HUDshell
  try {
    $run = Run-Scenario @('--term', '--autostarttest', '--delay', '2') 60000
    $en = @($run | Where-Object { $_ -match 'AUTOSTART name=' } | ForEach-Object { if ($_ -match 'enabled=(\w+)') { $Matches[1] } })
    Check 'qol' 'autostart off / on / on / off / off / on gives false,true,true,false,false,true' (($en -join ',') -eq 'false,true,true,false,false,true') ($en -join ',')
    Check 'qol' 'it uses a test-only registry value name, never the real one' (@($run | Where-Object { $_ -match 'AUTOSTART name=HUDshell-test ' }).Count -ge 6)
    $val = (Get-ItemProperty -Path $runKey -Name 'HUDshell-test' -ErrorAction SilentlyContinue).'HUDshell-test'
    Check 'qol' 'the stored value is the quoted path of the running executable' ($val -eq ('"' + $dbg + '"')) $(if ($val) { 'value present' } else { 'missing' })
  } finally {
    Remove-ItemProperty -Path $runKey -Name 'HUDshell-test' -ErrorAction SilentlyContinue
  }
  Check 'qol' 'the test value was removed afterwards' ($null -eq (Get-ItemProperty -Path $runKey -Name 'HUDshell-test' -ErrorAction SilentlyContinue))
  $realAfter = (Get-ItemProperty -Path $runKey -Name 'HUDshell' -ErrorAction SilentlyContinue).HUDshell
  Check 'qol' 'your real Start with Windows entry (if any) was not touched' ($realBefore -eq $realAfter)
}

function Suite-Sessions {
  Write-Host "=== sessions: several shells in one window (no OS input) ==="
  $run = Run-Scenario @('--term', '--sessiontest', '--delay', '2') 90000
  Check 'sessions' 'a second session is created and its own shell starts' (@($run | Where-Object { $_ -match 'pty: started .* for session 2 ' }).Count -ge 1)
  Check 'sessions' 'session 1 shows its own output, not session 2''s' ((Dump-Has $run 's1' 'SESSION-ONE-ALIVE') -and -not (Dump-Has $run 's1' 'SESSION-TWO-ALIVE'))
  Check 'sessions' 'session 2 shows its own output, not session 1''s' ((Dump-Has $run 's2' 'SESSION-TWO-ALIVE') -and -not (Dump-Has $run 's2' 'SESSION-ONE-ALIVE'))
  Check 'sessions' 'the host writes to the session that is on screen (2), not to 1' ((Dump-Has $run 's2b' 'ACTIVE-WRITE-WENT-TO-TWO') -and -not (Dump-Has $run 's1b' 'ACTIVE-WRITE-WENT-TO-TWO'))
  Check 'sessions' 'a shell that exits is reported in its own tab only' ((Dump-Has $run 's2-exited' 'session ended') -and -not (Dump-Has $run 's1b' 'session ended'))
  Check 'sessions' 'Enter in the finished tab starts a fresh shell in it (restart), and the old end-of-session text is gone' ((Dump-Has $run 's2-restarted' 'AFTER-RESTART') -and -not (Dump-Has $run 's2-restarted' 'session ended') -and (@($run | Where-Object { $_ -match 'pty: started .* for session 2 ' }).Count -ge 2))
  Check 'sessions' 'closing a session releases its shell' (@($run | Where-Object { $_ -match 'pty: session 2 closed' }).Count -ge 1)
  Check 'sessions' 'session 1 is untouched by all of it and still works' ((Dump-Has $run 's1-final' 'BACK-ON-ONE') -and (Dump-Has $run 's1-final' 'SESSION-ONE-ALIVE'))
  Check 'sessions' 'no start failures logged' (@($run | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  Check 'sessions' 'all shells are gone after the app quits (no orphan pwsh from this run)' ((Get-Process hudshell -ErrorAction SilentlyContinue | Measure-Object).Count -eq 0)
}

# The page's `ui` object from the dump header that follows "DUMP label=<label>".
function Get-Ui($run, [string]$label) {
  $i = -1
  for ($k = 0; $k -lt $run.Count; $k++) { if ($run[$k] -match ("DUMP label=" + [regex]::Escape($label) + '(\s|$)')) { $i = $k; break } }
  if ($i -lt 0) { return $null }
  for ($k = $i + 1; $k -lt $run.Count; $k++) { if ($run[$k] -match 'terminal dump:.* ui=(\{.*\})\s*$') { try { return ($Matches[1] | ConvertFrom-Json) } catch { return $null } } }
  return $null
}

function Suite-Shells {
  Write-Host "=== shells: tabs, profiles, switching, marks, titles, closing (no OS input) ==="
  $run = Run-Scenario @('--term', '--shellstest', '--test-profiles', '--delay', '2') 120000
  $u = Get-Ui $run 'single';   Check 'shells' 'one shell: no tab strip in auto mode' ($u -and -not $u.strip -and @($u.labels).Count -eq 1)
  $u = Get-Ui $run 'two';      Check 'shells' 'a second shell brings the tab strip up' ($u -and $u.strip -and @($u.labels).Count -eq 2 -and $u.active -eq 2)
  Check 'shells' 'PowerShell''s default title (an exe path) is shown as the profile name instead' ($u -and $u.labels[0] -eq '1 pwsh' -and $u.labels[1] -eq '2 pwsh') (($u.labels) -join ' | ')
  Check 'shells' 'profile with a start directory: the shell starts there (%TEMP% expanded)' (Dump-Has $run 'work' '\Temp')
  $u = Get-Ui $run 'cmd';      Check 'shells' 'profile command is typed into its shell and runs' ((Dump-Count $run 'cmd' 'PROFILE-COMMAND-RAN') -ge 2) ("occurrences " + (Dump-Count $run 'cmd' 'PROFILE-COMMAND-RAN'))
  Check 'shells' 'the tab label follows the program''s own window title (OSC 0 / 2)' ($u -and (@($u.labels | Where-Object { $_ -match '3 TITLE-TEST' }).Count -eq 1)) (($u.labels) -join ' | ')
  $u = Get-Ui $run 'jump1';    Check 'shells' 'jump to shell 1' ($u -and $u.active -eq 1)
  $u = Get-Ui $run 'marks';    Check 'shells' 'output in a background shell marks its tab' ($u -and (@($u.labels | Where-Object { $_ -match '^2 .*\u2022' }).Count -eq 1)) (($u.labels) -join ' | ')
  $u = Get-Ui $run 'attn';     Check 'shells' 'the attention hotkey jumps to the shell with unseen output and clears its mark' ($u -and $u.active -eq 2 -and (@($u.labels | Where-Object { $_ -match '^2 .*\u2022' }).Count -eq 0))
  $u = Get-Ui $run 'wrapback'; Check 'shells' 'previous shell wraps around from the first to the last' ($u -and $u.active -eq 4)
  $u = Get-Ui $run 'wrapfwd';  Check 'shells' 'next shell wraps around from the last to the first' ($u -and $u.active -eq 1)
  $u = Get-Ui $run 'clicked';  Check 'shells' 'clicking a tab switches to it' ($u -and $u.active -eq 3)
  $u = Get-Ui $run 'listed';   Check 'shells' 'the list hotkey shows the overlay with the active shell marked' ($u -and $u.overlay -match '\u25b6 3 ')
  $u = Get-Ui $run 'confirm';  Check 'shells' 'the first close press only asks for confirmation (shell still there)' ($u -and $u.overlay -match 'press the close key again' -and @($u.labels).Count -eq 4)
  $u = Get-Ui $run 'closed';   Check 'shells' 'the second press closes it and the right-hand neighbour takes over' ($u -and @($u.labels).Count -eq 3 -and $u.active -eq 4)
  Check 'shells' 'closing releases the shell process' (@($run | Where-Object { $_ -match 'pty: session 3 closed' }).Count -ge 1)
  $u = Get-Ui $run 'picker';   Check 'shells' 'the picker lists the profiles' ($u -and $u.picker)
  $u = Get-Ui $run 'picked';   Check 'shells' 'choosing profile 2 in the picker starts it as a new shell' ($u -and @($u.labels).Count -eq 4 -and $u.active -eq 5 -and (Dump-Has $run 'picked' '\Temp'))
  $u = Get-Ui $run 'one-left'; Check 'shells' 'closing down to one shell hides the strip again (auto mode)' ($u -and @($u.labels).Count -eq 1 -and -not $u.strip)
  Check 'shells' 'closing the last shell starts a fresh one in its place' ((Dump-Has $run 'lastclose' 'AFTER-LAST-CLOSE') -and -not (Dump-Has $run 'lastclose' 'ONE-ONE') -and @($run | Where-Object { $_ -match 'was the last one: starting it again' }).Count -eq 1)
  Check 'shells' 'no start failures logged' (@($run | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  # tab modes
  foreach ($mode in 'auto', 'always', 'overlay') {
    $r = Run-Scenario @('--term', '--tabmodes', '--tabs', $mode, '--delay', '2') 60000
    $one = Get-Ui $r 'mode-one'; $two = Get-Ui $r 'mode-two'; $nx = Get-Ui $r 'mode-next'; $later = Get-Ui $r 'mode-later'
    switch ($mode) {
      'auto'    { Check 'shells' 'tabs=auto: strip only with 2+ shells, no overlay popups' ($one -and -not $one.strip -and $two.strip -and -not $nx.overlay) }
      'always'  { Check 'shells' 'tabs=always: the strip shows even for one shell' ($one -and $one.strip -and $two.strip) }
      'overlay' { Check 'shells' 'tabs=overlay: never a strip; a quick overlay appears on switch and fades' ($one -and -not $one.strip -and -not $two.strip -and $nx.overlay -match '\u25b6' -and -not $later.overlay) }
    }
  }
  # tab labels: the program's whole title, and the program running in the shell (tab_program)
  foreach ($mode in 'known', 'all', 'off') {
    $r = Run-Scenario @('--term', '--labeltest', '--tab-program', $mode, '--delay', '2') 90000
    $t = Get-Ui $r 'p-title'; $c = Get-Ui $r 'p-codex'; $a = Get-Ui $r 'p-after'; $g = Get-Ui $r 'p-ping'; $a2 = Get-Ui $r 'p-after2'
    if ($mode -eq 'known') {
      Check 'shells' 'the tab shows the program''s whole window title (35 characters, not cut at 24)' ($t -and $t.labels[0] -ceq '1 HUDshell multi-shell phase 4 labels') (($t.labels) -join ' | ')
      Check 'shells' 'tab_program=known: a program called codex in the shell labels the tab "codex" (found below a cmd wrapper)' ($c -and $c.progs -ceq 'codex' -and $c.labels[0] -cmatch '^1 codex . HUDshell') (($c.labels) -join ' | ')
      Check 'shells' 'tab_program=known: the label goes away when the program ends' ($a -and $a.progs -ceq '' -and $a.labels[0] -ceq '1 HUDshell multi-shell phase 4 labels') (($a.labels) -join ' | ')
      Check 'shells' 'tab_program=known: an ordinary program (ping) gets no label' ($g -and $g.progs -ceq '' -and $a2 -and $a2.progs -ceq '') ("progs=" + $g.progs)
    }
    if ($mode -eq 'all') {
      Check 'shells' 'tab_program=all: any other program is named too (ping), and a known one still wins (codex)' ($c -and $c.progs -ceq 'codex' -and $g -and $g.progs -ceq 'ping' -and $g.labels[0] -cmatch '^1 ping . ' -and $a2 -and $a2.progs -ceq '') ("codex=" + $c.progs + " ping=" + $g.progs)
    }
    if ($mode -eq 'off') {
      Check 'shells' 'tab_program=off: no program labels at all' ($c -and $c.progs -ceq '' -and $g -and $g.progs -ceq '' -and $c.labels[0] -ceq '1 HUDshell multi-shell phase 4 labels') (($c.labels) -join ' | ')
    }
    Check 'shells' "tab labels ($mode): no start failures logged" (@($r | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  }
  # the window holds nine shells
  $r = Run-Scenario @('--term', '--shellsfull', '--delay', '2') 90000
  $n9 = Get-Ui $r 'nine'; $n10 = Get-Ui $r 'tenth-refused'
  Check 'shells' 'nine shells fit; the tenth is refused with a message' ($n9 -and @($n9.labels).Count -eq 9 -and $n10 -and @($n10.labels).Count -eq 9 -and $n10.overlay -match 'at most 9' -and @($r | Where-Object { $_ -match 'new shell refused' }).Count -eq 1)
}

function Suite-Layout {
  Write-Host "=== layout: split panes, presets, focus, zoom, fallback, own lists per pane, dividers (no OS input) ==="
  function Cols($sz) { if ($sz -and $sz -ne '-') { [int](($sz -split 'x')[0]) } else { 0 } }
  function Rows($sz) { if ($sz -and $sz -ne '-') { [int](($sz -split 'x')[1]) } else { 0 } }
  function Ids($u) { if ($u -and $u.layout) { (@($u.layout.cells) -join ',') } else { '?' } }
  $run = Run-Scenario @('--term', '--layouttest', '--delay', '2') 120000
  $one = Get-Ui $run 'l-single'
  Check 'layout' 'one shell: a single pane, no pane label' ($one -and $one.layout.shown -eq 'single' -and (Ids $one) -eq '1' -and -not ($one.layout.labels -join ''))
  $c1 = Cols $one.layout.sizes[0]; $r1 = Rows $one.layout.sizes[0]
  $u = Get-Ui $run 'l-side'
  Check 'layout' 'side by side: the second pane gets a new shell (layout_fill = new)' ($u -and $u.layout.shown -eq 'side-by-side' -and (Ids $u) -eq '1,2' -and @($run | Where-Object { $_ -match 'layout: started 1 shell\(s\) for empty panes' }).Count -ge 1) (Ids $u)
  Check 'layout' 'both panes are on screen, each about half as wide, with a name in the corner' ($u -and @($u.layout.visible).Count -eq 2 -and (Cols $u.layout.sizes[0]) -lt $c1 * 0.6 -and (Cols $u.layout.sizes[1]) -lt $c1 * 0.6 -and (Rows $u.layout.sizes[0]) -ge ($r1 - 2) -and (Rows $u.layout.sizes[0]) -le $r1 -and ($u.layout.labels[0] -match '^1 ') -and ($u.layout.labels[1] -match '^2 ')) (($u.layout.sizes -join ' ') + " vs ${c1}x${r1} for one pane")
  Check 'layout' 'dividers are off by default' ($u -and $u.layout.dividers -eq 0)
  Check 'layout' 'each pane shows only its own shell' ((Dump-Has $run 'l-side' 'PANE-ONE') -and -not (Dump-Has $run 'l-side' 'PANE-TWO') -and (Dump-Has $run 'l-side2' 'PANE-TWO') -and -not (Dump-Has $run 'l-side2' 'PANE-ONE'))
  $u = Get-Ui $run 'l-stacked'
  Check 'layout' 'stacked: two panes above each other, full width, about half as tall' ($u -and $u.layout.shown -eq 'stacked' -and (Ids $u) -eq '1,2' -and (Rows $u.layout.sizes[0]) -lt $r1 * 0.6 -and (Cols $u.layout.sizes[0]) -ge $c1 - 2) ($u.layout.sizes -join ' ')
  $u = Get-Ui $run 'l-grid'
  Check 'layout' 'grid: four panes, two more shells started' ($u -and $u.layout.shown -eq 'grid' -and (Ids $u) -eq '1,2,3,4' -and @($u.layout.visible).Count -eq 4 -and @($run | Where-Object { $_ -match 'layout: started 2 shell\(s\)' }).Count -ge 1) (Ids $u)
  $u = Get-Ui $run 'l-focus2'
  Check 'layout' 'the pane hotkey moves the keyboard: typing goes to the shell in pane 2 only' ($u -and $u.layout.focus -eq 1 -and $u.active -eq 2 -and (Dump-Has $run 'l-focus2' 'TYPED-IN-PANE-TWO') -and -not (Dump-Has $run 'l-jump' 'TYPED-IN-PANE-TWO'))
  $u = Get-Ui $run 'l-zoom'
  Check 'layout' 'zoom shows only the focused pane, at full size' ($u -and $u.layout.zoom -and (@($u.layout.visible) -join ',') -eq '2' -and (Cols $u.layout.sizes[0]) -ge $c1 - 2) (($u.layout.visible -join ',') + ' ' + ($u.layout.sizes -join ' '))
  $u = Get-Ui $run 'l-unzoom'
  Check 'layout' 'zoom again restores the four panes' ($u -and -not $u.layout.zoom -and @($u.layout.visible).Count -eq 4)
  $u = Get-Ui $run 'l-jump'
  Check 'layout' 'jumping to a shell already on screen focuses its pane (nothing moves)' ($u -and $u.active -eq 1 -and $u.layout.focus -eq 0 -and (Ids $u) -eq '1,2,3,4')
  $u = Get-Ui $run 'l-small'
  Check 'layout' 'a window too small for the grid shows fewer panes' ($u -and $u.layout.shown -ne 'grid' -and @($u.layout.visible).Count -lt 4 -and @($run | Where-Object { $_ -match 'layout: area \d+x\d+ cells, showing (stacked|side-by-side|single)' }).Count -ge 1) ($u.layout.shown + ' area ' + $u.layout.area)
  $u = Get-Ui $run 'l-big'
  Check 'layout' 'growing the window brings the grid back' ($u -and $u.layout.shown -eq 'grid' -and @($u.layout.visible).Count -eq 4)
  $u = Get-Ui $run 'l-closed'
  Check 'layout' 'closing a shell on screen leaves its pane empty when every other shell is showing' ($u -and (Ids $u) -eq '0,2,3,4' -and $u.layout.empties -eq 1 -and $u.active -eq 2) (Ids $u)
  $u = Get-Ui $run 'l-refill'
  Check 'layout' 'a new shell goes into the empty pane and gets the keyboard' ($u -and (Ids $u) -eq '5,2,3,4' -and $u.layout.focus -eq 0 -and $u.active -eq 5) (Ids $u)
  $u = Get-Ui $run 'l-single-again'
  Check 'layout' 'back to single: one pane, the other shells stay open as tabs' ($u -and $u.layout.shown -eq 'single' -and (Ids $u) -eq '5' -and @($u.labels).Count -eq 4)
  Check 'layout' 'no start failures logged' (@($run | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  # layout_fill = empty
  $run = Run-Scenario @('--term', '--layoutempty', '--layout-fill', 'empty', '--delay', '2') 60000
  $u = Get-Ui $run 'e-grid'
  Check 'layout' 'layout_fill = empty: the grid leaves three empty panes and starts no shell' ($u -and (Ids $u) -eq '1,0,0,0' -and $u.layout.empties -eq 3 -and @($u.labels).Count -eq 1) (Ids $u)
  $u = Get-Ui $run 'e-focus-empty'
  Check 'layout' 'an empty pane can take the focus' ($u -and $u.layout.focus -eq 1)
  $u = Get-Ui $run 'e-filled'
  Check 'layout' 'a new shell goes into the focused empty pane' ($u -and (Ids $u) -eq '1,2,0,0' -and $u.layout.focus -eq 1) (Ids $u)
  # pane_shells = own: every pane has its own list of shells
  $run = Run-Scenario @('--term', '--layoutown', '--pane-shells', 'own', '--delay', '2') 120000
  $u = Get-Ui $run 'o-side'
  Check 'layout' 'own lists: the new pane starts its own shell, and each pane lists only its own shells' ($u -and $u.layout.own -and $u.layout.gt -eq '1|2' -and (Ids $u) -eq '1,2' -and $u.layout.focus -eq 0) ("groups " + $u.layout.gt)
  $u = Get-Ui $run 'o-joined'
  Check 'layout' 'own lists: a new shell joins the focused pane (now two shells); the other pane is untouched' ($u -and $u.layout.gt -eq '1,3|2' -and (Ids $u) -eq '3,2' -and $u.active -eq 3) ("groups " + $u.layout.gt + " showing " + (Ids $u))
  Check 'layout' 'own lists: the tab strip shows only the focused pane''s two shells (not all three)' ($u -and $u.layout.te -eq 2 -and $u.layout.st -eq '1,3' -and @($u.labels).Count -eq 3) ("tabs " + $u.layout.te)
  Check 'layout' 'own lists: pane names read position/length (2/2 and 1/1)' ($u -and $u.layout.labels[0] -match '^2/2 ' -and $u.layout.labels[1] -match '^1/1 ') (($u.layout.labels) -join ' | ')
  $u = Get-Ui $run 'o-prev'
  Check 'layout' 'own lists: previous shell stays inside the focused pane (the other pane does not change)' ($u -and (Ids $u) -eq '1,2' -and $u.active -eq 1 -and (Dump-Has $run 'o-prev' 'OWN-ONE')) (Ids $u)
  $u = Get-Ui $run 'o-jump'
  Check 'layout' 'own lists: the number keys count inside the focused pane (2 does not exist there, 1 is its shell)' ($u -and $u.layout.focus -eq 1 -and $u.active -eq 2 -and $u.layout.st -eq '2' -and @($run | Where-Object { $_ -match 'no shell 2 to jump to' }).Count -ge 1) ("strip " + $u.layout.st)
  $u = Get-Ui $run 'o-second-tab'
  Check 'layout' 'own lists: a new shell in the second pane gives that pane two tabs' ($u -and $u.layout.gt -eq '1,3|2,4' -and (Ids $u) -eq '1,4' -and $u.layout.st -eq '2,4' -and $u.layout.labels[1] -match '^2/2 ') ("groups " + $u.layout.gt)
  $u = Get-Ui $run 'o-closed'
  Check 'layout' 'own lists: closing a shell shows the next one of the same pane' ($u -and $u.layout.gt -eq '1,3|2' -and (Ids $u) -eq '1,2' -and $u.active -eq 2) ("groups " + $u.layout.gt)
  $u = Get-Ui $run 'o-last-closed'
  Check 'layout' 'own lists: closing a pane''s last shell leaves it empty (nothing borrowed) and the keyboard moves to a pane with a shell' ($u -and $u.layout.gt -eq '1,3|' -and (Ids $u) -eq '1,0' -and $u.layout.empties -eq 1 -and $u.layout.focus -eq 0 -and $u.active -eq 1) ("groups " + $u.layout.gt + " showing " + (Ids $u))
  $u = Get-Ui $run 'o-merged'
  Check 'layout' 'own lists: going back to one pane keeps every shell (they become that pane''s tabs)' ($u -and $u.layout.gt -eq '1,3' -and (Ids $u) -eq '1' -and $u.layout.st -eq '1,3' -and @($u.labels).Count -eq 2) ("groups " + $u.layout.gt)
  Check 'layout' 'own lists: no start failures logged' (@($run | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  # start layout, uneven split, dividers
  $run = Run-Scenario @('--term', '--layoutstart', '--layout', 'side-by-side', '--split', '60,50', '--dividers', '--delay', '2') 60000
  $u = Get-Ui $run 'st-start'
  Check 'layout' 'layout at start: side by side with a second shell' ($u -and $u.layout.shown -eq 'side-by-side' -and (Ids $u) -eq '1,2' -and @($run | Where-Object { $_ -match 'layout at start: side-by-side' }).Count -eq 1) (Ids $u)
  $a0 = Cols $u.layout.sizes[0]; $b0 = Cols $u.layout.sizes[1]
  Check 'layout' 'split 60,50: the first pane is wider' ($u -and $a0 -gt $b0 -and (($u.layout.split) -join ',') -eq '60,50') "$a0 vs $b0"
  Check 'layout' 'pane_dividers: one divider is drawn' ($u -and $u.layout.dividers -eq 1)
  $u = Get-Ui $run 'st-dragged'
  Check 'layout' 'dragging the divider to 70% resizes the panes and tells the host' ($u -and (($u.layout.split) -join ',') -eq '70,50' -and (Cols $u.layout.sizes[0]) -gt $a0 -and @($run | Where-Object { $_ -match 'split -> 70,50' }).Count -eq 1) (($u.layout.sizes) -join ' ')
}

# The latest page dump's `ui` object / window state in a slice of the log (the `--inputtest` scenario writes both every second).
function Last-Ui([string[]]$lines) {
  for ($k = $lines.Count - 1; $k -ge 0; $k--) {
    if ($lines[$k] -match 'terminal dump:.* ui=(\{.*\})\s*$') { try { return ($Matches[1] | ConvertFrom-Json) } catch { return $null } }
  }
  return $null
}
function Last-State([string[]]$lines) {
  for ($k = $lines.Count - 1; $k -ge 0; $k--) {
    if ($lines[$k] -match 'STATE label=m rect=(-?\d+),(-?\d+),(-?\d+),(-?\d+) work=(-?\d+),(-?\d+),(-?\d+),(-?\d+)') {
      return @{ L = [int]$Matches[1]; T = [int]$Matches[2]; R = [int]$Matches[3]; B = [int]$Matches[4]; WL = [int]$Matches[5]; WT = [int]$Matches[6]; WR = [int]$Matches[7]; WB = [int]$Matches[8] }
    }
  }
  return $null
}

function Suite-Input {
  Write-Host "=== input: real picker hotkey, real mouse (tab click, divider drag, window drag, edge snap) - do not touch mouse/keyboard ==="
  Stop-HUDshell
  GTSave
  $p = $null
  try {
    $n = LineCount $log
    $p = Start-Process $dbg -ArgumentList @('--term', '--inputtest', '--interactive', '--layout', 'side-by-side', '--dividers', '--tabs', 'always', '--delay', '2') -PassThru
    $ready = $null
    for ($i = 0; $i -lt 60 -and -not $ready; $i++) {
      Start-Sleep 1
      if ($p.HasExited) { break }
      $u = Last-Ui @(Cut $log $n)
      if ($u -and $u.rects -and @($u.layout.cells).Count -eq 2 -and $u.layout.dividers -eq 1 -and @($u.rects.tabs).Count -eq 2) { $ready = $u }
    }
    $h = [GT]::Find([uint32]$p.Id)
    Check 'input' 'window found with two panes, one divider and two tabs' ($null -ne $ready -and $h -ne [IntPtr]::Zero)
    if (-not $ready) { return }
    function Now-Ui { Last-Ui @(Cut $log $n) }
    function Now-State { Last-State @(Cut $log $n) }
    function Pt($st, $dpr, $r) { return @([int]($st.L + ($r[0] + $r[2] / 2) * $dpr), [int]($st.T + ($r[1] + $r[3] / 2) * $dpr)) }
    function Click-At($pt) { [GT]::Move($pt[0], $pt[1]); Start-Sleep -Milliseconds 300; [GT]::MouseDown(); Start-Sleep -Milliseconds 120; [GT]::MouseUp(); Start-Sleep -Milliseconds 2200 }
    function Drag-To($x0, $y0, $x1, $y1) {
      [GT]::Move($x0, $y0); Start-Sleep -Milliseconds 300; [GT]::MouseDown(); Start-Sleep -Milliseconds 250
      for ($i = 1; $i -le 10; $i++) { [GT]::Move([int]($x0 + ($x1 - $x0) * $i / 10), [int]($y0 + ($y1 - $y0) * $i / 10)); Start-Sleep -Milliseconds 80 }
      Start-Sleep -Milliseconds 250; [GT]::MouseUp(); Start-Sleep -Milliseconds 2500
    }
    # 1. the real profile-picker hotkey (it takes the keyboard) and a real key press
    $nn = LineCount $log
    [GT]::HotkeyShift(0x4E); Start-Sleep -Milliseconds 2000
    $u = Now-Ui
    Check 'input' 'Ctrl+Alt+Shift+N opens the picker and takes the keyboard' ($u -and $u.picker -and [GT]::IsFg($h))
    [GT]::Key(0x31); Start-Sleep -Milliseconds 5000
    $u = Now-Ui
    Check 'input' 'the real key 1 starts that profile as a new shell, and the keyboard is given back' ((@(Cut $log $nn | Where-Object { $_ -match 'new shell 3 from profile' }).Count -eq 1) -and @($u.labels).Count -eq 3 -and -not $u.picker -and -not [GT]::IsFg($h)) ("shells " + @($u.labels).Count)
    [GT]::HotkeyShift(0x4E); Start-Sleep -Milliseconds 1800
    $open = (Now-Ui).picker
    [GT]::Key(0x1B); Start-Sleep -Milliseconds 2200
    $u = Now-Ui
    Check 'input' 'Esc closes the picker without starting a shell, and the keyboard is given back' ($open -and -not $u.picker -and @($u.labels).Count -eq 3 -and -not [GT]::IsFg($h))
    # 2. a real mouse click on a tab
    $u = Now-Ui; $st = Now-State; $dpr = $u.rects.dpr
    $t1 = @($u.rects.tabs | Where-Object { $_.id -eq 1 })[0]; $t2 = @($u.rects.tabs | Where-Object { $_.id -eq 2 })[0]
    Click-At (Pt $st $dpr $t1.r)
    $a = (Now-Ui).active
    $u = Now-Ui; $st = Now-State
    Click-At (Pt $st $dpr (@($u.rects.tabs | Where-Object { $_.id -eq 2 })[0]).r)
    $b = (Now-Ui).active
    Check 'input' 'a real click on tab 1, then on tab 2, switches to each' ($a -eq 1 -and $b -eq 2) "active $a then $b"
    # 3. dragging a pane divider with the real mouse
    $u = Now-Ui; $st = Now-State
    $dv = @($u.rects.dividers | Where-Object { $_.axis -eq 'v' })[0]
    $split0 = [int]$u.layout.split[0]
    $c = Pt $st $dpr $dv.r
    $termW = $u.rects.term[2] * $dpr
    $nd = LineCount $log
    Drag-To $c[0] $c[1] ($c[0] + [int]($termW * 0.15)) $c[1]
    $split1 = [int](Now-Ui).layout.split[0]
    Check 'input' 'dragging the divider with the real mouse moves it by about 15% and the host is told' (($split1 -ge $split0 + 11) -and ($split1 -le $split0 + 19) -and (@(Cut $log $nd | Where-Object { $_ -match 'split -> \d+,\d+' }).Count -ge 1)) "$split0 -> $split1"
    # 4. dragging the window by its top strip: far from every edge it moves exactly; near an edge it snaps.
    # The clicks above took the keyboard (clicking the terminal does, by design); give it back first, so the check at the
    # end proves that dragging and resizing do not take it.
    if ([GT]::IsFg($h)) { [GT]::Hotkey(0x47); Start-Sleep -Milliseconds 1500 }
    Check 'input' 'before the drags the keyboard is released' (-not [GT]::IsFg($h))
    $st = Now-State
    $gx = $st.L + [int](300 * $dpr); $gy = $st.T + [int](10 * $dpr)
    Drag-To $gx $gy ($gx + 40) ($gy + 30)
    $s2 = Now-State
    Check 'input' 'a real window drag far from every edge moves it by exactly the drag distance (no snapping)' ([Math]::Abs(($s2.L - $st.L) - 40) -le 3 -and [Math]::Abs(($s2.T - $st.T) - 30) -le 3) ("moved " + ($s2.L - $st.L) + "," + ($s2.T - $st.T))
    $gx = $s2.L + [int](300 * $dpr); $gy = $s2.T + [int](10 * $dpr)
    $margin = [int](8 * $dpr)
    $dx = ($s2.WL + [int](5 * $dpr)) - $s2.L
    Drag-To $gx $gy ($gx + $dx) $gy
    $s3 = Now-State
    Check 'input' 'a real drag that ends 5 px from the left screen edge snaps the window to the margin (8 px)' ([Math]::Abs($s3.L - ($s3.WL + $margin)) -le 2 -and [Math]::Abs($s3.T - $s2.T) -le 3) ("left " + ($s3.L - $s3.WL) + " px from the work area, top moved " + ($s3.T - $s2.T))
    $gx = $s3.L + [int](300 * $dpr); $gy = $s3.T + [int](10 * $dpr)
    $dy = ($s3.WT + [int](4 * $dpr)) - $s3.T
    Drag-To $gx $gy $gx ($gy + $dy)
    $s4 = Now-State
    Check 'input' 'a real drag that ends 4 px from the top edge snaps to the margin there too' ([Math]::Abs($s4.T - ($s4.WT + $margin)) -le 2) ("top " + ($s4.T - $s4.WT) + " px from the work area")
    # 5. resizing by dragging the right edge outwards (the edge handles are drawn by the page)
    $s5 = Now-State
    $ex = $s5.R - [int](2 * $dpr); $ey = $s5.T + [int](($s5.B - $s5.T) / 2)
    Drag-To $ex $ey ($ex + 60) $ey
    $s6 = Now-State
    Check 'input' 'a real drag of the right edge 60 px outwards makes the window about 60 px wider (same top-left)' (([Math]::Abs(($s6.R - $s5.R) - 60) -le 4) -and ($s6.L -eq $s5.L) -and ($s6.T -eq $s5.T)) ("width " + ($s5.R - $s5.L) + " -> " + ($s6.R - $s6.L))
    Check 'input' 'moving, snapping and resizing with the mouse never took the keyboard' (-not [GT]::IsFg($h))
    Check 'input' 'no start failures logged' (@(Cut $log $n | Where-Object { $_ -match 'FAILED to start' }).Count -eq 0)
  } finally {
    Stop-HUDshell
    [GT]::RestoreCursor()
  }
}

function Tree-Ids([int]$rootPid) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId
  $ids = New-Object System.Collections.Generic.HashSet[int]; [void]$ids.Add($rootPid)
  $grew = $true
  while ($grew) { $grew = $false; foreach ($x in $all) { if ($ids.Contains([int]$x.ParentProcessId) -and -not $ids.Contains([int]$x.ProcessId)) { [void]$ids.Add([int]$x.ProcessId); $grew = $true } } }
  return $ids
}
function Tree-Cpu([int]$rootPid) {
  $sum = 0.0
  foreach ($id in (Tree-Ids $rootPid)) { $pr = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($pr) { try { $sum += $pr.TotalProcessorTime.TotalSeconds } catch { } } }
  return $sum
}
function Suite-Cost {
  Write-Host "=== cost: memory and idle CPU with 1, 3 and 9 shells (sends Ctrl+Alt+N; do not touch the keyboard) ==="
  Stop-HUDshell
  $p = $null
  try {
    $n = LineCount $log
    $p = Start-Process $dbg -ArgumentList '--idle' -PassThru
    Start-Sleep 10
    $h = [GT]::Find([uint32]$p.Id)
    Check 'cost' 'window found' ($h -ne [IntPtr]::Zero)
    if ($h -eq [IntPtr]::Zero) { return }
    $have = 1; $rows = @()
    foreach ($target in 1, 3, 9) {
      while ($have -lt $target) { [GT]::Hotkey(0x4E); $have++; Start-Sleep -Milliseconds 1500 }
      Start-Sleep 12
      $mem = Tree-Memory $p.Id
      $c0 = Tree-Cpu $p.Id; Start-Sleep 10; $c1 = Tree-Cpu $p.Id
      $cpu = [Math]::Round((($c1 - $c0) / 10.0) * 100, 1)
      $started = @(Cut $log $n | Where-Object { $_ -match 'pty: started .* for session' }).Count
      $rows += [pscustomobject]@{ Shells = $target; HostMB = [Math]::Round($mem[0] / 1MB, 1); TreeMB = [Math]::Round($mem[1] / 1MB, 0); CpuPct = $cpu; Started = $started }
      Check 'cost' ("$target shell(s): all running (" + $started + " started), tree " + [Math]::Round($mem[1] / 1MB, 0) + " MB (host " + [Math]::Round($mem[0] / 1MB, 1) + " MB), idle CPU " + $cpu + "% of one core over 10 s") (($started -eq $target) -and ($mem[1] -lt 1.5GB) -and ($cpu -lt 10)) 'sanity bounds: under 1.5 GB and under 10% of one core'
    }
    if ($rows.Count -eq 3) {
      $per = [Math]::Round(($rows[2].TreeMB - $rows[0].TreeMB) / 8.0, 0)
      Check 'cost' "each extra shell costs about $per MB (1 -> 9 shells: $($rows[0].TreeMB) -> $($rows[2].TreeMB) MB)" ($per -lt 150)
    }
  } finally { Stop-HUDshell }
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
    'place' { Suite-Place }
    'qol' { Suite-Qol }
    'sessions' { Suite-Sessions }
    'shells' { Suite-Shells }
    'layout' { Suite-Layout }
    'input' { Suite-Input }
    'cost' { Suite-Cost }
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
