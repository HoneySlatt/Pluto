# 0012. The checks in one script, run by CI in the Nix shell

- Status: Accepted
- Date: 2026-11-07

## Context

The definition of done in `AGENTS.md` asks that `scripts/check.sh` pass. Until M1.1 it ran
formatting, clippy and the tests, with whatever cargo the machine had, and no CI ran it: nothing
checked the dependencies' licenses or advisories, nor the links between documents. The owner's
method asks for one command holding every automated check, run unchanged by CI in the project's
development shell, and once a week so that a project at rest still hears of new advisories.

## Decision

**One script.** `scripts/check.sh` runs formatting, clippy with warnings as errors, the tests, the
dependencies' check and the documents' links. Its head lists them. `--apk` adds the Android app's
Kotlin unit tests and its build.

**cargo-deny for the dependencies** (MIT OR Apache-2.0). `deny.toml` lists the licenses accepted,
fails on any advisory but the ones it names with their reason, refuses crates from unknown
registries or git, and lists duplicate versions without refusing them. It fetches the RustSec
database at each run, so this step needs the network.

**lychee for the links** (MIT OR Apache-2.0), offline, with anchors: it checks every link between
the Markdown files git tracks, and to their headings, and never reaches the web. A new file counts
once it is added to git, so that untracked notes never fail the check.

**The flake's default shell is the one shell.** `nix develop` provides the toolchain (Rust stable
from rust-overlay, pinned by `flake.lock`, with clippy and rustfmt only), cargo-deny, lychee and
the libraries eframe loads. `shell.nix`, which gave only those libraries over the machine's own
cargo, is removed: a local run and CI use the same tools. The `mobile` shell carries the checks'
tools too, for `--apk`.

**CI on GitHub Actions.** `.github/workflows/ci.yml` installs Nix and runs
`nix develop --no-update-lock-file --command scripts/check.sh` on every push to any branch, and
every Monday. Actions are pinned to commit SHAs. The crates and `target/` are cached between runs,
keyed by `Cargo.lock` and `flake.lock`, and saved even when a check fails. CI builds without
incremental files and with line tables only for debug information, which changes the cache's size,
not what is checked. The job's limit, 45 minutes, lets a run without the cache finish and fill it,
and stops a run that hangs; the budget and the figures are in `docs/measurements.md`.

**Android stays out of CI.** CI checks `pluto-ffi` on the host, with clippy and its tests; the
Kotlin tests and the APK need the Android SDK and NDK, which `--apk` runs locally. Bringing them to
CI waits for a need, such as a release of the app.

## Alternatives considered

- **cargo-audit and cargo-about**: cargo-audit checks advisories only; cargo-about, already used for
  the APK's notice, would check licenses. Two tools and two policies, and nothing on the crates'
  sources. Rejected by the owner for cargo-deny.
- **A link checker of our own**: a short script would see missing files but not broken anchors, and
  would be more code to keep. Rejected by the owner for lychee.
- **Keeping `shell.nix` beside the flake**: two shells to keep in step, and the machine's cargo
  locally against another in CI.
- **CI outside the Nix shell** (a setup action for Rust): faster to start, but CI would no longer run
  what a contributor runs.

## Consequences

- CI runs the same script with the same pinned tools as a local run. It can still differ: its
  runner is smaller, so slower; cargo-deny needs the network; and `nix develop` keeps the host's
  `PATH` behind its own.
- The weekly run happens on the default branch, `main`, only, as GitHub schedules it there.
- CI on a private repository spends the owner's GitHub minutes; the cache keeps a run short.
- The advisories ignored in `deny.toml` are debt, each followed in `docs/PLAN.md`.
