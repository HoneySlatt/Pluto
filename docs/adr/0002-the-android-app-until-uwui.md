# 0002. The Android app, in Kotlin until uwui

- Status: Accepted; amended by [0018](0018-the-markets-features-and-the-triage.md) (notifications
  through UnifiedPush; models on the phone)
- Date: 2026-11-06, recording decisions of 2026-10-28 and 2026-10-29

## Context

The owner wanted to follow and continue their threads from their phone, a Pixel on GrapheneOS
without Google's services. A first app was written in Dioxus, all Rust over Android's web view. It
was dropped on 2026-10-29: its controlled text inputs dropped fast keystrokes, and images never
showed. It was rewritten in Kotlin and Jetpack Compose with the same screens. The Rust core
(`crates/pluto-core`) is exposed to Kotlin through UniFFI (`crates/pluto-ffi`).

## Decision

**The Android app is Kotlin and Compose over the shared Rust core.** The core holds the connection,
the model, the timeline and each screen's view. Compose only draws them. Notifications come from a
foreground service that keeps the connection open, since the phone has no Google push.

**It moves to uwui once uwui runs whole Android applications.** It then shares the desktop's
interface code, with screens of its own where the phone needs them. Kotlin stays for the system's
services ([ADR-0003](0003-what-pluto-is-to-become.md)).

## Alternatives considered

- **Dioxus**: all Rust, but tried and dropped for the reasons above.
- **The upstream Expo app**: a second code base in TypeScript, on T3's protocol.
- **A pure Kotlin port of the core**: native, but two implementations of the protocol and the
  timeline.

## Consequences

- The app follows the core: when the core moves to Pluto's protocol
  ([ADR-0005](0005-plutos-own-typed-protocol.md)), the app moves with it.
- Its notifications may later come through the relay
  ([ADR-0007](0007-several-servers-and-a-relay.md)) instead of a kept connection.
