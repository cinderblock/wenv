#!/usr/bin/env bash
# Size-optimized wenv build: nightly `build-std` + the `immediate-abort` panic
# strategy, which rebuilds std without unwinding/panic-formatting machinery.
#
# Prerequisite (one time):
#   rustup toolchain install nightly --component rust-src
#
# Usage:
#   ./scripts/build-min.sh                 # host target
#   ./scripts/build-min.sh <target-triple> # explicit target
set -euo pipefail

target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

flags="-Zunstable-options -Cpanic=immediate-abort"
case "$target" in
*windows-msvc)
    # RUSTFLAGS (env) replaces, not merges with, .cargo/config.toml rustflags, so
    # re-add crt-static to keep the Windows binary self-contained.
    flags="$flags -C target-feature=+crt-static"
    ;;
esac

RUSTFLAGS="$flags" cargo +nightly build --release -Z build-std=std,panic_abort --target "$target"
echo "built: target/$target/release/wenv"
