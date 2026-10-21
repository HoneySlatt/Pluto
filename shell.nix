# Runtime libraries loaded dynamically: eframe's (wgpu/Vulkan, Wayland, X11)
# and libdbus, which rfd opens to reach the desktop portal's file chooser.
{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    vulkan-loader
    libGL
    wayland
    libxkbcommon
    libx11
    libxcursor
    libxi
    libxrandr
    dbus
  ]);
}
