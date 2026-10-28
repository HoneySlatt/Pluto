#!/usr/bin/env bash
# Generates the Android app's license data with cargo-about (every crate the
# app links for aarch64-linux-android, licenses from the crates' own files,
# offline). mobile/build.rs turns it into the Licenses page and the full
# notice. Run it in `nix develop .#mobile` before `dx bundle`.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$here/mobile/notice"
cd "$here/mobile"
cargo about generate --offline --format json -o notice/about.json
echo "notice data: $here/mobile/notice/about.json"
