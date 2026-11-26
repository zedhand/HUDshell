#Requires -Version 7.4
# Build the release executable with machine-specific paths removed from the binary.
# Rust embeds source paths (for panic messages) from the build machine, including the user name;
# --remap-path-prefix rewrites them to generic ones. Paths are computed here, never committed.
$ErrorActionPreference = 'Stop'
# A native command that exits non-zero now throws, so a failed npm step cannot be followed by cargo.
$PSNativeCommandUseErrorActionPreference = $true
$root = Split-Path -Parent $PSScriptRoot
$home_ = $env:USERPROFILE
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $home_ '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $home_ '.rustup' }
# When several prefixes match, the last one given wins, so the broad one goes first.
# CARGO_ENCODED_RUSTFLAGS separates flags with 0x1f, so a path with a space stays one argument.
$flags = @(
  "--remap-path-prefix=$home_=/home",
  "--remap-path-prefix=$cargoHome=/cargo",
  "--remap-path-prefix=$rustupHome=/rustup",
  "--remap-path-prefix=$root=/hudshell"
) -join [char]0x1f
$oldEncoded = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $root
try {
  npm install --no-audit --no-fund
  npm run build
  $env:CARGO_ENCODED_RUSTFLAGS = $flags
  cargo build --release
  Write-Host "Built target\release\hudshell.exe"
} finally {
  # leave the caller's session as it was: same folder, same flags
  if ($null -eq $oldEncoded) { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue } else { $env:CARGO_ENCODED_RUSTFLAGS = $oldEncoded }
  Pop-Location
}
