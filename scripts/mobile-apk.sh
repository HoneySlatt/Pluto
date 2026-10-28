#!/usr/bin/env bash
# Builds the Android app for one ABI (arm64 by default, x86_64 for the
# emulator) and checks the APK carries that ABI only: dx keeps the libraries
# of earlier builds in its Gradle project, so a phone APK built after an
# emulator one would ship both. Run it in `nix develop .#mobile`; the notice
# data (scripts/mobile-notice.sh) is regenerated first.
#
#   scripts/mobile-apk.sh            # aarch64-linux-android
#   scripts/mobile-apk.sh x86_64     # x86_64-linux-android
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
case "${1:-arm64}" in
  arm64 | aarch64) target=aarch64-linux-android abi=arm64-v8a ;;
  x86_64) target=x86_64-linux-android abi=x86_64 ;;
  *) echo "usage: $0 [arm64|x86_64]" >&2; exit 2 ;;
esac
project="$here/target/dx/Pluto/release/android/app"
# The libraries of an earlier build for another ABI go first.
rm -rf "$project/app/src/main/jniLibs"
"$here/scripts/mobile-notice.sh"
(cd "$here/mobile" && dx bundle --platform android --release --target "$target")
apk="$project/app/build/outputs/apk/debug/app-debug.apk"
abis=$(unzip -Z1 "$apk" 'lib/*' | cut -d/ -f2 | sort -u | tr '\n' ' ')
if [ "$abis" != "$abi " ]; then
  echo "error: $apk carries [$abis], expected [$abi]" >&2
  exit 1
fi
echo "APK ($abi, signed with the debug key): $apk"
