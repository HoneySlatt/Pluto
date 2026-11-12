# shellcheck shell=bash
# Whether a run's workload was the one measured (ADR-0028): read from the files the sandbox leaves
# at both ends of the measure (lib.sh's `at_bounds`), and the fake model's journal. Sourced by
# run.sh; tested by test-validity.sh.

# The fake model's journal as it stood at one end of the measure (`ready` or `done`).
journal_at() {
    head -c "$(cat "$1/journal-$2.offset" 2>/dev/null || echo 0)" "$1/fake.jsonl" 2>/dev/null
}

# Main requests begun, and ended, by one end of the measure; and the threads begun: the main
# requests at their turn's first segment.
main_begun() { journal_at "$1" "$2" | grep -c '^{"begin":' || true; }
main_ended() { journal_at "$1" "$2" | grep -c '"kind":"main"' || true; }
threads_begun() { journal_at "$1" "$2" | grep -c '^{"begin":.*"segment":0}$' || true; }

# How much of the screen changed between both ends of the measure, in percent of the bytes of its
# screenshots (lib.sh's `at_bounds`); 0 without them.
screen_change() {
    local size changed
    size=$(stat -c %s "$1/screen-ready.ppm" 2>/dev/null || echo 0)
    [ "$size" -gt 0 ] && [ -s "$1/screen-done.ppm" ] || { echo 0; return; }
    changed=$(cmp -l "$1/screen-ready.ppm" "$1/screen-done.ppm" 2>/dev/null | wc -l)
    awk -v c="$changed" -v s="$size" 'BEGIN { printf "%.2f", 100 * c / s }'
}

# The least share of the screen a streaming thread changes over the measure, in percent.
SCREEN_CHANGE_MIN=${SCREEN_CHANGE_MIN:-1}

# The checks, as fingerprint lines, the last `valid: yes` or `valid: no`. A run is valid when:
# - the subject showed exactly one window when it was ready and when the measure ended;
# - each of its parts held a process when it was ready (a server that died, an Xwayland not moved);
# - at rest, exactly `expected` threads had begun by then: none lost, none typed twice;
# - streaming, `expected` answers were in flight at both ends and none ended between them, so that
#   they all streamed for the whole measure; and at least SCREEN_CHANGE_MIN percent of the screen
#   changed, as a thread shown streaming changes it.
# A run that fails is left out, as a noisy one.
# usage: validity <run-folder> <scenario> <expected threads>
validity() {
    local dir=$1 scenario=$2 expected=$3 valid=yes at_ready at_end
    at_ready=$(cat "$dir/windows-ready.txt" 2>/dev/null || echo unknown)
    at_end=$(cat "$dir/windows-done.txt" 2>/dev/null || echo unknown)
    echo "windows: $at_ready at ready, $at_end at the end"
    { [ "$at_ready" = 1 ] && [ "$at_end" = 1 ]; } || valid=no
    if [ -s "$dir/parts-ready.txt" ]; then
        echo "parts: $(paste -sd ' ' "$dir/parts-ready.txt")"
        ! grep -q ':0$' "$dir/parts-ready.txt" || valid=no
    fi
    case "$scenario" in
    rest)
        at_ready=$(threads_begun "$dir" ready)
        echo "threads_begun: $at_ready"
        [ "$at_ready" = "$expected" ] || valid=no
        ;;
    stream)
        local ended
        at_ready=$(($(main_begun "$dir" ready) - $(main_ended "$dir" ready)))
        at_end=$(($(main_begun "$dir" "done") - $(main_ended "$dir" "done")))
        ended=$(($(main_ended "$dir" "done") - $(main_ended "$dir" ready)))
        echo "streams_in_flight: $at_ready at ready, $at_end at the end, $ended ended between"
        { [ "$at_ready" = "$expected" ] && [ "$at_end" = "$expected" ] && [ "$ended" = 0 ]; } || valid=no
        # A thread shown while it streams changes the screen: an empty page, a list, a dialog
        # barely do (a caret, a clock).
        if [ "$expected" -gt 0 ]; then
            at_ready=$(screen_change "$dir")
            echo "screen_changed: $at_ready% of its bytes (at least $SCREEN_CHANGE_MIN%)"
            awk -v c="$at_ready" -v m="$SCREEN_CHANGE_MIN" 'BEGIN { exit !(c >= m) }' || valid=no
        fi
        ;;
    esac
    echo "valid: $valid"
}
