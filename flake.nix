{
  description = "Pluto: the desktop client and the Android app's build tools";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
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
      buildTools = "35.0.0";
      android = pkgs.androidenv.composeAndroidPackages {
        platformVersions = [
          "34"
          "35"
        ];
        buildToolsVersions = [
          "34.0.0"
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
      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
        targets = [
          "aarch64-linux-android"
          "x86_64-linux-android"
        ];
      };
    in
    {
      devShells.${system} = {
        emulator = pkgs.mkShell {
          packages = [
            emulator.androidsdk
            emulator.platform-tools
          ];
          ANDROID_HOME = "${emulator.androidsdk}/libexec/android-sdk";
          ANDROID_SDK_ROOT = "${emulator.androidsdk}/libexec/android-sdk";
        };
        mobile = pkgs.mkShell {
          packages = [
            rust
            pkgs.dioxus-cli
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
