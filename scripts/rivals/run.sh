#!/usr/bin/env bash
# The rivals' bench (docs/adr/0026-the-rivals-measured-how.md): runs subjects through a scenario,
# interleaved, and keeps each run's figures. Run it in `nix develop .#measure`, with the client,
# the fake model, pluto-measure and the driver built in release (the README says how).
#
# usage: scripts/rivals/run.sh <scenario> <runs> <subject>...
#   scenario  rest: 15 threads run their turn, then the subject rests while it is measured;
#             stream: 15 threads stream a long answer at once, measured while they stream.
# env:       MEASURE_SECONDS (rest: 600, stream: 45), OUT (/tmp/pluto-rivals), BUSY_LIMIT (0.30 of
#            a core used by the rest of the machine, above which a run waits, then is marked noisy),
#            FRAMES=1 to count the subject's frames (those runs' resources are not to be kept)
#
# Each run's folder, OUT/<date>-<time>/<subject>/<scenario>-<n>/, holds figures.json
# (pluto-measure's), frames.txt, fingerprint.txt, the fake model's journal and every log; a new
# call of run.sh never overwrites an earlier one's.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

scenario=$1
runs=$2
shift 2
subjects=("$@")
case "$scenario" in
rest) seconds=${MEASURE_SECONDS:-600} ;;
stream) seconds=${MEASURE_SECONDS:-45} ;;
probe) seconds=${MEASURE_SECONDS:-5} ;;
*) echo "unknown scenario $scenario" >&2; exit 2 ;;
esac
out=${OUT:-/tmp/pluto-rivals}/$(date +%Y-%m-%d-%H%M%S)
busy_limit=${BUSY_LIMIT:-0.30}

export PLUTO_FAKE_MODEL=$repo/target/release/pluto-fake-model
export PLUTO_FAKE_SCRIPT=$repo/crates/pluto-fake-model/scripts/workload.json
measure=$repo/target/release/pluto-measure
for binary in "$PLUTO_FAKE_MODEL" "$measure" "$repo/target/release/pluto" "$repo/target/release/examples/drive"; do
    [ -x "$binary" ] || { echo "build $binary first (scripts/rivals/README.md)" >&2; exit 1; }
done

# Processor time the whole machine spent, in seconds, from /proc/stat (USER_HZ ticks).
machine_busy() {
    awk '/^cpu / { print ($2 + $3 + $4 + $7 + $8) / 100 }' /proc/stat
}

# Waits up to five minutes for the machine to be calm: less than busy_limit of a core over 5 s.
wait_calm() {
    local tries=60 before after
    while [ "$tries" -gt 0 ]; do
        before=$(machine_busy)
        sleep 5
        after=$(machine_busy)
        if awk -v b="$before" -v a="$after" -v l="$busy_limit" 'BEGIN { exit !((a - b) / 5 < l) }'; then
            return 0
        fi
        tries=$((tries - 1))
    done
    echo "the machine stayed busy; running anyway, marked noisy" >&2
    return 1
}

# A program's package as its store path names it (claude-code-2.1.311): never by running it.
store_name() {
    basename "$(dirname "$(dirname "$(readlink -f "$(command -v "$1")")")")" | cut -d- -f2-
}

fingerprint() {
    local subject=$1
    {
        echo "date: $(date -Iseconds)"
        echo "subject: $subject"
        echo "version: $(bash -c "repo=$repo; source $here/subjects/$subject.sh; subject_version")"
        echo "kernel: $(uname -r)"
        echo "mesa: $(readlink -f /run/opengl-driver/lib/dri/radeonsi_dri.so | cut -d/ -f4 | cut -d- -f2-)"
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null) $(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference 2>/dev/null)"
        echo "compositor: sway $(sway --version | awk '{print $3}') headless 3840x2160@240Hz gles2"
        echo "script: $(sha256sum "$PLUTO_FAKE_SCRIPT" | cut -c1-16)"
        echo "bench: $(git -C "$repo" rev-parse --short HEAD)$(git -C "$repo" diff --quiet HEAD -- scripts/rivals crates || echo -dirty)"
        echo "client: sha256 $(sha256sum "$repo/target/release/pluto" | cut -c1-16)"
        echo "agents: $(store_name claude); $(store_name codex)"
        echo "busy_limit: $busy_limit"
        echo "frames_counted: ${FRAMES:-0}"
    }
}

# Counts the subject's frames between two offsets of the compositor's log: its wl_surface commits.
frames() {
    local log=$1 from=$2 to=$3
    tail -c +"$((from + 1))" "$log" | head -c "$((to - from))" | grep -c 'wl_surface#[0-9]*\.commit()' || true
}

# Stops what is left of a run: the bus, the compositor and the fake model outlive the sandbox's
# script, and the scope with them.
stop_scope() {
    systemctl --user stop "$1.scope" 2>/dev/null || true
}

one_run() {
    local unit=pluto-rivals-$$-$1-$2
    # An interrupted run stops its scope, rather than leave the subject running for hours.
    trap 'stop_scope "$unit"; exit 130' INT TERM
    local subject=$1 n=$2
    local dir=$out/$subject/$scenario-$n
    rm -rf "$dir"
    mkdir -p "$dir"
    local noisy=no
    wait_calm || noisy=yes
    fingerprint "$subject" > "$dir/fingerprint.txt"
    systemd-run --user --scope --quiet --collect -p Delegate=yes -p RuntimeMaxSec=7200 --unit="$unit" -- \
        "$here/sandbox.sh" netns "$dir" "$subject" "$scenario" > "$dir/sandbox.log" 2>&1 &
    local sandbox=$!
    local waited=0
    while [ ! -e "$dir/ready" ]; do
        kill -0 "$sandbox" 2>/dev/null || { echo "$subject run $n failed; see $dir/sandbox.log" >&2; stop_scope "$unit"; return 1; }
        sleep 1
        waited=$((waited + 1))
        [ "$waited" -lt 1800 ] || { echo "$subject run $n never got ready" >&2; stop_scope "$unit"; return 1; }
    done
    local cgroup
    cgroup=/sys/fs/cgroup$(systemctl --user show -p ControlGroup --value "$unit.scope")/subject
    local agents=()
    read -r -a names < <(bash -c "repo=$repo; source $here/subjects/$subject.sh; subject_agents")
    for name in "${names[@]}"; do agents+=(--agent "$name"); done
    local before after
    before=$(machine_busy)
    local part parts=() part_measures=()
    read -r -a parts < <(bash -c "repo=$repo; source $here/subjects/$subject.sh; subject_parts")
    for part in "${parts[@]}"; do
        "$measure" --cgroup "$cgroup/$part" --seconds "$seconds" "${agents[@]}" \
            --out "$dir/figures-$part.json" 2> "$dir/measure-$part.log" &
        part_measures+=($!)
    done
    "$measure" --cgroup "$cgroup" --seconds "$seconds" "${agents[@]}" --out "$dir/figures.json" 2> "$dir/measure.log"
    # Only the parts' measures: a bare `wait` would also wait for the sandbox, which waits for `done`.
    for part in "${part_measures[@]}"; do wait "$part" || true; done
    after=$(machine_busy)
    touch "$dir/done"
    wait "$sandbox" || true
    stop_scope "$unit"
    local subject_cpu
    subject_cpu=$(jq '.cpu_seconds' "$dir/figures.json")
    # What the rest of the machine did meanwhile: the bench's compositor and fake model count here,
    # since they serve the subject; a figure above the limit marks the run noisy.
    awk -v b="$before" -v a="$after" -v s="$subject_cpu" -v t="$seconds" -v l="$busy_limit" \
        'BEGIN { other = (a - b - s) / t; printf "other_cores: %.3f\n", other; exit !(other < l) }' \
        >> "$dir/fingerprint.txt" || noisy=yes
    echo "noisy: $noisy" >> "$dir/fingerprint.txt"
    # A subject that died while it was measured leaves figures that look whole: say it.
    local ended
    ended=$(jq -r '.ended_early_at_ms // empty' "$dir/figures.json")
    if [ -n "$ended" ]; then
        echo "ended_early_at_ms: $ended" >> "$dir/fingerprint.txt"
        echo "$subject run $n: the subject ended after $ended ms of $seconds s; see $dir" >&2
    fi
    # The subject must draw on the GPU, never in software: its DRM clients' graphics engine ran.
    if [ "$(jq '.app.memory.gpu_engine_ns' "$dir/figures.json")" -gt 0 ]; then
        echo "gpu: used" >> "$dir/fingerprint.txt"
    else
        echo "gpu: not used" >> "$dir/fingerprint.txt"
        echo "$subject run $n: the subject did not render on the GPU; see $dir" >&2
    fi
    if [ -s "$dir/browser.log" ]; then
        echo "browser: asked" >> "$dir/fingerprint.txt"
        echo "$subject run $n: the subject asked for a browser; see $dir/browser.log" >&2
    fi
    if [ "${FRAMES:-0}" = 1 ]; then
        echo "frames: $(frames "$dir/compositor.log" "$(cat "$dir/ready")" "$(cat "$dir/done.offset")") in $seconds s" > "$dir/frames.txt"
    else
        echo "frames: not counted" > "$dir/frames.txt"
    fi
    echo "$subject $scenario $n: $(cat "$dir/measure.log"); $(cat "$dir/frames.txt"); noisy $noisy"
}

for n in $(seq "$runs"); do
    for subject in "${subjects[@]}"; do
        one_run "$subject" "$n" || true
    done
done
