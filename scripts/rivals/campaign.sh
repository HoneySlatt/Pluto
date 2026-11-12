#!/usr/bin/env bash
# One subject's campaign ([ADR-0028](../../docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md)):
# its runs at rest and streaming, beside the anchors run once each, then its frames' own runs. A
# web rival's default deployment, when given, is measured at rest only, beside it. Run it as a user
# service, in `nix develop .#measure` (the README says how); it first waits until no other
# campaign of the bench runs, since two at once make each other noisy.
#
# usage: scripts/rivals/campaign.sh <subject> [<default-deployment>]
# env:   RUNS (3), ANCHORS ("chromium-blank pluto"), and run.sh's own: OUT, BUSY_LIMIT (0.6 here),
#        CALM_WAIT (60 here: a run waits up to an hour for the machine to rest)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

subject=$1
default=${2:-}
runs=${RUNS:-3}
read -r -a anchors <<< "${ANCHORS-chromium-blank pluto}"
export BUSY_LIMIT=${BUSY_LIMIT:-0.6} CALM_WAIT=${CALM_WAIT:-60}

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

echo "$(date -Iseconds) campaign of $subject${default:+ and $default}"
run() { "$here/run.sh" "$@" || echo "run.sh $* failed" >&2; }
run rest 1 "$subject" ${default:+"$default"} "${anchors[@]}"
[ "$runs" -le 1 ] || run rest "$((runs - 1))" "$subject" ${default:+"$default"}
run stream 1 "$subject" "${anchors[@]}"
[ "$runs" -le 1 ] || run stream "$((runs - 1))" "$subject"
FRAMES=1 MEASURE_SECONDS=120 run rest 1 "$subject" ${default:+"$default"}
FRAMES=1 run stream 1 "$subject"
echo "$(date -Iseconds) campaign of $subject done"
