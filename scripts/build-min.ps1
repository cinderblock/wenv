#!/usr/bin/env pwsh
# Size-optimized wenv build: nightly `build-std` + the `immediate-abort` panic
# strategy, which rebuilds std without unwinding/panic-formatting machinery.
#
# Prerequisite (one time):
#   rustup toolchain install nightly --component rust-src
#
# Usage:
#   ./scripts/build-min.ps1                 # host target
#   ./scripts/build-min.ps1 <target-triple> # explicit target
[CmdletBinding()]
param([string]$Target)

$ErrorActionPreference = 'Stop'

if (-not $Target) {
    $Target = ((& rustc -vV | Select-String '^host:').ToString() -split '\s+')[1]
}

$flags = @('-Zunstable-options', '-Cpanic=immediate-abort')
if ($Target -like '*windows-msvc') {
    # RUSTFLAGS (env) replaces, not merges with, .cargo/config.toml rustflags, so
    # re-add crt-static to keep the Windows binary self-contained.
    $flags += @('-C', 'target-feature=+crt-static')
}

$env:RUSTFLAGS = $flags -join ' '
& cargo +nightly build --release -Z build-std=std,panic_abort --target $Target
Write-Host "built: target/$Target/release/wenv"
