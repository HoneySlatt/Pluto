# 0009. Secrets, privacy and the agents' sandbox

- Status: Accepted
- Date: 2026-11-06

## Context

Several secrets exist:
- an API key per provider that needs one: Z Code, Brave Search, DeepSeek;
- an access token per device;
- the agents' own sign-ins, kept by each CLI.

Today the client asks for its token at start and keeps it in memory. The phone seals it with an
Android Keystore key. Agents run with all of the owner's rights. They read `.env` files and logs
that may hold keys, and what they read is shown, exported and notified. Most rivals send telemetry
by default, and some keep conversations on their servers.

## Decision

**Nothing leaves the owner's machines but the agents' own traffic.**
- No telemetry.
- Crash reports stay on the machine.
- No update is checked without consent.
- The relay sees only encrypted bytes.

**Secrets live in the system's keyring.** On Linux that is Secret Service, on Android the Keystore.
They are entered by the owner and never copied into a log, an export or an agent's capture. Each
paired device has its own key, revocable alone. Pluto never looks for, reads or keeps the
credentials of other programs: each agent's CLI keeps its own sign-in.

**Agents run in a sandbox that can be adjusted.** It is set per project or per thread. The modes go
from free to locked: the project's folder only, the network filtered, a memory ceiling. On Linux it
uses bubblewrap. Unattended tasks, scheduled or overnight, are locked by default.

**Secrets that agents meet are masked.** Pluto recognises the known shapes of keys and tokens and
masks them on screen, in exports and in notifications. One action reveals them on screen.

## Alternatives considered

- **An encrypted file opened by a passphrase**: independent of the system, but a passphrase at
  each start.
- **Memory only, as today**: nothing on disk, but unattended tasks cannot run, and every start asks
  again.
- **Always sandboxed**: the safest, but friction every day with agents' tools.
- **Showing what agents saw**: faithful, but a key can end in a notification on the phone.

## Consequences

- A server on a machine without a graphical session needs a keyring that works without one. How is
  decided when the server's secrets are built.
- The sandbox on Windows and Android is decided when Pluto goes public.
- Masking has false positives. The patterns are kept in one place, with tests.
