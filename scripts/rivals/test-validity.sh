#!/usr/bin/env bash
# Tests validity.sh, the rivals' bench's checks that a run's workload was the one measured, on run
# folders made up here: each case's verdict must be the expected one. scripts/check.sh runs it.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=validity.sh
source "$here/validity.sh"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

# A begin line and an end line of the fake model's journal, as it writes them (the Rust test
# `the_journal_lines_keep_their_text` keeps their text).
begin_line() { printf '{"begin":%s,"at_ms":1,"api":"anthropic","turn":%s,"segment":%s}\n' "$1" "$2" "$3"; }
end_line() {
    printf '{"request":%s,"at_ms":1,"method":"POST","path":"/v1/messages","api":"anthropic","kind":"main","status":200,"turn":%s,"segment":%s,"ended_ms":2}\n' "$1" "$2" "$3"
}

# A run folder: its windows at both ends, and its journal, cut at `ready` after its first
# `ready_lines` lines.
make_run() {
    local dir=$work/$1 windows_ready=$2 windows_done=$3 ready_lines=$4
    mkdir -p "$dir"
    echo "$windows_ready" > "$dir/windows-ready.txt"
    echo "$windows_done" > "$dir/windows-done.txt"
    cat > "$dir/fake.jsonl"
    head -n "$ready_lines" "$dir/fake.jsonl" | wc -c > "$dir/journal-ready.offset"
    wc -c < "$dir/fake.jsonl" > "$dir/journal-done.offset"
    echo "$dir"
}

# Threads begun at their first segment (fifteen, or as many as given), each of whose answers then
# began at the second.
streams() {
    local n
    for n in $(seq "${1:-15}"); do begin_line "$n" 3 0; end_line "$n" 3 0; begin_line "$((n + 15))" 3 1; done
}

# Fifteen short turns, all ended.
short_turns() {
    local n
    for n in $(seq 15); do begin_line "$n" $((n % 3)) 0; end_line "$n" $((n % 3)) 0; done
}

expect() {
    local name=$1 verdict=$2 got
    shift 2
    got=$(validity "$@" | tail -n 1)
    if [ "$got" != "valid: $verdict" ]; then
        echo "FAIL $name: $got, expected valid: $verdict"
        validity "$@" | sed 's/^/    /'
        failures=$((failures + 1))
    fi
}

# Streaming: the fifteen in flight at both ends, none ended between.
dir=$(streams | make_run stream-valid 1 1 45)
expect "15 streams in flight" yes "$dir" stream 15
# One answer ended while it was measured.
dir=$({ streams; end_line 16 3 1; } | make_run stream-ended 1 1 45)
expect "a stream ended between" no "$dir" stream 15
# Only fourteen began.
dir=$(streams 14 | make_run stream-fourteen 1 1 42)
expect "14 streams in flight" no "$dir" stream 15
# Two windows at the end: a bookmark manager, a dialog.
dir=$(streams | make_run stream-two-windows 1 2 45)
expect "a second window" no "$dir" stream 15
# No journal at all.
dir=$(make_run stream-no-journal 1 1 0 < /dev/null)
expect "no journal" no "$dir" stream 15
# The control streams nothing.
dir=$(make_run control 1 1 0 < /dev/null)
expect "the control, none expected" yes "$dir" stream 0
# A thread opened in the window (`subject_show`): the screen changed wholly.
dir=$(streams | make_run stream-shown 1 1 45)
head -c 1000 /dev/zero > "$dir/screen-before-show.ppm"
head -c 1000 /dev/zero | tr '\0' 'x' > "$dir/screen-ready.ppm"
expect "a thread opened" yes "$dir" stream 15
# The opening missed: the screen stayed, or a caret blinked.
cp "$dir/screen-before-show.ppm" "$dir/screen-ready.ppm"
expect "an opening that missed" no "$dir" stream 15
head -c 995 /dev/zero > "$dir/screen-ready.ppm"
printf 'xxxxx' >> "$dir/screen-ready.ppm"
expect "a caret's change" no "$dir" stream 15
# No screenshot when ready.
rm "$dir/screen-ready.ppm"
expect "no screenshot when ready" no "$dir" stream 15
# A part without a process: a server that died, an Xwayland not moved.
dir=$(streams | make_run stream-part 1 1 45)
printf 'app:12\nxwayland:0\n' > "$dir/parts-ready.txt"
expect "a part without a process" no "$dir" stream 15
printf 'app:12\nxwayland:1\n' > "$dir/parts-ready.txt"
expect "every part with a process" yes "$dir" stream 15

# At rest: exactly fifteen threads begun by the time it is ready.
dir=$(short_turns | make_run rest-valid 1 1 30)
expect "15 threads at rest" yes "$dir" rest 15
dir=$({ short_turns; begin_line 16 0 0; end_line 16 0 0; } | make_run rest-sixteen 1 1 32)
expect "a prompt typed twice" no "$dir" rest 15
dir=$(short_turns | make_run rest-no-window 0 1 30)
expect "no window when ready" no "$dir" rest 15

if [ "$failures" -gt 0 ]; then
    echo "test-validity: $failures case(s) failed" >&2
    exit 1
fi
echo "test-validity: every case gave its verdict"
