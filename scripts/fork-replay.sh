#!/usr/bin/env bash
# Replays the agents' recorded sessions through the fork's own adapters, on the owner's machine,
# and writes the stamp that CI checks (docs/adr/0025-agents-sessions-recorded-and-replayed.md).
#
#   scripts/fork-replay.sh [--fork PATH]            the check, which writes the stamp
#   scripts/fork-replay.sh [--fork PATH] --record   one live recording, by the owner only
#   scripts/fork-replay.sh --check-stamp            CI's check of the stamp: no fork, no node
#   scripts/fork-replay.sh --check-home PATH        refuses a disposable home that is not safe
#
# The check builds pluto-record and pluto-replay-agent, runs the harness's self-test on its
# synthetic recordings, then replays every recording in crates/pluto-recording/recordings/, both
# through the fork's own test runner. It fails if any of it fails, or if the run wrote anything
# in the fork, which it checks however the run ends. Then it writes recordings/fork-replay.sha256,
# the hashes of what the replays read (see `stamped_files` and `dependencies_line`), and
# recordings/fork-replay.meta: the fork's commit, the SDK's version, the day.
#
# --check-stamp hashes the same files again and fails, naming them, when one is new, gone or
# changed since the stamp was written: the owner then runs the check again.
#
# --record runs crates/pluto-recording/fork-harness/record.test.ts with the real agent's CLI,
# which spends the owner's quota: see that file for the variables it needs. Agents never run it.
# It refuses a PLUTO_RECORD_HOME that is the owner's own home, holds it, or lies in this
# repository (`refuse_unsafe_home`, which --check-home runs alone).
#
# The fork (PATH, by default ~/Projects/pluto-server) is only read; its dependencies must be
# installed. Run the check in `nix develop`, which provides cargo; node comes from the system.
set -euo pipefail

# Read before anything changes it: the caller's home.
real_home=${HOME:-}

usage() {
  echo "usage: scripts/fork-replay.sh [--fork PATH] [--record | --check-stamp | --check-home PATH]" >&2
  exit 2
}

fail() {
  echo "fork-replay: $1" >&2
  exit 1
}

# A path the caller gave, absolute from the caller's folder.
absolute() {
  realpath -m -- "$1"
}

fork="${real_home}/Projects/pluto-server"
mode=check
while [ $# -gt 0 ]; do
  case "$1" in
    --fork)
      [ $# -ge 2 ] || usage
      fork=$(absolute "$2")
      shift 2
      ;;
    --record)
      [ "$mode" = check ] || usage
      mode=record
      shift
      ;;
    --check-stamp)
      [ "$mode" = check ] || usage
      mode=check-stamp
      shift
      ;;
    --check-home)
      [ "$mode" = check ] && [ $# -ge 2 ] || usage
      mode=check-home
      home_to_check=$2
      shift 2
      ;;
    *) usage ;;
  esac
done

# Whether the first folder is the second or holds it. A new folder inside the real home, such as
# the default ~/.local/state/pluto-recording/home, is a home of its own and is fine.
holds() {
  local outer inner
  outer="$(absolute "$1")"
  inner="$(absolute "$2")"
  outer="${outer%/}/"
  inner="${inner%/}/"
  case "$inner" in "$outer"*) return 0 ;; esac
  return 1
}

# Refuses a disposable home that would put the agent among the owner's own sign-ins, settings and
# files (the real home, or a folder holding it), or the agents' tokens in this working tree.
refuse_unsafe_home() {
  local home=$1 real repository
  [ -n "$home" ] || fail "no disposable home given"
  home=$(absolute "$home")
  repository=$(cd "$(dirname "$0")/.." && pwd)
  for real in "$real_home" "$(getent passwd "$(id -u)" 2>/dev/null | cut -d: -f6 || true)"; do
    if [ -n "$real" ] && holds "$home" "$real"; then
      fail "the disposable home, $home, is or holds your own home, $real: give a folder of its own, where you sign in once to the agents"
    fi
  done
  if holds "$repository" "$home"; then
    fail "the disposable home, $home, lies in this repository: the agents' sign-ins would sit in its working tree"
  fi
}

if [ "$mode" = check-home ]; then
  refuse_unsafe_home "$home_to_check"
  exit 0
fi

if [ "$mode" = record ]; then
  for name in PLUTO_RECORD_AGENT PLUTO_RECORD_SCENARIO PLUTO_AGENT_BIN PLUTO_RECORD_HOME PLUTO_RECORD_OUT; do
    [ -n "${!name:-}" ] || fail "--record needs $name: see crates/pluto-recording/fork-harness/record.test.ts"
  done
  # pluto-record takes absolute folders; the caller may give them from where they stand.
  PLUTO_RECORD_HOME=$(absolute "$PLUTO_RECORD_HOME")
  PLUTO_RECORD_OUT=$(absolute "$PLUTO_RECORD_OUT")
  case "$PLUTO_AGENT_BIN" in */*) PLUTO_AGENT_BIN=$(absolute "$PLUTO_AGENT_BIN") ;; esac
  export PLUTO_RECORD_HOME PLUTO_RECORD_OUT PLUTO_AGENT_BIN
  export PLUTO_REAL_HOME="$real_home"
  refuse_unsafe_home "$PLUTO_RECORD_HOME"
fi

cd "$(dirname "$0")/.."
repo=$PWD
recordings="crates/pluto-recording/recordings"
stamp="$recordings/fork-replay.sha256"
meta="$recordings/fork-replay.meta"

# What a replay reads, and so what the stamp covers: the recordings and their references, the
# crate's code and manifest, the harness, this script, and the crate's own resolved dependencies.
# Paths are those git tracks or would track, so that a stray untracked file counts for nothing.
stamped_files() {
  {
    git ls-files -z --cached --others --exclude-standard -- \
      crates/pluto-recording/Cargo.toml crates/pluto-recording/src \
      crates/pluto-recording/fork-harness crates/pluto-recording/recordings
    printf '%s\0' scripts/fork-replay.sh
  } | LC_ALL=C sort -z -u | while IFS= read -r -d '' path; do
    case "$path" in
      "$stamp" | "$meta") continue ;;
    esac
    # A tracked file deleted from the working tree is gone, not hashed.
    if [ -f "$path" ]; then printf '%s\0' "$path"; fi
  done
}

# The crate's resolved dependencies, by name and version, as one more line of the stamp: a change
# of the workspace's other dependencies leaves it as it is.
dependencies_line() {
  local hash
  # Without the trailing parenthesis: a path crate's absolute folder, or `(*)` for a repeat.
  hash=$(cargo tree --quiet --locked --offline -p pluto-recording -e normal --prefix none |
    sed -E 's/ \([^)]*\)$//' | LC_ALL=C sort -u | sha256sum | cut -d' ' -f1)
  printf '%s  %s\n' "$hash" "(the crate's resolved dependencies)"
}

stamped_hashes() {
  stamped_files | xargs -0 --no-run-if-empty sha256sum
  dependencies_line
}

if [ "$mode" = check-stamp ]; then
  [ -f "$stamp" ] || fail "there is no stamp, $stamp: run scripts/check.sh --fork on the owner's machine"
  differences=$(
    stamped_hashes | awk '
      # A sha256sum line is 64 hexadecimal digits, two spaces, then the path.
      NR == FNR { recorded[substr($0, 67)] = $1; next }
      {
        path = substr($0, 67)
        if (!(path in recorded)) print "new:     " path
        else if (recorded[path] != $1) print "changed: " path
        delete recorded[path]
      }
      END { for (path in recorded) print "gone:    " path }
    ' "$stamp" - | LC_ALL=C sort -k2
  )
  if [ -n "$differences" ]; then
    echo "fork-replay: the stamp, $stamp, does not match these files:" >&2
    echo "$differences" >&2
    echo "fork-replay: run scripts/check.sh --fork on the owner's machine, which replays them through the fork and writes the stamp again" >&2
    exit 1
  fi
  echo "fork-replay: the stamp matches the $(wc -l <"$stamp") files it covers"
  exit 0
fi

[ -d "$fork/apps/server" ] || fail "$fork is not a checkout of the fork: it has no apps/server"
runner="$fork/node_modules/.bin/vp"
[ -x "$runner" ] || fail "the fork has no test runner at $runner: install its dependencies first"
command -v cargo >/dev/null || fail "cargo is not on PATH: run this in nix develop"

harness="$repo/crates/pluto-recording/fork-harness"
target="${CARGO_TARGET_DIR:-$repo/target}"
work="$target/fork-harness"
mkdir -p "$work"

step() { printf '\n== %s\n' "$1"; }

# What was modified in the fork after the marker was: the run's writes.
fork_writes() {
  find "$fork" -newer "$marker" -not -path '*/.git/*' -print 2>/dev/null | head -n 20 || true
}

check_fork_untouched() {
  local written
  written=$(fork_writes)
  if [ -n "$written" ]; then
    echo "fork-replay: the run wrote in the fork:" >&2
    echo "$written" >&2
    exit 1
  fi
}

# However the run ends, a failure included, the fork is checked once more.
marker=$(mktemp "$work/fork-marker.XXXXXX")
on_exit() {
  local code=$? written
  set +e
  written=$(fork_writes)
  rm -f "$marker"
  if [ -n "$written" ]; then
    echo "fork-replay: the run wrote in the fork:" >&2
    echo "$written" >&2
    [ "$code" -ne 0 ] || code=1
  fi
  exit "$code"
}
trap on_exit EXIT

step "Building pluto-record and pluto-replay-agent"
CARGO_TARGET_DIR="$target" cargo build --quiet --locked -p pluto-recording

export PLUTO_FORK="$fork"
export PLUTO_HARNESS_CACHE="$work/vite-cache"
export PLUTO_RECORD_BIN="$target/debug/pluto-record"
export PLUTO_REPLAY_BIN="$target/debug/pluto-replay-agent"

# The fork's runner, from its server package, with the harness's configuration instead of the
# fork's: see crates/pluto-recording/fork-harness/vitest.config.mjs.
run_harness() {
  (cd "$fork/apps/server" &&
    "$runner" test run --config "$harness/vitest.config.mjs" --configLoader native \
      --reporter verbose --dir "$harness" "$@")
}

if [ "$mode" = record ]; then
  step "Recording $PLUTO_RECORD_AGENT/$PLUTO_RECORD_SCENARIO with the real agent"
  run_harness record.test.ts
  step "Checking that nothing was written in the fork"
  check_fork_untouched
  exit 0
fi

step "Self-test: the harness on its synthetic recordings"
run_harness self-test.test.ts

step "Replaying the committed recordings"
run_harness replay.test.ts

step "Checking that nothing was written in the fork"
check_fork_untouched

step "Writing the stamp"
# Written beside the build, outside the files it lists, then moved in place.
stamped_hashes >"$work/fork-replay.sha256.tmp"
mv "$work/fork-replay.sha256.tmp" "$stamp"

sdk=$(node -e '
  const sdk = require(process.argv[1]);
  console.log(`${sdk.name} ${sdk.version} (Claude Code ${sdk.claudeCodeVersion})`);
' "$fork/apps/server/node_modules/@anthropic-ai/claude-agent-sdk/package.json")
fork_commit=$(git -C "$fork" rev-parse HEAD)
# Without optional locks, git status does not refresh the fork's index: it writes nothing there.
if [ -n "$(git -C "$fork" --no-optional-locks status --porcelain)" ]; then
  fork_commit="$fork_commit (with uncommitted changes)"
fi
{
  echo "fork: $fork_commit"
  echo "sdk: $sdk"
  echo "date: $(date -u +%Y-%m-%d)"
} >"$work/fork-replay.meta.tmp"
mv "$work/fork-replay.meta.tmp" "$meta"
echo "wrote $stamp ($(wc -l <"$stamp") files) and $meta"

step "The fork replays passed"
