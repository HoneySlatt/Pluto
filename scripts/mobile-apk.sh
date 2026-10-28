#!/usr/bin/env bash
# Builds the Android app for one ABI (arm64 by default, x86_64 for the
# emulator) and checks the APK carries that ABI only: dx keeps the libraries
# of earlier builds in its Gradle project, so a phone APK built after an
# emulator one would ship both. dx 0.7 always writes its own launcher icon:
# Pluto's (mobile/android/icon, rendered from icon.svg) replaces it and
# Gradle packs the APK again. Run it in `nix develop .#mobile`; the notice
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
# The libraries of an earlier build for another ABI go first, and the icon it
# put in place of dx's (dx writes its own again).
rm -rf "$project/app/src/main/jniLibs" "$project/app/src/main/res"
"$here/scripts/mobile-notice.sh"
(cd "$here/mobile" && dx bundle --platform android --release --target "$target")
# The launcher icon: an adaptive one (the planet over the gradient), and the
# whole icon for launchers before Android 8.
res="$project/app/src/main/res" icons="$here/mobile/android/icon/png"
rm -f "$res/drawable/ic_launcher_background.xml" "$res/drawable-v24/ic_launcher_foreground.xml" "$res"/mipmap-*/ic_launcher.webp
mkdir -p "$res/drawable-nodpi"
cp "$icons/ic_launcher_background.png" "$icons/ic_launcher_foreground.png" "$res/drawable-nodpi/"
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  cp "$icons/ic_launcher-$density.png" "$res/mipmap-$density/ic_launcher.png"
done
(cd "$project" && ./gradlew --quiet assembleDebug)
apk="$project/app/build/outputs/apk/debug/app-debug.apk"
abis=$(unzip -Z1 "$apk" 'lib/*' | cut -d/ -f2 | sort -u | tr '\n' ' ')
if [ "$abis" != "$abi " ]; then
  echo "error: $apk carries [$abis], expected [$abi]" >&2
  exit 1
fi
echo "APK ($abi, signed with the debug key): $apk"
