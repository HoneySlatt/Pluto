# Measurements

The figures measured against the budgets, with how each was taken. The product's budgets are in
the [ROADMAP](ROADMAP.md)'s quality bar, a milestone's own in its [PLAN](PLAN.md) entry; this file
holds what was measured, and when.

## The check

Budget: `scripts/check.sh` takes at most 20 minutes, locally and in CI
([M1.1](PLAN.md#m11--under-the-method)).

| Where | Build | Duration | Date |
|---|---|---|---|
| The owner's desktop, 16 threads | cold: an empty `target/`, crates already downloaded | 5 min 50 s | 2026-11-07 |
| The owner's desktop, 16 threads | warm: nothing changed since the last run | 7.8 s | 2026-11-07 |
| The owner's desktop, 16 threads | `--apk` in `nix develop .#mobile`: the Kotlin tests and the APK added, the desktop's build warm | 3 min 56 s | 2026-11-07 |
| CI, `ubuntu-24.04` | first run, no cache | not yet run | |

How: the wall time of `nix develop --no-update-lock-file --command scripts/check.sh`, the Nix
shell already built; cold with `CARGO_TARGET_DIR` set to an empty directory. In the cold run,
clippy's build took 2 min 13 s, and most of the rest went to building the tests. cargo-deny
fetches the advisory database each time, which needs the network.
