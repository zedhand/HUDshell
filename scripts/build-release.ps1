# Build the release executable with machine-specific paths removed from the binary.
# Rust embeds source paths (for panic messages) from the build machine, including the user name;
# --remap-path-prefix rewrites them to generic ones. Paths are computed here, never committed.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$root = (Get-Location).Path
$home_ = $env:USERPROFILE
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $home_ '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $home_ '.rustup' }
# When several prefixes match, the last one given wins, so the broad one goes first.
$flags = @(
  "--remap-path-prefix=$home_=/home",
  "--remap-path-prefix=$cargoHome=/cargo",
  "--remap-path-prefix=$rustupHome=/rustup",
  "--remap-path-prefix=$root=/hudshell"
) -join ' '
npm install --no-audit --no-fund
npm run build
$env:RUSTFLAGS = $flags
cargo build --release
Write-Host "Built target\release\hudshell.exe"