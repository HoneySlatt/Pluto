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

# How much the screen changed when the subject's file opened a streaming thread
# (`subject_show`), in percent of the bytes of its screenshots before and when ready (lib.sh); 0
# without them.
show_change() {
    local size changed
    size=$(stat -c %s "$1/screen-before-show.ppm" 2>/dev/null || echo 0)
    [ "$size" -gt 0 ] && [ -s "$1/screen-ready.ppm" ] || { echo 0; return; }
    changed=$(cmp -l "$1/screen-before-show.ppm" "$1/screen-ready.ppm" 2>/dev/null | wc -l)
    awk -v c="$changed" -v s="$size" 'BEGIN { printf "%.2f", 100 * c / s }'
}

# The least share of the screen that opening a thread changes, in percent: a thread replaces a
# page; a click or a key that missed changes a caret or a clock.
SHOW_CHANGE_MIN=${SHOW_CHANGE_MIN:-1}

# The checks, as fingerprint lines, the last `valid: yes` or `valid: no`. A run is valid when:
# - the subject showed exactly one window when it was ready and when the measure ended;
# - each of its parts held a process when it was ready (a server that died, an Xwayland not moved);
# - at rest, exactly `expected` threads had begun by then: none lost, none typed twice;
# - streaming, `expected` answers were in flight at both ends and none ended between them, so that
#   they all streamed for the whole measure;
# - streaming, where the subject's file opens a thread in its window (`subject_show`), the screen
#   changed by at least SHOW_CHANGE_MIN percent when it did: the thread was opened. Some agents
#   give their text only once a segment ends (Claude Code's), so a thread shown may change little
#   while it is measured; what is checked is that it was opened.
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
        if [ -e "$dir/screen-before-show.ppm" ]; then
            at_ready=$(show_change "$dir")
            echo "shown: the screen changed by $at_ready% (at least $SHOW_CHANGE_MIN%)"
            awk -v c="$at_ready" -v m="$SHOW_CHANGE_MIN" 'BEGIN { exit !(c >= m) }' || valid=no
        fi
        ;;
    esac
    echo "valid: $valid"
}
