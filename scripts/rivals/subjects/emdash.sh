# shellcheck shell=bash
# Emdash, the vendor's AppImage (manifest.txt), unpacked once and run through nixpkgs'
# appimage-run, as T3 Code. It drives the agents' CLIs it finds on the PATH: here the measure
# shell's Claude Code, as Pluto's and T3 Code's, pointed at the fake model by Claude Code's user
# settings (lib.sh's claude_on_fake_model), each task's in its own terminal (Emdash's default
# surface for Claude Code) and its own Git worktree, under ~/emdash/worktrees. Its telemetry is
# off by its documented switch, TELEMETRY_ENABLED=false. It is driven by the keyboard and the
# pointer.
#
# Its configuration, each for a reason:
# - the project has a remote of its own, a bare repository in the home: Emdash publishes each
#   task's branch, and refuses to create a task in a repository without one;
# - Claude Code's prompt for its bypass mode already accepted (skipDangerousModePermissionPrompt,
#   which Claude Code writes once its user accepts): Emdash's "Auto-approve permissions", turned on
#   at prepare, runs it in that mode, as Pluto's threads run in full access.
# Its first start, at prepare, through its own windows: GitHub skipped ("Skip for now"), the
# project added (its folder chosen in GTK's file chooser, by its path), and "Auto-approve
# permissions" turned on in the new task's window, which Emdash keeps for the next.
#
# Probed on 2026-11-11 (M1.6b2a): it renders on the GPU, keeps one window, and streams 15 tasks at
# once, each its own `claude` process.

# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

EMDASH_DIR=$(package_dir emdash)
# Its new task's window sends its prompt by Ctrl+Return.
SEND_KEY=ctrl+Return

subject_version() {
    printf '%s; claude %s\n' "$(vendor_version emdash)" \
        "$(basename "$(dirname "$(dirname "$(readlink -f "$(command -v claude)")")")")"
}

subject_binds() { :; }

subject_agents() { echo claude; }

subject_parts() { :; }

subject_setup() {
    make_project
    git init --bare -q -b main "$HOME/origin.git"
    git -C "$HOME/project" remote add origin "$HOME/origin.git"
    git -C "$HOME/project" push -q -u origin main
    claude_on_fake_model
    jq '. + {skipDangerousModePermissionPrompt: true}' "$HOME/.claude/settings.json" > "$HOME/.claude/settings.tmp" &&
        mv "$HOME/.claude/settings.tmp" "$HOME/.claude/settings.json"
}

subject_start() {
    cd "$HOME/project" || return 1
    TELEMETRY_ENABLED=false exec appimage-run -w "$EMDASH_DIR"
}

# Its first start, by the pointer and the keyboard, at the places its windows have on the
# 3840 × 2160 output; each step's screenshot is kept.
subject_prepare() {
    sleep 30
    # "Connect GitHub": "Skip for now"; then "Start shipping".
    click 1920 1216
    sleep 5
    click 1920 1140
    sleep 5
    shot prepare-1
    # "Open project": its folder chosen in the file chooser by its path, from the home folder
    # (Alt+Home; from its first view, Recent, Return opens nothing), the chooser's inline completion
    # removed before Return; then "Create" (Ctrl+Return).
    click 2200 1036
    sleep 3
    click 1880 1128
    sleep 3
    input "key alt+Home" "sleep 1000" "key ctrl+l" "sleep 500" "type $HOME/project" "sleep 1500" "key Delete" \
        "sleep 300" "key Return"
    sleep 3
    shot prepare-2
    input "key ctrl+Return"
    sleep 5
    shot prepare-3
    # A new task's window: "Auto-approve permissions" on, then closed without a task.
    input "key ctrl+n"
    sleep 3
    click 1696 1113
    sleep 1
    shot prepare-4
    input "key Escape"
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

# The threads, one a task: the project chosen in the sidebar, a new task (Ctrl+N), its prompt's box
# clicked. The newest is the one shown while they stream.
launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 30
    for n in $(seq 0 $((threads - 1))); do
        typed_turn "${turns[$((n % ${#turns[@]}))]}" "move 64 96" "sleep 500" "click" "sleep 800" \
            "key ctrl+n" "sleep 1200" "move 1914 1153" "sleep 500" "click" "sleep 300" || return 1
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
