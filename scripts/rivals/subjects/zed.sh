# shellcheck shell=bash
# Zed, the vendor's archive, unpacked once into target/rivals/packages/ (scripts/rivals/README.md),
# run in the FHS environment nixpkgs gives AppImages (appimage-run), since its binary expects a
# standard Linux file system. Its own agent talks to the fake model as an Anthropic-compatible
# provider, as Zed's documentation describes; it is driven by the keyboard.

ZED_VERSION=1.27.0
ZED_DIR=$repo/target/rivals/packages/zed-$ZED_VERSION/unpacked/zed.app

subject_version() {
    printf 'zed %s (archive sha256 %s)\n' "$ZED_VERSION" \
        "$(cut -c1-16 "$repo/target/rivals/packages/zed-$ZED_VERSION/SHA256")"
}

subject_binds() { :; }

# Zed's agent runs its tools itself: no agent's process of its own.
subject_agents() { echo; }

subject_parts() { :; }

subject_setup() {
    mkdir -p "$HOME/.config/zed"
    make_project
    cat > "$HOME/.config/zed/settings.json" <<JSON
{
  "telemetry": {"diagnostics": false, "metrics": false},
  "auto_update": false,
  "session": {"trust_all_worktrees": true},
  "language_models": {
    "anthropic_compatible": {
      "Fake": {
        "api_url": "$FAKE_URL",
        "available_models": [
          {"name": "fake-model", "display_name": "Fake", "max_tokens": 200000, "max_output_tokens": 32000}
        ]
      }
    }
  },
  "agent": {
    "default_model": {"provider": "Fake", "model": "fake-model"},
    "tool_permissions": {"default": "allow"}
  }
}
JSON
    # Two shortcuts of the bench's own, so that the driver never depends on a layout's symbols.
    cat > "$HOME/.config/zed/keymap.json" <<'JSON'
[{"bindings": {"ctrl-alt-a": "agent::ToggleFocus", "ctrl-alt-n": "agent::NewThread"}}]
JSON
}

subject_start() {
    printf '#!/bin/sh\nexec %q %q\n' "$ZED_DIR/libexec/zed-editor" "$HOME/project" > "$HOME/zed-start"
    chmod +x "$HOME/zed-start"
    FAKE_API_KEY=fake APPIMAGE_DEBUG_EXEC="$HOME/zed-start" exec appimage-run -w "$ZED_DIR"
}

launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 15
    # The agent panel takes the focus.
    input "sleep 300" "key ctrl+alt+a" "sleep 300"
    sleep 2
    for n in $(seq 0 $((threads - 1))); do
        # A new thread in the agent panel, then its prompt (lib.sh's typed_turn).
        typed_turn "${turns[$((n % ${#turns[@]}))]}" "sleep 300" "key ctrl+alt+n" "sleep 1000" || return 1
    done
    shot after-launch
}

subject_open() {
    launch "$1" 0 1 2 || return 1
    wait_for 600 finished "$1" 2 0 1 2
}

subject_stream() {
    launch "$1" 3 || return 1
    wait_for 120 finished "$1" 0 3 && sleep 3
}

subject_stop() {
    local pid
    pid=$(cat "$run_dir/subject.pid" 2>/dev/null) || return 0
    pkill -TERM -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
}
