#!/usr/bin/env bash
# The owner's recordings of real agents' sessions, run by hand: they spend the owner's quota, so
# agents never run this script for real (docs/adr/0025-agents-sessions-recorded-and-replayed.md).
# The procedure is in crates/pluto-recording/README.md.
#
#   scripts/record-session.sh login                  sign in once to Claude and Grok, in the
#                                                    disposable home
#   scripts/record-session.sh claude|grok SCENARIO   record one scenario; `all` records them all
#   scripts/record-session.sh accept                 move what was recorded into the recordings,
#                                                    then replay everything and write the stamp
#
# The disposable home is $PLUTO_RECORDING_HOME, by default ~/.local/state/pluto-recording/home:
# the agents run there, never in the real home, so the owner's own settings, skills, memory and
# account details stay out of the recordings. Recordings wait in target/recordings-staging until
# `accept`. PLUTO_FORK names the fork (default ~/Projects/pluto-server). Run it in `nix develop`;
# the agents' CLIs come from PATH.
#
# With PLUTO_RECORDING_DRY_RUN=1, `claude` and `grok` record stand-ins that play the harness's
# synthetic recordings instead of the real CLIs, in a throwaway home: the whole procedure runs,
# with no quota spent, so that it can be checked by anyone. `accept` then only checks what is
# staged, and moves nothing.
set -euo pipefail

# Resolved from where the caller stands, before the script moves to the repository.
home=$(realpath -m -- "${PLUTO_RECORDING_HOME:-$HOME/.local/state/pluto-recording/home}")
fork=$(realpath -m -- "${PLUTO_FORK:-$HOME/Projects/pluto-server}")
dry_run=${PLUTO_RECORDING_DRY_RUN:-0}

cd "$(dirname "$0")/.."
repo=$PWD
staging="$repo/target/recordings-staging"
# A dry run stages elsewhere, so that a later real `accept` never takes its recordings.
if [ "$dry_run" != 0 ]; then staging="$staging-dry-run"; fi
recordings="$repo/crates/pluto-recording/recordings"
target="${CARGO_TARGET_DIR:-$repo/target}"
claude_scenarios=(turn approval question interrupt resume)
grok_scenarios=(turn approval question interrupt resume wake)

usage() {
  sed -n '6,10p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

fail() {
  echo "record-session: $1" >&2
  exit 1
}

# The path of an agent's CLI. Its callers run it in `$(…)` and stop on its failure themselves.
agent_binary() {
  command -v "$1" || fail "$1 is not on PATH"
}

# An agent's CLI with only the disposable home and a minimal environment, as pluto-record runs it,
# in an empty folder of its own, so that it trusts and reads no project.
in_disposable_home() {
  local folder status=0
  folder=$(mktemp -d)
  (cd "$folder" && env -i PATH="$PATH" HOME="$home" LANG=C.UTF-8 \
    TERM="${TERM:-xterm-256color}" "$@") || status=$?
  rm -rf "$folder"
  return "$status"
}

login() {
  [ "$dry_run" = 0 ] || fail "login has no dry run: it is the owner's sign-in"
  scripts/fork-replay.sh --check-home "$home"
  local claude grok
  claude=$(agent_binary claude) || exit 1
  grok=$(agent_binary grok) || exit 1
  mkdir -p "$home"
  chmod 700 "$home"
  echo "Claude starts in the disposable home ($home)."
  echo "Answer its first questions, sign in (with /login if it does not ask), then leave with"
  echo "/exit. No message is needed."
  in_disposable_home "$claude"
  echo
  echo "Grok starts in the same home. Sign in as it asks, then leave it. No message is needed."
  in_disposable_home "$grok"
  echo
  echo "Done. Next: scripts/record-session.sh claude all, then grok all."
}

# A stand-in for an agent's CLI that plays the harness's synthetic recording of a scenario: the
# dry run's agent. The CLI runs in the scenario's project with the disposable home, which the fake
# agent puts back in place of its placeholders.
stand_in() {
  local agent=$1 scenario=$2 folder
  folder=$(mktemp -d -p "$dry_run_folder")
  {
    # Bash, since %q may quote in a form only Bash reads.
    echo '#!/usr/bin/env bash'
    printf 'exec %q --recording %q --state %q --workspace "$PWD" --home "$HOME" --timeout-ms 20000 -- "$@"\n' \
      "$target/debug/pluto-replay-agent" \
      "$repo/crates/pluto-recording/fork-harness/synthetic/$agent/$scenario.jsonl" "$folder/state"
  } >"$folder/$agent"
  chmod +x "$folder/$agent"
  echo "$folder/$agent"
}

record_one() {
  local agent=$1 scenario=$2 binary
  if [ "$dry_run" = 0 ]; then
    [ -d "$home" ] || fail "no disposable home yet: run scripts/record-session.sh login first"
    binary=$(agent_binary "$agent") || exit 1
    echo
    echo "== Recording $agent/$scenario (this spends a little of your $agent quota)"
  else
    binary=$(stand_in "$agent" "$scenario") || fail "cannot write the stand-in for $agent/$scenario"
    echo
    echo "== Recording $agent/$scenario, dry run: a stand-in plays the synthetic recording"
  fi
  PLUTO_RECORD_AGENT="$agent" PLUTO_RECORD_SCENARIO="$scenario" PLUTO_AGENT_BIN="$binary" \
    PLUTO_RECORD_HOME="$home" PLUTO_RECORD_OUT="$staging" \
    scripts/fork-replay.sh --fork "$fork" --record
  local file="$staging/$agent/$scenario.jsonl"
  echo
  echo "== Review of $agent/$scenario: read it, the values to read above all, before accepting"
  "$target/debug/pluto-record" review "$file"
  "$target/debug/pluto-record" check "$file" ||
    fail "the scan found something in $file: do not accept it; record it again or tell an agent"
}

record() {
  local agent=$1 scenario=$2
  local -n scenarios="${agent}_scenarios"
  if [ "$dry_run" != 0 ]; then
    # The throwaway home and the stand-ins, removed however the run ends.
    dry_run_folder=$(mktemp -d)
    trap 'rm -rf "$dry_run_folder"' EXIT
    home="$dry_run_folder/home"
    mkdir "$home"
  fi
  if [ "$scenario" = all ]; then
    for each in "${scenarios[@]}"; do
      record_one "$agent" "$each"
    done
  else
    [[ " ${scenarios[*]} " == *" $scenario "* ]] ||
      fail "$agent has no scenario $scenario; it has: ${scenarios[*]}"
    record_one "$agent" "$scenario"
  fi
}

# Every staged recording, with its reference beside it and a clean scan; the check comes before
# any move, so that a recording never reaches the recordings without its reference.
staged_recordings() {
  local file found=0
  for file in "$staging"/*/*.jsonl; do
    [ -e "$file" ] || continue
    [ -f "${file%.jsonl}.thread.json" ] ||
      fail "$file has no reference beside it: its recording did not finish; record it again"
    "$target/debug/pluto-record" check "$file" >&2 || fail "the scan found something in $file"
    echo "$file"
    found=$((found + 1))
  done
  [ "$found" -gt 0 ] || fail "nothing is waiting in $staging"
}

accept() {
  local files file agent
  files=$(staged_recordings) || exit 1
  if [ "$dry_run" != 0 ]; then
    echo "Dry run: these would be moved into $recordings, then replayed through the fork:"
    echo "$files"
    return
  fi
  while IFS= read -r file; do
    agent=$(basename "$(dirname "$file")")
    mkdir -p "$recordings/$agent"
    mv "$file" "${file%.jsonl}.thread.json" "$recordings/$agent/"
  done <<<"$files"
  echo "Moved $(wc -l <<<"$files") recordings. Replaying everything through the fork and writing the stamp:"
  scripts/fork-replay.sh --fork "$fork"
  echo
  echo "Done: commit crates/pluto-recording/recordings/, or ask an agent to."
}

case "${1:-}" in
  login) [ $# -eq 1 ] || usage; login ;;
  claude | grok) [ $# -eq 2 ] || usage; record "$1" "$2" ;;
  accept) [ $# -eq 1 ] || usage; accept ;;
  *) usage ;;
esac
