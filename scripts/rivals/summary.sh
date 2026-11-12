#!/usr/bin/env bash
# Sums up the rivals' bench's runs of one scenario: for each subject, the median of its calm runs
# and their range, as Markdown tables for docs/measurements.md (scripts/rivals/README.md).
#
# usage: scripts/rivals/summary.sh <campaign-folder> <scenario>
#   campaign-folder  OUT/<date>-<time> of run.sh, which holds <subject>/<scenario>-<n>/, or a
#                    folder that gathers the runs of several campaigns the same way
# env:   CONTROL (chromium-blank): the subject a web rival's figure is read less of
#
# Memory is in MiB: the headline figure (private + swap + GPU) of the program and of its agents,
# and the program's proportional figure (PSS) beside it; wake-ups a second; processor in cores
# (1.0 is one core busy throughout). The tables:
# - the whole subject, and its agents apart;
# - the start to the first window, in milliseconds, and the profile's growth over the run, in KiB;
# - each part of a subject split in parts (Pluto's client and server, a web rival's server and
#   browser), the whole being their sum;
# - a web rival (one with a `browser` part) less the control: its median less the control's, which
#   approaches its cost in a browser already open (ADR-0028);
# - the frames a second, from the runs that counted them (FRAMES=1).
# Only calm and valid runs count: a noisy run, one whose subject did not render on the GPU, asked
# for a browser, ended early, or whose workload was not the one measured (`valid: no`), is
# counted apart and left out. A run that counted frames gives only its frames: the compositor's log
# slows it. With KEEP_NOISY=1 noisy runs count too, for runs whose noise flag is known to be wrong
# (the README).
set -euo pipefail

folder=$1
scenario=$2
control=${CONTROL:-chromium-blank}

# The median and the range of numbers given one a line, as "median (min–max)".
median_range() {
    sort -g | awk '{ v[NR] = $1 } END {
        if (NR == 0) { print "–"; exit }
        m = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
        printf "%.4g (%.4g–%.4g)", m, v[1], v[NR]
    }'
}

# The median alone of numbers given one a line; empty without any.
median() {
    sort -g | awk '{ v[NR] = $1 } END {
        if (NR == 0) exit
        printf "%.6g", (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
    }'
}

# One figure of every run given, by a jq query on one of its figures files, one value a line.
values() {
    local file=$1 query=$2 run
    shift 2
    for run in "$@"; do jq -r "$query" "$run/$file"; done
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
        ! grep -q -E '^(browser|ended_early_at_ms):|^valid: no' "$fingerprint"
}

# Whether a run counted its frames, which slowed its compositor.
counted_frames() {
    grep -q '^frames_counted: 1' "$1/fingerprint.txt"
}

# A subject's calm runs of the scenario, into `runs`; those left out counted into `left_out`. With
# `frames`, the runs that counted frames, otherwise the others.
calm_runs() {
    local subject_dir=$1 kind=$2 run
    runs=()
    left_out=0
    for run in "$subject_dir$scenario"-*; do
        [ -f "$run/figures.json" ] || continue
        if [ "$kind" = frames ]; then counted_frames "$run" || continue; else ! counted_frames "$run" || continue; fi
        if is_calm "$run"; then runs+=("$run"); else left_out=$((left_out + 1)); fi
    done
}

# The four figures of a program, from one figures file of each run, as table cells.
program_cells() {
    local file=$1
    shift
    printf '%s | %s | %s | %s' \
        "$(values "$file" '.app.memory.headline / 1048576' "$@" | median_range)" \
        "$(values "$file" '.app.memory.pss / 1048576' "$@" | median_range)" \
        "$(values "$file" '.app.wakeups_per_second' "$@" | median_range)" \
        "$(values "$file" '.app.cpu_seconds / .seconds' "$@" | median_range)"
}

echo "| Subject | Calm runs (left out) | Its memory, MiB | Its PSS, MiB | Its wake-ups/s | Its processor, cores | Agents' memory, MiB | Agents' wake-ups/s |"
echo "|---|---|---|---|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    calm_runs "$subject_dir" resources
    [ "${#runs[@]}" -gt 0 ] || { [ "$left_out" = 0 ] || printf '| %s | 0 (%d) | | | | | | |\n' "$subject" "$left_out"; continue; }
    printf '| %s | %d (%d) | %s | %s | %s |\n' "$subject" "${#runs[@]}" "$left_out" \
        "$(program_cells figures.json "${runs[@]}")" \
        "$(values figures.json '.agents.memory.headline / 1048576' "${runs[@]}" | median_range)" \
        "$(values figures.json '.agents.wakeups_per_second' "${runs[@]}" | median_range)"
done

echo
echo "| Subject | Calm runs | Start to its first window, ms | Profile's growth, KiB |"
echo "|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    calm_runs "$subject_dir" resources
    [ "${#runs[@]}" -gt 0 ] || continue
    printf '| %s | %d | %s | %s |\n' "$subject" "${#runs[@]}" \
        "$(value start.txt first_window_ms "${runs[@]}")" \
        "$(for run in "${runs[@]}"; do sed -n 's/^profile_growth_bytes: \([-0-9]*\)$/\1/p' "$run/profile.txt" 2>/dev/null | awk '{ print $1 / 1024 }'; done | median_range)"
done

echo
echo "| Subject | Part | Calm runs | Its memory, MiB | Its PSS, MiB | Its wake-ups/s | Its processor, cores |"
echo "|---|---|---|---|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    calm_runs "$subject_dir" resources
    [ "${#runs[@]}" -gt 0 ] || continue
    for part_file in "${runs[0]}"/figures-*.json; do
        [ -f "$part_file" ] || continue
        part_file=$(basename "$part_file")
        part=${part_file#figures-}
        printf '| %s | %s | %d | %s |\n' "$subject" "${part%.json}" "${#runs[@]}" \
            "$(program_cells "$part_file" "${runs[@]}")"
    done
done

# The control's medians, for the web rivals' second reading.
calm_runs "$folder/$control/" resources
if [ "${#runs[@]}" -gt 0 ]; then
    control_runs=("${runs[@]}")
    queries=('.app.memory.headline / 1048576' '.app.memory.pss / 1048576' '.app.wakeups_per_second'
        '.app.cpu_seconds / .seconds')
    echo
    echo "| Subject less $control (${#control_runs[@]} runs) | Its memory, MiB | Its PSS, MiB | Its wake-ups/s | Its processor, cores |"
    echo "|---|---|---|---|---|"
    for subject_dir in "$folder"/*/; do
        subject=$(basename "$subject_dir")
        [ "$subject" != "$control" ] || continue
        calm_runs "$subject_dir" resources
        [ "${#runs[@]}" -gt 0 ] && [ -f "${runs[0]}/figures-browser.json" ] || continue
        cells=()
        for query in "${queries[@]}"; do
            cells+=("$(awk -v s="$(values figures.json "$query" "${runs[@]}" | median)" \
                -v c="$(values figures.json "$query" "${control_runs[@]}" | median)" \
                'BEGIN { printf "%.4g", s - c }')")
        done
        printf '| %s | %s | %s | %s | %s |\n' "$subject" "${cells[@]}"
    done
fi

echo
echo "| Subject | Runs that counted frames (left out) | Frames a second |"
echo "|---|---|---|"
for subject_dir in "$folder"/*/; do
    subject=$(basename "$subject_dir")
    calm_runs "$subject_dir" frames
    [ "${#runs[@]}" -gt 0 ] || [ "$left_out" -gt 0 ] || continue
    printf '| %s | %d (%d) | %s |\n' "$subject" "${#runs[@]}" "$left_out" \
        "$(for run in "${runs[@]}"; do sed -n 's/^frames: \([0-9]*\) in \([0-9.]*\) s$/\1 \2/p' "$run/frames.txt" | awk '{ print $1 / $2 }'; done | median_range)"
done
