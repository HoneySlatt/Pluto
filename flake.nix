{
  description = "Pluto: the development shells of the desktop client, the checks and the Android app";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The last nixpkgs with T3 Code v0.0.45, whose server builds the fixture
    # (scripts/fixture-server.sh). Pinned to a revision, so updates never move it.
    nixpkgs-t3-fixture.url = "github:NixOS/nixpkgs/09063bd1f2b36fd9fd793c36dfade7bd40a93dfc";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      nixpkgs-t3-fixture,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
        config = {
          allowUnfree = true;
          android_sdk.accept_license = true;
        };
      };
      buildTools = "37.0.0";
      android = pkgs.androidenv.composeAndroidPackages {
        platformVersions = [
          "34"
          "35"
          "37.0"
        ];
        buildToolsVersions = [
          "34.0.0"
          "35.0.0"
          buildTools
        ];
        includeNDK = true;
        includeEmulator = false;
        includeSystemImages = false;
      };
      sdk = "${android.androidsdk}/libexec/android-sdk";
      # A headless x86_64 phone to look at the app on this computer.
      emulator = pkgs.androidenv.composeAndroidPackages {
        platformVersions = [ "34" ];
        includeEmulator = true;
        includeSystemImages = true;
        systemImageTypes = [ "default" ];
        abiVersions = [ "x86_64" ];
      };
      # The libraries eframe loads at runtime (wgpu/Vulkan, Wayland, X11), and
      # libdbus, which rfd opens to reach the desktop portal's file chooser.
      runtimeLibraries = pkgs.lib.makeLibraryPath (
        with pkgs;
        [
          vulkan-loader
          libGL
          wayland
          libxkbcommon
          libx11
          libxcursor
          libxi
          libxrandr
          dbus
        ]
      );
      # What scripts/check.sh runs besides cargo (docs/adr/0012-the-checks-and-ci.md).
      checkTools = [
        pkgs.cargo-deny
        pkgs.lychee
      ];
      # What scripts/measure.sh runs (docs/measurements.md): the heap by call stack, repeated
      # starts, wake-ups per thread, and a headless compositor for the client's real window.
      measureTools = [
        pkgs.heaptrack
        pkgs.hyperfine
        pkgs.sysstat
        pkgs.weston
      ];
      # The desktop toolchain is kept minimal: CI downloads it at every run.
      desktopRust = pkgs.rust-bin.stable.latest.minimal.override {
        extensions = [
          "clippy"
          "rustfmt"
        ];
      };
      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
        targets = [
          "aarch64-linux-android"
          "x86_64-linux-android"
        ];
      };
    in
    {
      # T3 Code v0.0.45, about 3.5 GB with its Electron app: a package of its own, so that only
      # scripts/fixture-server.sh downloads it, rather than every shell.
      packages.${system}.fixture-t3 = nixpkgs-t3-fixture.legacyPackages.${system}.t3code;

      devShells.${system} = {
        # The desktop client and every check: `nix develop`, then scripts/check.sh.
        default = pkgs.mkShell {
          packages = [ desktopRust ] ++ checkTools;
          LD_LIBRARY_PATH = runtimeLibraries;
        };
        # The measurements of docs/measurements.md: `nix develop .#measure`, then scripts/measure.sh.
        measure = pkgs.mkShell {
          packages = [ desktopRust ] ++ measureTools;
          LD_LIBRARY_PATH = runtimeLibraries;
        };
        emulator = pkgs.mkShell {
          packages = [
            emulator.androidsdk
            emulator.platform-tools
          ];
          ANDROID_HOME = "${emulator.androidsdk}/libexec/android-sdk";
          ANDROID_SDK_ROOT = "${emulator.androidsdk}/libexec/android-sdk";
        };
        # The Android app, and `scripts/check.sh --apk`.
        mobile = pkgs.mkShell {
          packages = checkTools ++ [
            rust
            # The app (android/): Gradle, and the Rust core built for each
            # ABI.
            pkgs.gradle_9
            pkgs.cargo-ndk
            # The app's license notice (android/app/notice.gradle.kts).
            pkgs.cargo-about
            # The APK's ABI check (scripts/android-apk.sh).
            pkgs.unzip
            pkgs.jdk17
            android.androidsdk
            android.platform-tools
          ];
          ANDROID_HOME = sdk;
          ANDROID_SDK_ROOT = sdk;
          ANDROID_NDK_HOME = "${sdk}/ndk-bundle";
          JAVA_HOME = pkgs.jdk17.home;
          # Gradle's own aapt2 is a generic Linux binary NixOS cannot run.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${sdk}/build-tools/${buildTools}/aapt2";
        };
      };
    };
}
