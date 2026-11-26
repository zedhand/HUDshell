# Roadmap

Open work, in the order it is being done. Verified behaviour is in [test-results.md](test-results.md); untested and unimplemented things are in [known-gaps.md](known-gaps.md).

## PowerShell audit of the build and test scripts (2026-11-25)

Source: an audit of `scripts/build-release.ps1` and `scripts/test-all.ps1`. Numbers are the audit's own. Order of work: runner trust first (C1, M2), then the build script (H3, H4), config safety (H2, M9), the static suite (H1), input safety (M3, M4, M5), then the rest.

| Id | Issue | Status (2026-11-26) |
|---|---|---|
| C1 | A check that throws vanishes instead of failing; no count of checks that ran | done: `Invoke-Suite` guard, `$MaximumErrorCount`, guarded reads in `windowsnap` / `winrestore` / `autotest` |
| M2 | A mistyped `-Suite` name exits 0 | done: an unknown suite name is a FAIL; a run with no check is a FAIL |
| H3 | `build-release.ps1` ignores native exit codes | done: `$PSNativeCommandUseErrorActionPreference`, `#Requires -Version 7.4` |
| H4 | `RUSTFLAGS` splits on a space in a path, leaks into the caller's session, changes the caller's folder | done: `CARGO_ENCODED_RUSTFLAGS`, `Push-Location` / `Pop-Location`, the caller's session is left as it was |
| H2 | The config backup can be deleted while the config is not restored | done: restore stops on error, the scratch folder is kept when a config check fails, the backup path is printed |
| M9 | `Restore-Config` can delete the user's own `config.json.*` files | done: only files that were not there before the run are removed |
| H1 | The static suite passes when the release exe or a UI file is missing | done: the static suite fails when an exe or UI file is missing, checks `cargo` exit code |
| M3 | `Stop-HUDshell` kills every `hudshell.exe`, including the user's own | done: only test copies are stopped; another HUDshell is reported by the preflight |
| M4 | The neutral-window helper clicks without checking what it clicks | done: the neutral helper clicks only when its own window is under the point; no neutral window for `static` |
| M5 | The mouse button / Shift can stay held after Ctrl+C during a drag | done: `RestoreCursor` releases the mouse button and Shift / Ctrl / Alt |
| M1 | `Start-Process -ArgumentList` does not quote arguments with a space | done: `Start-App` quotes arguments with a space |
| M6 | `0xFFFFFFFF` is Int32 -1: the layout-handle mask does nothing | done: `0xFFFFFFFFL` |
| M7 | Elevated helper scripts are written to `%TEMP%` and run with Bypass | done: elevated commands go through `-EncodedCommand`, values quoted as literals |
| M8 | PresentMon is downloaded unpinned and run elevated | done: PresentMon pinned to v2.6.0 and its SHA256; any other file is deleted and not run |
| M10 | The test log grows forever and is read in full at every check | done: the old log is set aside at the start of each run |
| L1 | PowerShell 5.1 claimed, but no `#Requires` and `Get-Content -Raw` without `-Encoding` | done: `#Requires -Version 7.0` |
| L2 | Clipboard restore loses images/files and has no retry | done: files / images / text saved and restored, with retries |
| L3 | Bitmap / graphics / runspace leaks on error | done: bitmaps and graphics disposed in `finally`, the runspace is closed |
| L4 | Duplicated, drifted helpers (dump parser x9, process-tree walk, `$isGame`) | done: one `Find-DumpLabel` / `Find-DumpHeader`, one `Tree-Ids`, one `Test-GameInFront` and shared process lists |
| L5 | Dead variables | done: dead variables removed (`$us` no longer loads the US layout) |
| L6 | Usage line, a wrong comment, a `\t` escape turned into a tab in the docs | done: usage list, the comment and the tab in three docs lines |
| L7 | Loading the script creates `.claude\scratch-archive\shots` | done: the shots folder is made when the first screenshot is taken |
| L8 | `Get-States` splits on single spaces | done: split only before a `key=` |
| (check) | Full run after the audit fixes (2026-11-26) | done: 506 pass, 3 fail, 6 warn; the 3 fails were a drag-restore bug in the product, fixed and re-tested (see test-results) |
| L9 | `Stop-Process -Name explorer` kills File Explorer windows; taskbar backups never read back | done: open File Explorer folders are reopened; a backup left by an interrupted run is applied at the next start |
