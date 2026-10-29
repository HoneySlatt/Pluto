#!/usr/bin/env bash
# The checks a change must pass: formatting, clippy and the tests of the
# whole workspace (the desktop client, pluto-core and the Android app's
# Rust core, pluto-ffi); with --apk, the Android build too (in `nix develop
# .#mobile`). The visual comparison with the reference stays manual
# (examples/snapshot.rs against scripts/reference/shot.sh, see README).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --quiet
if [ "${1:-}" = "--apk" ]; then
  scripts/android-apk.sh
fi
echo "checks passed"
