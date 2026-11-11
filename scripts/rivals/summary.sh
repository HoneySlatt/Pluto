#!/usr/bin/env bash
# Sums up the rivals' bench's runs of one scenario: for each subject, the median of its runs and
# their range, as a Markdown table for docs/measurements.md (scripts/rivals/README.md).
#
# usage: scripts/rivals/summary.sh <runs-folder> <scenario>
#   runs-folder  OUT/<date> of run.sh, which holds <subject>/<scenario>-<n>/
#
# Memory is in MiB, the headline figure (private + swap + GPU) of the program and of its agents;
# wake-ups a second; processor in cores (1.0 is one core busy throughout); frames, the subject's
# wl_surface commits a second while measured. Noisy runs are counted apart.
set -euo pipefail

folder=$1
scenario=$2

# The median and the range of numbers given one a line, as "median (min–max)".
median_range() {
    sort -g | awk '{ v[NR] = $1 } END {
        if (NR == 0) { print "–"; exit }
        m = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
        printf "%.4g (%.4g–%.4g)", m, v[1], v[NR]
    }'
}

figure() {
    local runs=("$@")
    local query=$QUERY
    for run in "${runs[@]}"; do jq -r "$query" "$run/figures.json"; done | median_range
}

frames_per_second() {
    local run
    for run in "$@"; do
        awk '{ print $2 / $4 }' "$run/frames.txt"
    done | median_range
}

echo "| Subject | Runs (noisy) | App memory, MiB | App wake-ups/s | App processor, cores | Agents' memory, MiB | Agents' wake-ups/s | Frames/s |"
echo "|---|---|---|---|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    runs=()
    noisy=0
    for run in "$subject_dir$scenario"-*; do
        [ -f "$run/figures.json" ] || continue
        runs+=("$run")
        grep -q '^noisy: yes' "$run/fingerprint.txt" && noisy=$((noisy + 1))
    done
    [ "${#runs[@]}" -gt 0 ] || continue
    printf '| %s | %d (%d) | %s | %s | %s | %s | %s | %s |\n' "$subject" "${#runs[@]}" "$noisy" \
        "$(QUERY='.app.memory.headline / 1048576' figure "${runs[@]}")" \
        "$(QUERY='.app.wakeups_per_second' figure "${runs[@]}")" \
        "$(QUERY='.app.cpu_seconds / .seconds' figure "${runs[@]}")" \
        "$(QUERY='.agents.memory.headline / 1048576' figure "${runs[@]}")" \
        "$(QUERY='.agents.wakeups_per_second' figure "${runs[@]}")" \
        "$(frames_per_second "${runs[@]}")"
done
