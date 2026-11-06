# 0007. Several servers in one view, and a relay of Pluto's own

- Status: Accepted
- Date: 2026-11-06

## Context

The owner codes on a desktop and a modest laptop, may use a remote machine, and follows threads
from a phone. Today the server listens on the loopback only, and the phone reaches it through
Tailscale. The rivals' remote access depends on a vendor's account, such as T3 Connect or Claude's
Remote Control.

## Decision

**Each machine that codes runs a server.** Each client shows every server it is paired with in one
view. The sidebar, the dashboard and search merge their answers and say when one did not answer
([ADR-0005](0005-plutos-own-typed-protocol.md)). A thread lives on one server. Moving a thread to
another machine is a feature of its own, built later.

**Devices are paired, never logged in.** A device is paired with a QR code and gets its own key,
which can be revoked alone ([ADR-0009](0009-secrets-privacy-and-the-sandbox.md)).

**Away from home, Pluto goes through a relay of its own,** hosted by the owner, with no account.
It is built on iroh: QUIC connections between devices dialed by their public keys, direct when the
network allows it, and through a relay the owner hosts otherwise. The relay sees only encrypted
traffic. On a local or private network, devices connect directly without it.

## Alternatives considered

- **One server, several clients**: simpler, but nothing works when the desktop is off.
- **Tailscale or a private network only**: no relay to host, but Tailscale is needed on every
  device.
- **A relay and encryption written by Pluto**: no dependency, but it is code where one mistake leaks
  everything.

## Consequences

- iroh is a large dependency. Its license and its upkeep are checked when it is added.
- The phone's notifications may come through the relay, with a push service without Google's. This
  is decided when the relay is built.
- The relay is optional: a binary of its own, needed only away from home.
