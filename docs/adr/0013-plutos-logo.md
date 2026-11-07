# 0013. Pluto's logo

- Status: Accepted
- Date: 2026-11-07

## Context

Pluto's code is under MIT, which lets anyone use, change and redistribute it. The owner made a logo
for Pluto, `assets/logo/pluto.svg`. A logo names a project: under the code's license, a fork could
present itself with it. The applications have their own icon, `assets/icon.svg`, used by the
desktop client's window and `assets/pluto.desktop`.

## Decision

**The logo is not under the code's license; its author keeps its rights.** It may be shown,
unchanged, to refer to Pluto; it may not be the logo of another project, a fork included, nor
suggest that a project is Pluto or endorsed by it. The note beside it, `assets/logo/README.md`,
says so, and `NOTICE.md` names the exception. This follows uwui's logo (SlimeEngine's ADR-0183).

**The logo becomes the applications' icon with the move to uwui**, not before: the egui client is
frozen ([ADR-0010](0010-todays-features-triaged.md)), and the logo's SVG carries a raster texture of
about 1 MB, too heavy for a window's icon as it is.

## Alternatives considered

- **MIT, as the code**: simplest, but nothing would keep the logo to Pluto.
- **CC BY 4.0**: free to reuse with an attribution; a fork could keep the logo.
- **The logo as the icon now**: a change to the frozen client and the Android app, and icons to
  make at each size, for an interface about to be replaced.

## Consequences

- The repository is under MIT but for the logo, whose note governs it.
- A fork of Pluto takes a logo of its own.
- The move to uwui makes the applications' icons from the logo.
