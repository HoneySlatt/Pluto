# Runtime libraries eframe loads dynamically (wgpu/Vulkan, Wayland, X11).
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
  ]);
}
