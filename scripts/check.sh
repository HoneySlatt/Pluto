#!/usr/bin/env bash
# Every automated check of the definition of done, in one command; CI runs it unchanged
# (.github/workflows/ci.yml, docs/adr/0012-the-checks-and-ci.md). Run it in `nix develop`, which
# provides the toolchain, cargo-deny and lychee:
#
# - formatting, then clippy with warnings as errors, over the whole workspace: the desktop client,
#   pluto-core, and the Android app's Rust core, pluto-ffi;
# - the workspace's tests;
# - the dependencies: their licenses, the advisories against them and where they come from
#   (cargo deny, policy in deny.toml; it fetches the advisory database, so it needs the network);
# - the links between the Markdown files that git tracks or has been told to add, and their
#   anchors, offline (lychee);
# - the recordings of agents' sessions: their stamp still matches what a replay reads
#   (scripts/fork-replay.sh --check-stamp, whose head lists it); the crate's own tests read, scan
#   and replay each recording.
#
# With --apk, in `nix develop .#mobile`, the Android app's Kotlin unit tests and its build too; CI
# does not run them (docs/adr/0012-the-checks-and-ci.md).
#
# With --fork, on the owner's machine only, first every recording replayed through the fork's own
# adapters, which writes the stamp (scripts/fork-replay.sh; PLUTO_FORK names the fork, by default
# ~/Projects/pluto-server); CI cannot reach the fork
# (docs/adr/0025-agents-sessions-recorded-and-replayed.md).
set -euo pipefail
cd "$(dirname "$0")/.."

apk=false
fork=false
for option in "$@"; do
  case "$option" in
    --apk) apk=true ;;
    --fork) fork=true ;;
    *)
      echo "usage: scripts/check.sh [--apk] [--fork]" >&2
      exit 2
      ;;
  esac
done

step() { printf '\n== %s\n' "$1"; }

if $fork; then
  step "Recordings replayed through the fork"
  scripts/fork-replay.sh --fork "${PLUTO_FORK:-$HOME/Projects/pluto-server}"
fi

step "Formatting"
cargo fmt --all --check

step "Clippy, warnings as errors"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "Tests"
cargo test --workspace --locked --quiet

step "Dependencies (licenses, advisories, sources)"
cargo deny --locked check --hide-inclusion-graph

step "Links between documents"
mapfile -d '' markdown < <(git ls-files -z --cached -- '*.md')
lychee --offline --include-fragments --no-progress "${markdown[@]}"

step "Recordings' stamp"
scripts/fork-replay.sh --check-stamp

if $apk; then
  step "Android unit tests"
  (cd android && gradle --no-daemon --quiet testDebugUnitTest)

  step "Android build"
  scripts/android-apk.sh
fi

step "All checks passed"
