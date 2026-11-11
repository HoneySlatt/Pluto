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
    mkdir -p "$HOME/.config/zed" "$HOME/project"
    git -C "$HOME/project" init -q -b main
    echo "# A project" > "$HOME/project/README.md"
    git -C "$HOME/project" add . && git -C "$HOME/project" -c user.name=bench -c user.email=bench@example.invalid commit -qm init
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

# One thread: a new thread in the agent panel, the prompt typed, sent. One wtype for all of it,
# with pauses: each wtype is a new virtual keyboard, whose first keys Zed may drop while it loads
# the keyboard's keymap. A prompt can still be lost (2 of 15 in one run of M1.6a): the driver waits
# until the turn's first request reaches the fake model, and types it once more, in a new thread,
# if it does not within 30 seconds.
zed_thread() {
    local turn=$1 before attempt
    before=$(journaled "$turn" 0)
    for attempt in 1 2; do
        wtype -s 300 -M ctrl -M alt -k n -m alt -m ctrl -s 1000 \
            "[turn $turn] Look at this project and tell me what you find." -s 500 -k Return -s 300
        if wait_for 30 more_than "$before" journaled "$turn" 0; then
            return 0
        fi
        echo "turn $turn: the prompt did not reach the model (attempt $attempt)"
        grim -s 0.25 "$run_dir/lost-turn-$turn-$attempt.png"
    done
    return 1
}

# Whether a count, given by the command that follows, is above `before`.
more_than() {
    local before=$1
    shift
    [ "$("$@")" -gt "$before" ]
}

launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 15
    # The agent panel takes the focus.
    wtype -s 300 -M ctrl -M alt -k a -m alt -m ctrl -s 300
    sleep 2
    for n in $(seq 0 $((threads - 1))); do
        zed_thread "${turns[$((n % ${#turns[@]}))]}" || return 1
    done
    grim -s 0.25 "$run_dir/after-launch.png"
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
