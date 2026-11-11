# shellcheck shell=bash
# What a run does inside its sandbox, sourced by sandbox.sh after the subject's file, whose
# functions the README's "A subject's file" lists.

# The fake model's address inside the sandbox: its own loopback, which no other program shares.
FAKE_PORT=11435
FAKE_URL=http://127.0.0.1:$FAKE_PORT
export FAKE_PORT FAKE_URL

# Waits until `test` succeeds, checking every half second, for at most `seconds`.
wait_for() {
    local seconds=$1
    shift
    local tries=$((seconds * 2))
    while ! "$@"; do
        tries=$((tries - 1))
        [ "$tries" -gt 0 ] || return 1
        sleep 0.5
    done
}

# Whether a server listens on a port of the sandbox's loopback.
listening() {
    (exec 3<> "/dev/tcp/127.0.0.1/$1") 2> /dev/null
}

# How many model requests the fake model has journaled for a turn's segment (main requests only).
journaled() {
    local turn=$1 segment=$2
    grep -c "\"kind\":\"main\".*\"turn\":$turn,\"segment\":$segment," "$run_dir/fake.jsonl" 2>/dev/null || true
}

# At least `count` main requests journaled, over the turns given, at their last segment.
finished() {
    local count=$1 segment=$2
    shift 2
    local total=0 turn
    for turn in "$@"; do total=$((total + $(journaled "$turn" "$segment"))); done
    [ "$total" -ge "$count" ]
}

# Clicks at a point of the screen, in pixels of the 3840 × 2160 output: the virtual pointer goes
# to the top left corner first, then moves there, rests half a second (buttons shown on hover
# ignore an instant click), and clicks. Each `wlrctl` call is a new virtual pointer, and sway may
# handle its click before the window under it has the pointer's focus: the click can be lost.
# Drivers use the keyboard; the click serves `probe` only.
click() {
    wlrctl pointer move -4000 -4000
    wlrctl pointer move "$1" "$2"
    sleep 0.5
    wlrctl pointer click left
}

start_bus() {
    dbus-daemon --session --fork --address=unix:path=/tmp/run/bus --print-pid=3 3>/tmp/run/bus.pid
    export DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/run/bus
}

# The headless sway every subject draws into: 3840 × 2160 at 240 Hz on the GPU, one output,
# no input device. With FRAMES=1 it logs every client request (WAYLAND_DEBUG=server), which counts
# the subject's frames; the log slows the compositor, so runs that count frames are apart from
# those whose resources are kept (ADR-0026).
start_compositor() {
    printf 'output HEADLESS-1 mode 3840x2160@240Hz\ndefault_border none\n' > /tmp/run/sway.conf
    local debug=
    [ "${FRAMES:-0}" = 1 ] && debug=server
    (
        in_part compositor
        WAYLAND_DEBUG=$debug WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 \
            exec sway -c /tmp/run/sway.conf
    ) > "$run_dir/compositor.log" 2>&1 &
    wait_for 10 test -S /tmp/run/wayland-1 || { echo "sway did not start" >&2; return 1; }
    export WAYLAND_DISPLAY=wayland-1
    local socket
    for socket in /tmp/run/sway-ipc.*.sock; do export SWAYSOCK=$socket; done
}

start_fake() {
    (
        in_part fake
        exec "$PLUTO_FAKE_MODEL" --script "$PLUTO_FAKE_SCRIPT" --listen "127.0.0.1:$FAKE_PORT" \
            --listen "[::1]:$FAKE_PORT" --journal "$run_dir/fake.jsonl" --speed "${FAKE_SPEED:-1}"
    ) 2> "$run_dir/fake.log" &
    wait_for 10 grep -q "listening on http://127.0.0.1" "$run_dir/fake.log" \
        || { echo "the fake model did not start" >&2; return 1; }
}

# The compositor's window events, one JSON object a line, each stamped in milliseconds of the
# realtime clock: the first window it maps (`"change": "new"`) is the subject's first frame on
# screen, which run.sh compares with `started_ms`.
watch_windows() {
    stdbuf -oL swaymsg -t subscribe -m '["window"]' 2>/dev/null |
        jq -c --unbuffered '{at_ms: (now * 1000 | floor), change, app_id: .container.app_id}' \
            > "$run_dir/windows.jsonl" &
}

# The subject's profile on disk, in bytes: its whole home but the project the agents work in.
profile_bytes() {
    du -sb --exclude="$HOME/project" "$HOME" | cut -f1
}

# The frozen profile of a subject, prepared once by the `prepare` scenario (the README says how).
frozen_profile() {
    echo "$repo/target/rivals/profiles/$subject/home.tar"
}

# The subject's home for a run: its frozen profile, or, for `prepare` and `probe`, its setup.
make_home() {
    local scenario=$1 profile
    profile=$(frozen_profile)
    case "$scenario" in
    prepare) subject_setup ;;
    probe) if [ -f "$profile" ]; then tar -xf "$profile" -C "$HOME"; else subject_setup; fi ;;
    *) tar -xf "$profile" -C "$HOME" ;;
    esac
    # The bench's own token to a subject's server is issued again by each run, which its driver
    # waits for.
    rm -f "$HOME/token"
}

# Stops the subject and waits, up to 20 seconds, until none of its processes is left, so that what
# it writes on its way out is in its profile.
stop_and_wait() {
    subject_stop
    wait_for 20 grep -q '^populated 0' "$cg/subject/cgroup.events" ||
        echo "the subject was still running 20 s after it was asked to stop" >&2
}

# The phase files run.sh reads: `ready` once the subject is in the state to measure, holding
# where the compositor's log stood; run.sh writes `done` when it has measured.
mark_ready() {
    stat -c %s "$run_dir/compositor.log" > "$run_dir/ready.tmp"
    mv "$run_dir/ready.tmp" "$run_dir/ready"
}

# No browser may start in the sandbox: a link a subject opens goes nowhere, and is noted.
block_browsers() {
    mkdir -p /tmp/run/bin
    printf '#!/bin/sh\necho "a browser was asked for: $*" >> %q\n' "$run_dir/browser.log" \
        > /tmp/run/bin/xdg-open
    chmod +x /tmp/run/bin/xdg-open
    export PATH=/tmp/run/bin:$PATH
}

bench_inside() {
    run_dir=$1
    local scenario=$3
    in_part bench
    block_browsers
    start_bus
    start_compositor
    watch_windows
    start_fake
    make_home "$scenario"
    profile_bytes > "$run_dir/profile-before"
    (
        in_part subject
        date +%s%3N > "$run_dir/started_ms"
        subject_start
    ) > "$run_dir/subject.log" 2>&1 &
    echo $! > "$run_dir/subject.pid"
    (
        in_part driver
        case "$scenario" in
        rest) subject_open 15 && sleep 10 ;;
        # The stream is measured with one streaming thread open in the window, where the subject
        # can show one.
        stream) subject_stream 15 && { ! declare -F subject_show > /dev/null || subject_show; } ;;
        # What a subject's first start needs, before its home is frozen (subject_prepare, or 20 s).
        prepare) if declare -F subject_prepare > /dev/null; then subject_prepare; else sleep 20; fi ;;
        # For writing a subject's file: the subject starts, then a screenshot of the screen, at a
        # quarter of its size, shows where it stands after PROBE_WAIT seconds.
        probe)
            sleep "${PROBE_WAIT:-20}"
            grim -s 0.25 "$run_dir/probe-0.png"
            if [ -n "${PROBE_CLICK:-}" ]; then
                # shellcheck disable=SC2086 # x and y, split on purpose
                click $PROBE_CLICK
                sleep 3
                grim -s 0.25 "$run_dir/probe-1.png"
            fi
            if [ -n "${PROBE_KEYS:-}" ]; then
                # shellcheck disable=SC2086 # the keys are wtype's arguments, split on purpose
                wtype $PROBE_KEYS
                sleep 3
                grim -s 0.25 "$run_dir/probe-2.png"
            fi
            ;;
        *) echo "unknown scenario $scenario" >&2; exit 2 ;;
        esac
    ) > "$run_dir/driver.log" 2>&1 || { echo "the driver failed; see driver.log" >&2; subject_stop; return 1; }
    # What the window shows while it is measured, kept with the run's figures.
    grim -s 0.25 "$run_dir/measured.png" 2>/dev/null || true
    if [ "$scenario" = prepare ]; then
        stop_and_wait
        tar -cf "$run_dir/home.tar" -C "$HOME" .
        return 0
    fi
    mark_ready
    wait_for "${BENCH_MAX_WAIT:-7200}" test -e "$run_dir/done" || true
    stat -c %s "$run_dir/compositor.log" > "$run_dir/done.offset"
    stop_and_wait
    profile_bytes > "$run_dir/profile-after"
    # Where the profile's bytes are, for reading its growth.
    du -sb "$HOME"/.[!.]* "$HOME"/* 2>/dev/null | sort -rn > "$run_dir/profile-after.txt" || true
}
