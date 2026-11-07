# Measurements

The figures measured against the budgets, with how each was taken. The product's budgets are in
the [ROADMAP](ROADMAP.md)'s quality bar, a milestone's own in its [PLAN](PLAN.md) entry; this file
holds what was measured, and when.

## The check

Budget: `scripts/check.sh` takes at most 20 minutes, locally and in CI with its cache
([M1.1](PLAN.md#m11--under-the-method)). A CI run without the cache, the first of a branch or after
GitHub drops a cache unused for a while, may take longer: the owner accepted about 30 minutes on
2026-11-07.

| Where | Build | Duration | Date |
|---|---|---|---|
| The owner's desktop, 16 threads | cold: an empty `target/`, crates already downloaded | 5 min 50 s | 2026-11-07 |
| The owner's desktop, 16 threads | warm: nothing changed since the last run | 7.8 s | 2026-11-07 |
| The owner's desktop, 16 threads | `--apk` in `nix develop .#mobile`: the Kotlin tests and the APK added, the desktop's build warm | 3 min 56 s | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | cold: no cache | 30 min 07 s, accepted | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | cold: the first run on `main`, no cache for it | 23 min 01 s; the job 23 min 46 s | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | warm: the cache of the run before | 1 min 51 s; the job 3 min 00 s | 2026-11-07 |

How: the wall time of `nix develop --no-update-lock-file --command scripts/check.sh`, the Nix
shell already built; cold with `CARGO_TARGET_DIR` set to an empty directory. In the cold run,
clippy's build took 2 min 13 s, and most of the rest went to building the tests. cargo-deny
fetches the advisory database each time, which needs the network.

In CI, the script's step of the workflow, from its log
([run 42337377344](https://github.com/HoneySlatt/Pluto/actions/runs/42337377344), commit
`8d3761a`): the Nix shell 35 s, clippy 8 min, building and running the tests 21 min 30 s, cargo-deny
and the links 3 s. Saving the cache took 35 s more. Most of the time goes to the dependencies,
built at `opt-level = 2` (`Cargo.toml`, `[profile.dev.package."*"]`) on a runner far smaller than
the desktop.

The first run on `main` ([run 42340373676](https://github.com/HoneySlatt/Pluto/actions/runs/42340373676),
commit `d0d79f5`) restored no cache: a run restores only those of its own branch and of the default
branch, and every run before it was on another branch.

The warm run ([run 42339784714](https://github.com/HoneySlatt/Pluto/actions/runs/42339784714),
commit `c0d0671`): restoring the cache 59 s, the Nix shell 38 s, clippy 17 s, the tests 54 s,
cargo-deny and the links 2 s.
