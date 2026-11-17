# 0044. Five platforms, the three new ones at going public, the core portable now

- Status: Accepted
- Date: 2026-11-17

## Context

Pluto runs on Linux, and on Android through its Kotlin app
([ADR-0002](0002-the-android-app-until-uwui.md)). Windows and macOS were left to going public, at
Stage 8 ([ADR-0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md),
[ADR-0035](0035-the-sandbox-and-the-vault.md), [ADR-0040](0040-out-of-the-plan.md)); iOS was named
nowhere. On 2026-11-17 the owner said that Pluto is to run on Linux, Windows, macOS, Android and
iOS.

What each platform asks of Pluto:

- **What is portable already in design**: the session log, the protocol, Pluto's loop, its
  provider clients and the chat. M2.0 plans the platform-free crates checked on Windows and macOS
  in CI, and platform code behind one boundary.
- **What is Linux's today**: the sandbox (bubblewrap; Seatbelt on macOS, AppContainer on Windows,
  where Codex keeps three backends), the keyring (Secret Service; the Keychain, Windows' Credential
  Manager), a session's own unit ([ADR-0043](0043-each-session-in-its-own-unit.md); launchd, a job
  object or a service), terminals (ConPTY on Windows) and shells.
- **What breaks a format if it is designed for Linux alone**: Windows cannot rename a file over one
  that is open, and locks files its own way; macOS and Windows file systems ignore case by default.
  The durable write and the log's one-writer lock of M2.0 and M2.1 meet both.
- **iOS starts no other program.** No vendor's CLI, no sandbox of Pluto's own: Pluto there is a
  client of the owner's server through the relay ([ADR-0007](0007-several-servers-and-a-relay.md)),
  with the chat and the light form of Pluto's loop that calls providers' APIs, which
  [ADR-0018](0018-the-markets-features-and-the-triage.md) gives the phone. It needs a paid Apple
  developer account and a Mac with Xcode; uwui counts iOS as a trial only.

The owner chose among three timings.

## Decision

**Pluto targets five platforms: Linux, Windows, macOS, Android and iOS.**

**Linux and Android first, as today; Windows, macOS and iOS at going public, Stage 8**, the client
and the server whole on Windows and macOS, the client alone on iOS.

**The core is portable from M2.0.** The crates that need no platform build and pass their tests on
Windows and macOS in CI. The durable write and the log's one-writer lock are designed for Windows
and macOS too, and tested there. Identifiers never derive from a path's case. Platform code (the
sandbox, the keyring, a session's unit, terminals, shells, paths) stays behind one boundary, with
Linux's side alone built until Stage 8.

## Alternatives considered

- **Clients on every platform early, the server on Linux until Stage 8.** Pluto sooner on the
  owner's other devices, as soon as uwui allows. But a client per platform to hold at the bar from
  Stage 5, which the owner chose not to take on yet.
- **Everything whole earlier, at Stage 5.** The most complete. But every stage held at the bar on
  three systems with three sandboxes, while Windows and macOS are checked only in the owner's
  virtual machines.

## Consequences

- Stage 8 brings Windows, macOS and iOS, beside the packages; the ROADMAP says so.
- M2.0's checks build the portable crates on Windows and macOS runners; their cost in CI minutes is
  measured with M2.0's other figures.
- Nothing Linux-only goes into a persisted format or the protocol: a path is stored as Pluto
  writes it, never case-folded, and line endings are fixed by the format, not the platform.
- The owner's Mac virtual machine is Intel with an AMD GPU; whether it runs the Xcode that iOS
  builds need is checked before Stage 8, by the owner.

## Validation and reconsideration

Reviewed reasoning; nothing is built yet. The decision holds while the portable crates pass on
Windows and macOS in CI, and the durable write and the lock's tests pass there.

A new decision would be proposed to the owner if uwui brings its macOS or iOS host early enough to
change the cost of a client there, or if the owner starts to work daily on a Mac, Windows or an
iPhone.
