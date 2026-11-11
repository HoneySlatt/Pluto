# shellcheck shell=bash
# Pluto as it is today: the fork (`pluto-server`) and the egui client, driving the real Claude Code
# CLI of the measure shell against the fake model. Its threads are launched through the server's
# protocol, as the client launches them (crates/pluto-core/examples/drive.rs).

PLUTO_PORT=3773
PLUTO_URL=http://127.0.0.1:$PLUTO_PORT
# The fork's provider instance the threads run on, and its model: Claude Code here;
# pluto-codex.sh and pluto-opencode.sh change them.
PLUTO_INSTANCE=claudeAgent
PLUTO_MODEL=claude-sonnet-5-5

# A program's package as its store path names it (claude-code-2.1.311): never by running it.
package_of() {
    basename "$(dirname "$(dirname "$(readlink -f "$(command -v "$1")")")")"
}

# The client by its binary's hash, not the repository's commit: a commit that leaves the client
# alone must not make its frozen profile stale.
subject_version() {
    printf 'client sha256 %s; server %s; claude %s\n' "$(sha256sum "$repo/target/release/pluto" | cut -c1-16)" \
        "$(package_of pluto-server)" "$(package_of claude)"
}

subject_binds() {
    dirname "$(dirname "$(readlink -f "$(command -v pluto-server)")")"
}

subject_agents() { echo claude codex opencode; }

# The client and the server are measured apart too, as M1.2 measured them.
subject_parts() { echo client server; }

subject_setup() {
    pluto_setup
}

# The fork's settings and the project, which pluto-opencode.sh extends.
pluto_setup() {
    mkdir -p "$HOME/pluto/userdata" "$HOME/project"
    git -C "$HOME/project" init -q -b main
    echo "# A project" > "$HOME/project/README.md"
    git -C "$HOME/project" add . && git -C "$HOME/project" -c user.name=bench -c user.email=bench@example.invalid commit -qm init
    cat > "$HOME/pluto/userdata/settings.json" <<JSON
{
  "providerInstances": {
    "claudeAgent": {
      "driver": "claudeAgent",
      "enabled": true,
      "environment": [
        {"name": "ANTHROPIC_BASE_URL", "value": "$FAKE_URL"},
        {"name": "ANTHROPIC_AUTH_TOKEN", "value": "fake"},
        {"name": "DISABLE_TELEMETRY", "value": "1"},
        {"name": "DISABLE_ERROR_REPORTING", "value": "1"},
        {"name": "DISABLE_AUTOUPDATER", "value": "1"},
        {"name": "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "value": "1"}
      ]
    },
    "codex": {
      "driver": "codex",
      "enabled": true,
      "environment": [{"name": "FAKE_KEY", "value": "fake"}]
    }
  }
}
JSON
    # Codex writes the threads' titles; it reads its provider from its home's config.
    mkdir -p "$HOME/.codex"
    cat > "$HOME/.codex/config.toml" <<TOML
model_provider = "fake"
[model_providers.fake]
name = "Fake"
base_url = "$FAKE_URL/v1"
wire_api = "responses"
env_key = "FAKE_KEY"
[analytics]
enabled = false
TOML
}

subject_start() {
    # The fork sends usage events to PostHog unless told not to; the sandbox would drop them anyway.
    (
        in_part subject/server
        T3CODE_TELEMETRY_ENABLED=false exec pluto-server serve --base-dir "$HOME/pluto" \
            --port "$PLUTO_PORT" --host 127.0.0.1
    ) > "$run_dir/server.log" 2>&1 &
    wait_for 60 grep -q "server is ready" "$run_dir/server.log" || { echo "the server did not start"; return 1; }
    pluto-server auth session issue --base-dir "$HOME/pluto" --ttl 1d --label bench --token-only \
        > "$HOME/token" 2>/dev/null
    in_part subject/client
    exec "$repo/target/release/pluto" --server "$PLUTO_URL" --token-file "$HOME/token"
}

# The first start: the server makes its database, and the project is added to it.
subject_prepare() {
    wait_for 60 test -s "$HOME/token" || return 1
    pluto-server project add "$HOME/project" --base-dir "$HOME/pluto" > "$run_dir/project.log" 2>&1
    sleep 10
}

subject_open() {
    local threads=$1
    wait_for 60 test -s "$HOME/token" || return 1
    wait_for 60 listening "$PLUTO_PORT" || return 1
    "$repo/target/release/examples/drive" "$PLUTO_URL" "$HOME/token" "$PLUTO_INSTANCE" "$PLUTO_MODEL" \
        "$threads" 0 1 2 || return 1
    wait_for 600 finished "$threads" 2 0 1 2
}

subject_stream() {
    local threads=$1
    wait_for 60 test -s "$HOME/token" || return 1
    wait_for 60 listening "$PLUTO_PORT" || return 1
    "$repo/target/release/examples/drive" "$PLUTO_URL" "$HOME/token" "$PLUTO_INSTANCE" "$PLUTO_MODEL" \
        "$threads" 3 || return 1
    # The journal writes a request once its answer ends: segment 0's end is segment 1's start.
    wait_for 120 finished "$threads" 0 3 && sleep 3
}

# The newest thread, first in the sidebar, opened in the window while it streams: its default
# binding `mod+1` (`thread.jump.1`). Through the keyboard, not the pointer: each `wlrctl` call is a
# new virtual pointer, whose click sway handles before the window has the pointer's focus.
subject_show() {
    wtype -s 300 -M ctrl -k 1 -m ctrl -s 300
    sleep 3
}

subject_stop() {
    local pid
    pid=$(cat "$run_dir/subject.pid" 2>/dev/null) || return 0
    pkill -TERM -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
}
