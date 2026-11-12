#!/usr/bin/env bash
# One subject's campaign ([ADR-0028](../../docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md)):
# runs at rest and streaming until it has RUNS of each kept (calm and valid, validity.sh's
# `run_kept`), beside the anchors run once each, then one kept run of each scenario counting its
# frames. A web rival's default deployment, when given, is measured at rest only, beside it. The
# runs already kept under OUT, by any campaign, count. Run it as a user service, in
# `nix develop .#measure` (the README says how); it first waits until no other campaign of the
# bench runs, since two at once make each other noisy.
#
# usage: scripts/rivals/campaign.sh <subject> [<default-deployment>]
# env:   RUNS (3), TRIES (8: the runs it makes at most for one count, kept or not),
#        ANCHORS ("chromium-blank pluto"), and run.sh's own: OUT (/tmp/pluto-rivals),
#        BUSY_LIMIT (0.6 here), CALM_WAIT (60 here: a run waits up to an hour for the machine to rest)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=validity.sh
source "$here/validity.sh"

subject=$1
default=${2:-}
runs=${RUNS:-3}
tries=${TRIES:-8}
read -r -a anchors <<< "${ANCHORS-chromium-blank pluto}"
out=${OUT:-/tmp/pluto-rivals}
export OUT=$out BUSY_LIMIT=${BUSY_LIMIT:-0.6} CALM_WAIT=${CALM_WAIT:-60}

# The bench's other units: another campaign's service, or a run's scope. This campaign's own
# service is left out; its runs' scopes exist only while run.sh runs.
own_unit=$(sed -n 's|^0::.*/\([^/]*\.service\)$|\1|p' /proc/self/cgroup)
others() {
    systemctl --user list-units --plain --no-legend --state=active 'pluto-rivals-*' |
        awk -v own="$own_unit" '$1 != own { print $1 }'
}
while [ -n "$(others)" ]; do
    echo "$(date -Iseconds) waiting for $(others | paste -sd ' ')"
    sleep 60
done

run() { "$here/run.sh" "$@" || echo "run.sh $* failed" >&2; }

# The runs of a subject and scenario kept under OUT: those that counted frames with `frames`, the
# others otherwise.
kept() {
    local who=$1 scenario=$2 kind=$3 count=0 dir
    for dir in "$out"/*/"$who"/"$scenario"-*/; do
        [ -f "$dir/fingerprint.txt" ] || continue
        if [ "$kind" = frames ]; then counted_frames "$dir" || continue; else ! counted_frames "$dir" || continue; fi
        run_kept "$dir" && count=$((count + 1))
    done
    echo "$count"
}

# Runs one subject in one scenario until `wanted` runs are kept, or `tries` were made.
run_until() {
    local wanted=$1 scenario=$2 who=$3 kind=$4 made=0
    while [ "$(kept "$who" "$scenario" "$kind")" -lt "$wanted" ] && [ "$made" -lt "$tries" ]; do
        run "$scenario" 1 "$who"
        made=$((made + 1))
    done
    echo "$(date -Iseconds) $who $scenario${kind:+ ($kind)}: $(kept "$who" "$scenario" "$kind") kept of $wanted, $made run"
}

echo "$(date -Iseconds) campaign of $subject${default:+ and $default}"
run rest 1 "$subject" ${default:+"$default"} "${anchors[@]}"
run_until "$runs" rest "$subject" resources
[ -z "$default" ] || run_until "$runs" rest "$default" resources
run stream 1 "$subject" "${anchors[@]}"
run_until "$runs" stream "$subject" resources
FRAMES=1 MEASURE_SECONDS=120 run_until 1 rest "$subject" frames
[ -z "$default" ] || FRAMES=1 MEASURE_SECONDS=120 run_until 1 rest "$default" frames
FRAMES=1 run_until 1 stream "$subject" frames
echo "$(date -Iseconds) campaign of $subject done"
