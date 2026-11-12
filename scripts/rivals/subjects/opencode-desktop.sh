# shellcheck shell=bash
# OpenCode Desktop, the vendor's AppImage (manifest.txt), unpacked once and run through nixpkgs'
# appimage-run. An Electron window over the OpenCode server it bundles, which it installs into its
# profile at first start (~/.config/ai.opencode.desktop/cli/) and runs as `opencode-cli serve
# --service`: the server is the agent. It reaches the fake model as a custom OpenAI-compatible
# provider of its configuration (v2's `providers`, as its documentation shows one), its key from
# the environment, the only provider enabled, its model the default.
#
# Its sessions are launched through its server's API, as Pluto's and T3 Code's through theirs:
# `POST /api/session` in the project, then `POST /api/session/{id}/prompt`, with the address and the
# password its server writes in ~/.local/state/opencode/service.json (HTTP basic, user `opencode`).
# The window shows them on its home (Ctrl+H), the newest first; it is opened there by the pointer.
#
# Probed on 2026-11-12 (M1.6b2a): it renders on the GPU, keeps one window, and streams 15 sessions
# at once in its one server.

# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

OPENCODE_DESKTOP_DIR=$(package_dir opencode-desktop)
OPENCODE_SERVICE=.local/state/opencode/service.json

subject_version() {
    printf '%s\n' "$(vendor_version opencode-desktop)"
}

subject_binds() { :; }

subject_agents() { echo opencode-cli; }

subject_parts() { :; }

subject_setup() {
    make_project
    mkdir -p "$HOME/.config/opencode"
    cat > "$HOME/.config/opencode/opencode.json" <<JSON
{
  "\$schema": "https://opencode.ai/config.json",
  "enabled_providers": ["fake"],
  "providers": {
    "fake": {
      "name": "Fake",
      "env": ["FAKE_API_KEY"],
      "package": "@opencode/ai/providers/openai-compatible",
      "settings": {"baseURL": "$FAKE_URL/v1"},
      "models": {"fake-model": {"name": "Fake"}}
    }
  },
  "model": "fake/fake-model"
}
JSON
}

subject_start() {
    cd "$HOME/project" || return 1
    FAKE_API_KEY=fake exec appimage-run -w "$OPENCODE_DESKTOP_DIR"
}

# A call to its server's API: a method, a path, and a JSON body if any; the answer on stdout.
api() {
    local service=$HOME/$OPENCODE_SERVICE
    curl -sf -u "opencode:$(jq -r .password "$service")" -X "$1" -H "Content-Type: application/json" \
        ${3:+-d "$3"} "$(jq -r .url "$service")$2"
}

# Whether its server answers.
serving() {
    [ -s "$HOME/$OPENCODE_SERVICE" ] && api GET /api/info > /dev/null
}

# Its first start installs its command line and starts its server.
subject_prepare() {
    wait_for 120 serving || { echo "OpenCode's server never answered"; return 1; }
    sleep 10
}

# The sessions, one a thread, in the project, with the fake model.
launch() {
    local threads=$1
    shift
    local turns=("$@") n id
    wait_for 120 serving || { echo "OpenCode's server never answered"; return 1; }
    for n in $(seq 0 $((threads - 1))); do
        id=$(api POST /api/session "{\"location\":{\"directory\":\"$HOME/project\"},\"model\":{\"id\":\"fake-model\",\"providerID\":\"fake\"}}" |
            jq -r .data.id) || return 1
        api POST "/api/session/$id/prompt" "{\"text\":\"$(typed_prompt "${turns[$((n % ${#turns[@]}))]}")\"}" > /dev/null || return 1
    done
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

# The newest session opened in the window while it streams: its home (the toolbar's second
# button; its shortcut, Ctrl+H, did not open it), then the first session of its list.
subject_show() {
    click 64 20
    sleep 2
    click 1816 212
    sleep 3
}

subject_stop() {
    local pid
    pid=$(cat "$run_dir/subject.pid" 2>/dev/null) || return 0
    kill -TERM "$pid" 2>/dev/null || true
    wait_for 15 gone "$pid" || true
    stop_every_process
}
