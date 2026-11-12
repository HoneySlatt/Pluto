# shellcheck shell=bash
# Jan, the vendor's AppImage (manifest.txt), unpacked once and run through nixpkgs' appimage-run.
# A Tauri application, its window WebKitGTK's. Its AppImage forces GTK's X11 backend
# (apprun-hooks/linuxdeploy-plugin-gtk.sh: "Crash with Wayland backend"), so on a Wayland desktop
# it runs through Xwayland, which sway starts for it: Xwayland draws what Jan draws, so it is
# counted as a part of Jan's (`xwayland`), moved out of the compositor's cgroup once it starts.
# It reaches the fake model as an OpenAI-compatible custom provider, added at prepare through its
# settings, and keeps its key in its own encrypted file. It also starts its llama.cpp worker,
# though no local model is used: Jan's own behaviour, kept in its figure. It is driven by the
# keyboard, its first start by the pointer.
#
# Its first start, at prepare, with the network, through its own windows: its setup (its
# inference engine, and a small embedding model for file search, downloaded), its usage data
# denied ("Help Improve Jan": Deny), its first model skipped (no local model), the provider added
# and its model list fetched, and the fake model chosen for new chats.
#
# Probed on 2026-11-11 (M1.6b2a): it renders on the GPU (Jan's and WebKit's processes, and
# Xwayland), keeps one window, and streams 15 chats at once.

# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

# A chat: the fake model plays chat turns.
FAKE_CHAT=1
JAN_DIR=$(package_dir jan)

subject_version() {
    printf '%s\n' "$(vendor_version jan)"
}

subject_binds() { :; }

subject_agents() { echo; }

subject_parts() { echo app xwayland; }

subject_setup() { :; }

subject_start() {
    xwayland_counted_with_subject
    in_part subject/app
    # Xwayland's display, which a Wayland desktop gives every program.
    DISPLAY=:0 exec appimage-run -w "$JAN_DIR"
}

# Xwayland, once sway starts it for Jan's first X11 window, moved into the subject's `xwayland`
# part, from the bench's own part, where this waits.
xwayland_counted_with_subject() {
    (
        in_part bench
        wait_for 120 pgrep -x Xwayland > /dev/null || exit 0
        pgrep -x Xwayland > "$cg/subject/xwayland/cgroup.procs"
    ) &
}

# Its first start, by the pointer, at the places its windows have on the 3840 × 2160 output; each
# step's screenshot is kept.
subject_prepare() {
    sleep 40
    # "Let's start": its engine set up, then "Continue".
    click 2036 1192
    sleep 30
    shot prepare-1
    click 2036 1252
    sleep 15
    # "Help Improve Jan": Deny.
    shot prepare-2
    click 2144 1204
    sleep 8
    # "Your first model": none; "Browse other models in the Hub" ends the setup.
    shot prepare-3
    click 1936 1224
    sleep 4
    # Settings, a new custom provider: its name, its base URL, its key, then "Create".
    click 68 212
    sleep 3
    click 452 508
    sleep 3
    input "type Fake"
    click 1920 1104
    input "type $FAKE_URL/v1"
    click 1920 1172
    input "type fake"
    sleep 1
    shot prepare-4
    click 2244 1228
    sleep 4
    # Its models fetched from the provider.
    click 3760 552
    sleep 4
    shot prepare-5
    # A new chat, the fake model chosen in it: new chats keep it.
    input "key ctrl+n" "sleep 1500"
    click 344 28
    sleep 2
    click 312 190
    sleep 2
    shot prepare-6
    wait_for 30 grep -qrs '"Fake"' "$HOME/.local/share/Jan" ||
        { echo "Jan has no provider Fake"; return 1; }
    [ "$(journaled_listings)" -gt 0 ] || { echo "Jan never listed the fake model's models"; return 1; }
    sleep 5
}

# How many listings the fake model has answered.
journaled_listings() {
    grep -c '"kind":"listing"' "$run_dir/fake.jsonl" 2>/dev/null || true
}

subject_open() {
    launch "$1" 0 1 2 || return 1
    wait_for 600 finished "$1" 0 0 1 2
}

subject_stream() {
    launch "$1" 3 || return 1
    wait_for 120 started "$1" 3
}

# The threads, one a chat: a new chat (Ctrl+N), whose message box takes the focus. The newest is
# the one shown while they stream.
launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 30
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
