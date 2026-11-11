#!/usr/bin/env bash
# Sums up the rivals' bench's runs of one scenario: for each subject, the median of its calm runs
# and their range, as a Markdown table for docs/measurements.md (scripts/rivals/README.md).
#
# usage: scripts/rivals/summary.sh <campaign-folder> <scenario>
#   campaign-folder  OUT/<date>-<time> of run.sh, which holds <subject>/<scenario>-<n>/
#
# Memory is in MiB: the headline figure (private + swap + GPU) of the program and of its agents,
# and the program's proportional figure (PSS) beside it; wake-ups a second; processor in cores
# (1.0 is one core busy throughout). A second table gives the start to the first window, in
# milliseconds, and the profile's growth over the run, in KiB. Only calm runs count: a noisy run, one whose subject did not
# render on the GPU, asked for a browser or ended early, is counted apart and left out. With
# KEEP_NOISY=1 noisy runs count too, for runs whose noise flag is known to be wrong (the README).
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

# One figure of every run given, by a jq query on its figures.json, as its median and range.
figure() {
    local query=$1 run
    shift
    for run in "$@"; do jq -r "$query" "$run/figures.json"; done | median_range
}

# One value of every run given, from a `key: value` line of one of its files, as its median and
# range; a run without it is skipped.
value() {
    local file=$1 key=$2 run
    shift 2
    for run in "$@"; do sed -n "s/^$key: \([-0-9.]*\)$/\1/p" "$run/$file" 2>/dev/null; done | median_range
}

# Whether a run's figures may be kept.
is_calm() {
    local fingerprint=$1/fingerprint.txt
    { [ "${KEEP_NOISY:-0}" = 1 ] || grep -q '^noisy: no' "$fingerprint"; } &&
        grep -q '^gpu: used' "$fingerprint" &&
        ! grep -q -E '^(browser|ended_early_at_ms):' "$fingerprint"
}

echo "| Subject | Calm runs (left out) | Its memory, MiB | Its PSS, MiB | Its wake-ups/s | Its processor, cores | Agents' memory, MiB | Agents' wake-ups/s |"
echo "|---|---|---|---|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    runs=()
    left_out=0
    for run in "$subject_dir$scenario"-*; do
        [ -f "$run/figures.json" ] || continue
        if is_calm "$run"; then runs+=("$run"); else left_out=$((left_out + 1)); fi
    done
    [ "${#runs[@]}" -gt 0 ] || { printf '| %s | 0 (%d) | | | | | | |\n' "$subject" "$left_out"; continue; }
    printf '| %s | %d (%d) | %s | %s | %s | %s | %s | %s |\n' "$subject" "${#runs[@]}" "$left_out" \
        "$(figure '.app.memory.headline / 1048576' "${runs[@]}")" \
        "$(figure '.app.memory.pss / 1048576' "${runs[@]}")" \
        "$(figure '.app.wakeups_per_second' "${runs[@]}")" \
        "$(figure '.app.cpu_seconds / .seconds' "${runs[@]}")" \
        "$(figure '.agents.memory.headline / 1048576' "${runs[@]}")" \
        "$(figure '.agents.wakeups_per_second' "${runs[@]}")"
done

echo
echo "| Subject | Calm runs | Start to its first window, ms | Profile's growth, KiB |"
echo "|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    runs=()
    for run in "$subject_dir$scenario"-*; do
        [ -f "$run/figures.json" ] && is_calm "$run" && runs+=("$run")
    done
    [ "${#runs[@]}" -gt 0 ] || continue
    printf '| %s | %d | %s | %s |\n' "$subject" "${#runs[@]}" \
        "$(value start.txt first_window_ms "${runs[@]}")" \
        "$(for run in "${runs[@]}"; do sed -n 's/^profile_growth_bytes: \([-0-9]*\)$/\1/p' "$run/profile.txt" 2>/dev/null | awk '{ print $1 / 1024 }'; done | median_range)"
done
