# Launcher icon

`icon.svg` is the app icon. The adaptive icon Android draws in its launcher
mask splits it in two: `background.svg` (the gradient and glow, full bleed)
and `foreground.svg` (the planet, inside the mask's safe zone).
`scripts/mobile-apk.sh` puts the PNGs of `png/` in the APK. To render them
again after changing an SVG:

```sh
cd mobile/android/icon
inkscape foreground.svg -w 432 -h 432 -o png/ic_launcher_foreground.png
inkscape background.svg -w 432 -h 432 -o png/ic_launcher_background.png
for p in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  inkscape icon.svg -w ${p#*:} -h ${p#*:} -o png/ic_launcher-${p%:*}.png
done
```
