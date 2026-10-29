// Pluto on Android: the Compose app over the Rust core (crates/pluto-ffi).
// `preBuild` builds the core for the ABI asked (`-Pabi=x86_64` for the
// emulator, arm64-v8a by default) with cargo-ndk and generates its Kotlin
// bindings with UniFFI. Run it in `nix develop .#mobile`.

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

val workspace = rootDir.parentFile
val abi = (findProperty("abi") as String?) ?: "arm64-v8a"
val rustLibs = layout.buildDirectory.dir("rust/jniLibs")
val bindings = layout.buildDirectory.dir("generated/uniffi")

android {
    namespace = "app.pluto.mobile"
    compileSdk = 37
    // The SDK comes from the Nix shell (flake.nix), read-only.
    buildToolsVersion = "37.0.0"
    defaultConfig {
        applicationId = "app.pluto.mobile"
        minSdk = 28
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += abi }
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures { compose = true }
    sourceSets {
        getByName("main") {
            jniLibs.directories += rustLibs.get().asFile.path
            kotlin.directories += bindings.get().asFile.path
        }
    }
}

kotlin { jvmToolchain(17) }

val cargoRelease = !gradle.startParameter.taskNames.any { it.contains("Debug") }

val buildCore by tasks.registering(Exec::class) {
    description = "Builds the Rust core for $abi."
    workingDir = workspace
    val args = mutableListOf("cargo", "ndk", "-t", abi, "-P", "28", "-o", rustLibs.get().asFile.path, "build", "-p", "pluto-ffi")
    if (cargoRelease) args += "--release"
    commandLine(args)
}

val generateBindings by tasks.registering(Exec::class) {
    description = "Generates the core's Kotlin bindings."
    dependsOn(buildCore)
    workingDir = workspace
    val profile = if (cargoRelease) "release" else "debug"
    val target = if (abi == "x86_64") "x86_64-linux-android" else "aarch64-linux-android"
    commandLine(
        "cargo", "run", "-q", "-p", "uniffi-bindgen", "--", "generate",
        "--library", "target/$target/$profile/libpluto_ffi.so",
        "--language", "kotlin", "--no-format",
        "--out-dir", bindings.get().asFile.path,
    )
}

tasks.named("preBuild") { dependsOn(generateBindings) }

dependencies {
    val bom = platform("androidx.compose:compose-bom:2026.09.00")
    implementation(bom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("androidx.core:core-ktx:1.19.1")
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
