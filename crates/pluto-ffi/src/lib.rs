//! Pluto's core for the Android app, exposed to Kotlin through UniFFI.

uniffi::setup_scaffolding!();

/// The core's version, for the About row.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
