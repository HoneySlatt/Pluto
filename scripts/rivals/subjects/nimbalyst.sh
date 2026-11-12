# shellcheck shell=bash
# Nimbalyst, the vendor's AppImage (manifest.txt), unpacked once and run through nixpkgs'
# appimage-run, the project opened as its workspace from its command line. Its agent is Claude
# Code, the build Nimbalyst bundles (resources/claude-runtime/linux-x64/claude, 2.1.321 in 0.83.0,
# read from the binary), run through Claude's Agent SDK, one process a session, and pointed at the
# fake model by Claude Code's user settings (lib.sh's claude_on_fake_model), as Nimbalyst's
# documentation describes for a custom endpoint. It also starts its bundled Codex as an app server
# at start, idle here. It is driven by the keyboard, its first start by the pointer.
#
# Its defaults changed at prepare, through its own windows: Developer Mode (its coding agents), the
# project's agents allowed to do everything (no prompt for a permission, as Pluto's threads run in
# full access), and its analytics off (Settings, Advanced, "Send Anonymous Usage Data"); its
# documentation says one start event is still sent when analytics is off.
#
# Probed on 2026-11-11 (M1.6b2a): it renders on the GPU, keeps one window, and streams 15 sessions
# at once, each its own `claude` process.

# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

NIMBALYST_DIR=$(package_dir nimbalyst)
NIMBALYST_SETTINGS=.config/@nimbalyst/electron

subject_version() {
    printf '%s; claude 2.1.321 (bundled)\n' "$(vendor_version nimbalyst)"
}

subject_binds() { :; }

subject_agents() { echo claude codex; }

subject_parts() { :; }

subject_setup() {
    make_project
    claude_on_fake_model
}

subject_start() {
    cd "$HOME/project" || return 1
    # The project opened as its workspace, given on its command line.
    exec appimage-run -w "$NIMBALYST_DIR" "$HOME/project"
}

# Its first start, by the pointer, at the places its windows have on the 3840 × 2160 output; each
# step's screenshot is kept. Then its analytics must be off in its settings.
subject_prepare() {
    sleep 30
    # The welcome: Developer Mode, then "Get started…" (no tutorial).
    click 2089 984
    sleep 1
    click 1837 1389
    sleep 6
    shot prepare-1
    # The project's agents: "Allow everything", then "Start working".
    click 1708 1028
    sleep 1
    click 2100 1308
    sleep 3
    shot prepare-2
    # Settings, Advanced, "Send Anonymous Usage Data" off.
    input "key ctrl+,"
    sleep 3
    click 128 348
    sleep 2
    click 1088 920
    sleep 2
    shot prepare-3
    # Back to the agents' mode (Ctrl+K), which the next start reopens.
    input "key Escape" "sleep 500" "key ctrl+k"
    wait_for 10 grep -q '"analyticsEnabled": false' "$HOME/$NIMBALYST_SETTINGS/app-settings.json" ||
        { echo "Nimbalyst's analytics is still on"; return 1; }
    grep -q '"developerMode": true' "$HOME/$NIMBALYST_SETTINGS/app-settings.json" ||
        { echo "Nimbalyst is not in Developer Mode"; return 1; }
    sleep 5
}

subject_open() {
    launch "$1" 0 1 2 || return 1
    wait_for 600 finished "$1" 2 0 1 2
}

subject_stream() {
    launch "$1" 3 || return 1
    # Every long answer has begun: they all stream.
    wait_for 120 started "$1" 3 1
}

# The threads, one a session: a new session (Ctrl+N), whose message box takes the focus. The newest
# is the one shown while they stream.
launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 30
    # The agents' mode, whichever the last start left open.
    input "key ctrl+k" "sleep 1000"
    for n in $(seq 0 $((threads - 1))); do
        typed_turn "${turns[$((n % ${#turns[@]}))]}" "key ctrl+n" "sleep 1500" || return 1
    done
    shot after-launch
}

subject_stop() {
    local pid
    pid=$(cat "$run_dir/subject.pid" 2>/dev/null) || return 0
    kill -TERM "$pid" 2>/dev/null || true
    wait_for 15 gone "$pid" || true
    stop_every_process
}
