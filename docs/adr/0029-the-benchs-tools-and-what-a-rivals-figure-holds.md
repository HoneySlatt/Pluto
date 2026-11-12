# 0029. The bench's tools for the six, and what a rival's figure holds

- Status: Accepted; amends [0028](0028-the-open-rivals-and-the-web-chats-measured.md) (its tools,
  named)
- Date: 2026-11-12

## Context

[ADR-0028](0028-the-open-rivals-and-the-web-chats-measured.md) asked for a Secret Service keyring
for every subject and a route out for preparing profiles, without naming the tools. Probing the six
rivals in M1.6b2a, on 2026-11-11 and 12, found:

- **Jan's AppImage does not start under nixpkgs' `appimage-run`.** Its binary links
  `libgpg-error.so.0`, which the AppImage does not bundle and `appimage-run`'s environment lacks.
- **Jan's AppImage forces GTK's X11 backend** (its `apprun-hooks/linuxdeploy-plugin-gtk.sh`:
  "Crash with Wayland backend on Wayland"). On a Wayland desktop it runs through Xwayland, which
  the compositor starts. Xwayland then does much of Jan's drawing: in the probe, 33 s of the GPU's
  graphics engine against 27 s for Jan's own processes. Sway starts Xwayland in its own cgroup,
  which no subject's figure counts.
- **A profile prepared with a route out records the namespace's host name** in Chromium's and
  Electron's locks (`SingletonLock`), and Chromium refuses a profile that another computer seems to
  hold.

## Decision

The owner's decisions of 2026-11-11 and 2026-11-12.

**The keyring is gnome-keyring**, the one most Linux desktops run: its login keyring, in the
throwaway home, made at the first start and unlocked by a test password at each run, in the bench's
part, not the subject's. With an empty password gnome-keyring makes no login keyring, and the
subjects fall back to files of their own; a run whose keyring has no unlocked default stops.

**The route out is pasta** (passt), the user-mode network that podman uses by default, during
`prepare` only; a measured run is always offline. It forwards no port in either direction and maps
no address to the machine's loopback or its own address; the namespace refuses private and shared
addresses (the home network, the router), so a subject reaches only the Internet, never the owner's
services. Names resolve through pasta's forwarder.

**The sandbox is a machine of its own**: its host name (`bench`, in a namespace of its own) and its
machine id are the bench's, never the owner's, which an application calling home at its first start
could send. Nix's daemon is hidden from it. The Nix store stays visible, read-only, as the programs
need it; what the owner's own configuration put there is readable, as on the machine.

**`appimage-run` gains `libgpg-error`** in the measure shell (`extraPkgs`), as a distribution
provides it. Every AppImage subject sees it; their runs stayed valid with it.

**Xwayland is counted with a subject that draws through it.** Once sway starts it for Jan, the bench
moves it into the subject's cgroup, as a part of its own (`xwayland`): in the whole, shown apart
too, as a web rival's server and browser are. It is what a user of Jan on Wayland pays.

**A frozen profile holds no running program's locks.** `prepare` removes the `Singleton*` links
before it freezes a home, as it removes a pending update, and refuses a home that still holds one.

## Alternatives considered

- **oo7-daemon** for the keyring: light, in Rust, but young, its use by Electron's and Tauri's
  libsecret not proven. **KeePassXC**: an application, with a window of its own.
- **An empty password for the keyring**: the first choice, which left the subjects without a
  keyring.
- **slirp4netns**: the same role as pasta, slower, giving way to it. **The machine's own network**:
  no tool, but a driver would reach the owner's own server on 3773.
- **Jan from nixpkgs**: the same AppImage wrapped, but 0.8.7 in the flake's nixpkgs where the vendor
  publishes 0.8.8. **Jan with a written reason**: no change to the bench, no figure for Jan.
- **Xwayland left to the compositor**: Jan measured lighter than it is. **Jan forced onto Wayland**
  (`GDK_BACKEND=wayland`): against its vendor's choice, which says it crashes.

## Consequences

- The measure shell holds gnome-keyring (LGPL-2.1+, GPL-2.0+), passt (BSD-3-Clause,
  GPL-2.0+) and `libgpg-error` (LGPL-2.1+), beside ADR-0028's skopeo, umoci and Chromium.
- A subject that runs through X11 declares an `xwayland` part, and moves Xwayland into it.
- Jan's figure holds its Xwayland's; a later rival through X11 is counted alike.
