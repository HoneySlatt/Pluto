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

# Whether a web server of the sandbox answers a page with success.
served() {
    curl -sf -o /dev/null --max-time 5 "$1"
}

# Whether a process has ended.
gone() {
    ! kill -0 "$1" 2>/dev/null
}

# Whether a count, given by the command that follows, is above `before`.
more_than() {
    local before=$1
    shift
    [ "$("$@")" -gt "$before" ]
}

# The prompt a driver types for a turn: the same words for every subject that is typed into.
typed_prompt() {
    echo "[turn $1] Look at this project and tell me what you find."
}

# Types a turn's prompt by the keyboard, after the input commands given, which open a new thread and
# put the focus in its message box, and sends it (Return, or the subject's SEND_KEY); then checks
# that the turn reached the fake model; if it does not within 30 seconds, all of it once more, in a
# new thread. A typed prompt can be lost while an application is busy loading; the check makes
# sure each turn is played once.
typed_turn() {
    local turn=$1 before attempt
    shift
    before=$(begun "$turn" 0)
    for attempt in 1 2; do
        input "$@" "type $(typed_prompt "$turn")" "sleep 500" "key ${SEND_KEY:-Return}" "sleep 300"
        if wait_for 30 more_than "$before" begun "$turn" 0; then
            return 0
        fi
        echo "turn $turn: the prompt did not reach the model (attempt $attempt)"
        shot "lost-turn-$turn-$attempt"
    done
    return 1
}

# Asks every process of the subject to end, in all its parts: for a subject of several programs (a
# server, its database, a browser), once its main program has had its chance to end on its own.
stop_every_process() {
    local procs
    while read -r procs; do
        xargs -r kill -TERM < "$procs" 2>/dev/null || true
    done < <(find "$cg/subject" -name cgroup.procs)
}

# How many main requests of a turn's segment have begun: the journal writes a line when one is read.
begun() {
    local turn=$1 segment=$2
    grep -c "^{\"begin\":.*\"turn\":$turn,\"segment\":$segment}" "$run_dir/fake.jsonl" 2>/dev/null || true
}

# Whether at least `count` main requests of a turn's segment (its first by default) have begun: a
# chat's turns; an agent's long answers, at its turn's second segment.
started() {
    [ "$(begun "$2" "${3:-0}")" -ge "$1" ]
}

# How many model requests the fake model has journaled for a turn's segment, once their answer
# ended (main requests only).
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

# The project the agents work in: a git repository of one commit, at ~/project.
make_project() {
    mkdir -p "$HOME/project"
    git -C "$HOME/project" init -q -b main
    echo "# A project" > "$HOME/project/README.md"
    git -C "$HOME/project" add . && git -C "$HOME/project" -c user.name=bench -c user.email=bench@example.invalid commit -qm init
}

# Claude Code pointed at the fake model, for the subjects that run it on their own (Emdash,
# Nimbalyst): its user settings' environment, as Claude Code documents it, with its telemetry,
# error reports and updater off; and its first start and the project's trust already done, as for a
# user who has used it before.
claude_on_fake_model() {
    mkdir -p "$HOME/.claude"
    cat > "$HOME/.claude/settings.json" <<JSON
{
  "env": {
    "ANTHROPIC_BASE_URL": "$FAKE_URL",
    "ANTHROPIC_AUTH_TOKEN": "fake",
    "DISABLE_TELEMETRY": "1",
    "DISABLE_ERROR_REPORTING": "1",
    "DISABLE_AUTOUPDATER": "1",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"
  }
}
JSON
    cat > "$HOME/.claude.json" <<JSON
{
  "hasCompletedOnboarding": true,
  "projects": {"$HOME/project": {"hasTrustDialogAccepted": true, "hasCompletedProjectOnboarding": true}}
}
JSON
}

# The output's size, in pixels: the compositor's mode, and the extent of the pointer's positions.
OUTPUT_SIZE=3840x2160

# Plays commands on the run's keyboard and pointer (pluto-input; crates/pluto-input lists the
# commands), one argument a line, and returns once the compositor has had them all.
input() {
    local mark
    mark=$(mktemp -u /tmp/run/input-done.XXXXXX)
    # A pipe without its reader would block the writer: the tool must still run.
    kill -0 "$(cat /tmp/run/input.pid)" 2>/dev/null || { echo "the input tool has stopped" >&2; return 1; }
    printf '%s\n' "$@" "mark $mark" > "$run_dir/input"
    wait_for 120 test -e "$mark" || { echo "the input tool did not play: $*" >&2; return 1; }
    rm -f "$mark"
}

# Clicks at a point of the screen, in pixels of the output, after resting there half a second:
# buttons shown on hover ignore an instant click.
click() {
    input "move $1 $2" "sleep 500" "click"
}

# A screenshot of the whole output, at a quarter of its size, into the run's folder.
shot() {
    grim -s 0.25 "$run_dir/$1.png"
}

start_bus() {
    dbus-daemon --session --fork --address=unix:path=/tmp/run/bus --print-pid=3 3>/tmp/run/bus.pid
    export DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/run/bus
}

# A Secret Service on the session bus, as a desktop has, for the subjects that keep a secret in the
# system's keyring (Jan its providers' keys; Electron its own). It is gnome-keyring, its login
# keyring unlocked by a test password, `bench`: a keyring of the throwaway home, made at the first
# start and frozen with the profile. With an empty password gnome-keyring makes no login keyring,
# and the subjects fall back to a file of their own. It runs in the bench's part, as the bus does,
# never counted in the subject's figures. The run stops if the keyring has no unlocked default.
start_keyring() {
    printf bench | gnome-keyring-daemon --unlock --components=secrets > "$run_dir/keyring.log" 2>&1 ||
        { echo "the keyring did not start" >&2; return 1; }
    wait_for 10 keyring_ready || { echo "the keyring has no unlocked default collection" >&2; return 1; }
}

# Whether the Secret Service's default collection exists and is unlocked.
keyring_ready() {
    local collection
    collection=$(dbus-send --session --print-reply --dest=org.freedesktop.secrets /org/freedesktop/secrets \
        org.freedesktop.Secret.Service.ReadAlias string:default 2>/dev/null | sed -n 's/^ *object path "\(.*\)"$/\1/p')
    [ -n "$collection" ] && [ "$collection" != / ] &&
        dbus-send --session --print-reply --dest=org.freedesktop.secrets "$collection" \
            org.freedesktop.DBus.Properties.Get string:org.freedesktop.Secret.Collection string:Locked \
            2>/dev/null | grep -q 'boolean false'
}

# The run's keyboard and pointer: one of each for the whole run, made before the subject starts, so
# that it sees a desktop's seat from its first frame (crates/pluto-input). Drivers play commands on
# them through `input`; in a probe, so can the owner's lead, through the run's folder.
start_input() {
    mkfifo "$run_dir/input"
    "$PLUTO_INPUT" --commands "$run_dir/input" --size "$OUTPUT_SIZE" 2> "$run_dir/input.log" &
    echo $! > /tmp/run/input.pid
    wait_for 10 grep -q "pluto-input: ready" "$run_dir/input.log" ||
        { echo "the input tool did not start" >&2; cat "$run_dir/input.log" >&2; return 1; }
}

# The headless sway every subject draws into: 3840 × 2160 at 240 Hz on the GPU, one output,
# no input device of its own: the input tool's keyboard and pointer are the seat's. With FRAMES=1 it logs every client request (WAYLAND_DEBUG=server), which counts
# the subject's frames; the log slows the compositor, so runs that count frames are apart from
# those whose resources are kept (ADR-0026).
start_compositor() {
    printf 'output HEADLESS-1 mode %s@240Hz\ndefault_border none\n' "$OUTPUT_SIZE" > /tmp/run/sway.conf
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
        # A chat subject's file sets FAKE_CHAT=1: its turns are chat turns (the fake model's --chat).
        local chat=()
        [ "${FAKE_CHAT:-0}" = 1 ] && chat=(--chat)
        exec "$PLUTO_FAKE_MODEL" --script "$PLUTO_FAKE_SCRIPT" --listen "127.0.0.1:$FAKE_PORT" \
            --listen "[::1]:$FAKE_PORT" --journal "$run_dir/fake.jsonl" --speed "${FAKE_SPEED:-1}" "${chat[@]}"
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

# A profile is prepared with a route out, where an application's updater may download its next
# version and keep it for its next start: a frozen profile keeps none, so that every run starts the
# version measured. electron-updater keeps it in ~/.cache/<application>-updater/; what is removed is
# listed in the run's folder.
drop_pending_updates() {
    local folder
    for folder in "$HOME"/.cache/*-updater; do
        [ -e "$folder" ] || continue
        du -sb "$folder" >> "$run_dir/updates-dropped.txt"
        rm -rf "$folder"
    done
}

# Chromium's and Electron's locks on their profile (SingletonLock, SingletonCookie,
# SingletonSocket), which name the machine that held them: prepared in pasta's namespace, whose
# host name differs, a profile would look held by another computer, and Chromium would refuse it. A
# frozen profile holds no running program's state.
drop_running_locks() {
    find "$HOME" -name 'Singleton*' -type l -print -delete >> "$run_dir/locks-dropped.txt"
}

# The phase files run.sh reads: `ready` once the subject is in the state to measure, holding
# where the compositor's log stood; run.sh writes `done` when it has measured. Beside them, what the
# validity checks read at both ends of the measure (`at_bounds`).
mark_ready() {
    at_bounds ready
    stat -c %s "$run_dir/compositor.log" > "$run_dir/ready.tmp"
    mv "$run_dir/ready.tmp" "$run_dir/ready"
}

# The subject's windows, as the compositor's tree has them: its views, each with a process.
count_windows() {
    swaymsg -t get_tree | jq '[recurse(.nodes[]?, .floating_nodes[]?)
        | select((.type == "con" or .type == "floating_con") and .pid != null)] | length'
}

# What run.sh's validity checks read (validity.sh), at one end of the measure (`ready` or `done`):
# the subject's windows, the processes of each of its parts, the screen, and how far the fake
# model's journal had gone, which tells the answers in flight then.
at_bounds() {
    count_windows > "$run_dir/windows-$1.txt" 2>/dev/null || echo unknown > "$run_dir/windows-$1.txt"
    grim -t ppm -s 0.25 "$run_dir/screen-$1.ppm" 2>/dev/null || true
    # The subject's main program as it was started, its wrappers' own switches included.
    [ "$1" = ready ] && tr '\0' ' ' < "/proc/$(cat "$run_dir/subject.pid")/cmdline" > "$run_dir/command.txt" 2>/dev/null
    true
    local part
    for part in $(subject_parts); do
        echo "$part:$(wc -l < "$cg/subject/$part/cgroup.procs")"
    done > "$run_dir/parts-$1.txt"
    stat -c %s "$run_dir/fake.jsonl" > "$run_dir/journal-$1.offset" 2>/dev/null || echo 0 > "$run_dir/journal-$1.offset"
}

# Every process of the run, for writing a subject's file: its pid, its parent, its part (its cgroup
# under the run's scope), its name, its executable and its command line. `subject_agents` is set from
# it.
process_tree() {
    local pid part exe
    printf 'pid\tppid\tpart\tcomm\texe\targs\n'
    for pid in /proc/[0-9]*; do
        pid=${pid#/proc/}
        [ -r "/proc/$pid/stat" ] || continue
        part=$(sed -n 's|^0::||p' "/proc/$pid/cgroup" 2>/dev/null)
        exe=$(readlink "/proc/$pid/exe" 2>/dev/null || true)
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$pid" "$(awk '{print $4}' "/proc/$pid/stat" 2>/dev/null)" \
            "${part#"${cg#/sys/fs/cgroup}"}" "$(cat "/proc/$pid/comm" 2>/dev/null)" "$exe" \
            "$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | cut -c1-300)"
    done
}

# For writing a subject's file, with PROBE_CONSOLE=1: the subject keeps running, and each line
# written into the run's `console` pipe is run here, in the driver, its output in `console.log`
# (`shot NAME`, `input ...`, `process_tree`, a curl to the subject's API); `quit` ends the probe.
probe_console() {
    mkfifo "$run_dir/console"
    local line
    exec 7<> "$run_dir/console"
    echo "console ready" > "$run_dir/console.log"
    while read -r line <&7; do
        [ "$line" = quit ] && break
        printf '$ %s\n' "$line" >> "$run_dir/console.log"
        eval "$line" >> "$run_dir/console.log" 2>&1 || echo "(exit $?)" >> "$run_dir/console.log"
        printf '\n--\n' >> "$run_dir/console.log"
    done
    exec 7<&-
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
    start_input
    start_fake
    make_home "$scenario"
    start_keyring
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
        rest) subject_open "$THREADS" && sleep 10 ;;
        # The stream is measured with one streaming thread open in the window, where the subject
        # can show one.
        stream) subject_stream "$THREADS" && { ! declare -F subject_show > /dev/null || subject_show; } ;;
        # What a subject's first start needs, before its home is frozen (subject_prepare, or 20 s).
        prepare) if declare -F subject_prepare > /dev/null; then subject_prepare; else sleep 20; fi ;;
        # For writing a subject's file: the subject starts, then a screenshot of the screen, at a
        # quarter of its size, shows where it stands after PROBE_WAIT seconds; then PROBE_CLICK,
        # PROBE_INPUT and the console, each followed by a screenshot; then its processes.
        probe)
            sleep "${PROBE_WAIT:-20}"
            shot probe-0
            if [ -n "${PROBE_CLICK:-}" ]; then
                # shellcheck disable=SC2086 # x and y, split on purpose
                click $PROBE_CLICK
                sleep 3
                shot probe-1
            fi
            if [ -n "${PROBE_INPUT:-}" ]; then
                # The input tool's commands, separated by `;`.
                IFS=';' read -r -a probe_commands <<< "$PROBE_INPUT"
                input "${probe_commands[@]}"
                sleep 3
                shot probe-2
            fi
            [ "${PROBE_CONSOLE:-0}" = 1 ] && probe_console
            process_tree > "$run_dir/processes.tsv"
            ;;
        *) echo "unknown scenario $scenario" >&2; exit 2 ;;
        esac
    ) > "$run_dir/driver.log" 2>&1 || { echo "the driver failed; see driver.log" >&2; subject_stop; return 1; }
    # What the window shows while it is measured, kept with the run's figures.
    grim -s 0.25 "$run_dir/measured.png" 2>/dev/null || true
    if [ "$scenario" = prepare ]; then
        stop_and_wait
        drop_pending_updates
        drop_running_locks
        tar -cf "$run_dir/home.tar" -C "$HOME" .
        return 0
    fi
    mark_ready
    wait_for "${BENCH_MAX_WAIT:-7200}" test -e "$run_dir/done" || true
    at_bounds "done"
    stat -c %s "$run_dir/compositor.log" > "$run_dir/done.offset"
    stop_and_wait
    profile_bytes > "$run_dir/profile-after"
    # Where the profile's bytes are, for reading its growth.
    du -sb "$HOME"/.[!.]* "$HOME"/* 2>/dev/null | sort -rn > "$run_dir/profile-after.txt" || true
}
