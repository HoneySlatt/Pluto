#!/usr/bin/env bash
# Builds the Android app for one ABI (arm64 by default, x86_64 for the
# emulator): the Rust core in release, the app minified, signed with
# Gradle's debug key for installing by hand. Checks the APK carries that ABI
# only. Run it in `nix develop .#mobile`.
#
#   scripts/android-apk.sh            # arm64-v8a
#   scripts/android-apk.sh x86_64     # x86_64
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
case "${1:-arm64}" in
  arm64 | aarch64) abi=arm64-v8a ;;
  x86_64) abi=x86_64 ;;
  *) echo "usage: $0 [arm64|x86_64]" >&2; exit 2 ;;
esac
(cd "$here/android" && gradle --no-daemon --quiet assembleRelease -Pabi="$abi")
built="$here/android/app/build/outputs/apk/release/app-release.apk"
apk="$here/android/app/build/outputs/apk/release/pluto-$abi.apk"
cp "$built" "$apk"
abis=$(unzip -Z1 "$apk" 'lib/*' | cut -d/ -f2 | sort -u | tr '\n' ' ')
if [ "$abis" != "$abi " ]; then
  echo "error: $apk carries [$abis], expected [$abi]" >&2
  exit 1
fi
echo "APK ($abi, signed with the debug key): $apk"
