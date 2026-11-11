# shellcheck shell=bash
# T3 Code's desktop application, the vendor's AppImage, unpacked once into
# target/rivals/packages/ (scripts/rivals/README.md), run through nixpkgs' appimage-run. It drives
# the same Claude Code CLI as Pluto, pointed at the fake model by its instance's environment, as
# T3 Code's documentation describes.

T3CODE_VERSION=0.1.8
T3CODE_DIR=$repo/target/rivals/packages/t3code-$T3CODE_VERSION/unpacked

subject_version() {
    printf 't3code %s (AppImage sha256 %s); claude %s\n' "$T3CODE_VERSION" \
        "$(cut -c1-16 "$repo/target/rivals/packages/t3code-$T3CODE_VERSION/SHA256")" \
        "$(basename "$(dirname "$(dirname "$(readlink -f "$(command -v claude)")")")")"
}

subject_binds() { :; }

subject_agents() { echo claude codex opencode; }

subject_parts() { :; }

subject_setup() {
    mkdir -p "$HOME/.t3/userdata" "$HOME/project"
    git -C "$HOME/project" init -q -b main
    echo "# A project" > "$HOME/project/README.md"
    git -C "$HOME/project" add . && git -C "$HOME/project" -c user.name=bench -c user.email=bench@example.invalid commit -qm init
    cat > "$HOME/.t3/userdata/settings.json" <<JSON
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
    }
  }
}
JSON
}

subject_start() {
    cd "$HOME/project"
    T3CODE_TELEMETRY_ENABLED=false exec appimage-run -w "$T3CODE_DIR"
}

# The first start shows a welcome page: its Start button is the second element from the end
# (the last is the privacy policy's link). A new virtual keyboard may lose its first keys, so the
# keys wait a little; then the page must be gone: T3 Code keeps the date it was dismissed.
dismiss_welcome() {
    sleep 20
    wtype -s 300 -M shift -k Tab -s 100 -k Tab -m shift -s 100 -k Return -s 300
    wait_for 15 grep -rqa --include='*settings.json' onboardingCompletedAt "$HOME" ||
        { echo "the welcome page was not dismissed"; return 1; }
}

# T3 Code's command line, installed at first start; it runs the AppImage's Electron as Node, so it
# needs the AppImage's environment too.
# appimage-run runs APPIMAGE_DEBUG_EXEC without arguments, so the call goes through a script.
t3() {
    printf '#!/bin/sh\nexec %q' "$HOME/.local/bin/t3" > "$HOME/t3-call"
    printf ' %q' "$@" >> "$HOME/t3-call"
    chmod +x "$HOME/t3-call"
    APPIMAGE_DEBUG_EXEC="$HOME/t3-call" appimage-run -w "$T3CODE_DIR"
}

# T3 Code's own server listens on 127.0.0.1:3773 inside the sandbox; its command line issues a
# session token and adds the project, and the threads are launched through the protocol Pluto's
# client speaks, as for Pluto.
launch() {
    local threads=$1
    shift
    wait_for 60 test -x "$HOME/.local/bin/t3" || { echo "the t3 command was not installed"; return 1; }
    # The frozen profile already holds the command: its server must be listening too.
    wait_for 120 listening 3773 || { echo "T3 Code's server never listened"; return 1; }
    t3 auth session issue --base-dir "$HOME/.t3" --ttl 1d --label bench --token-only > "$HOME/token" || return 1
    # Under a heavy load, T3 Code's server has three times sent nothing for a minute to a client
    # that had just connected, before drive launched any thread; the cause is not known. drive may
    # then try once more, only if no thread reached the fake model, so that none is doubled.
    local attempt
    for attempt in 1 2; do
        if "$repo/target/release/examples/drive" http://127.0.0.1:3773 "$HOME/token" claudeAgent \
            claude-sonnet-5-5 "$threads" "$@" 2> "$run_dir/drive-$attempt.log"; then
            return 0
        fi
        cat "$run_dir/drive-$attempt.log"
        grep -q "the server stopped answering" "$run_dir/drive-$attempt.log" || return 1
        [ "$(grep -c '"kind":"main"' "$run_dir/fake.jsonl" 2>/dev/null || true)" = 0 ] || return 1
        echo "drive: no answer from T3 Code's server (attempt $attempt)"
    done
    return 1
}

# The first start: the welcome page dismissed, the t3 command installed, the project added.
subject_prepare() {
    wait_for 60 test -x "$HOME/.local/bin/t3" || { echo "the t3 command was not installed"; return 1; }
    dismiss_welcome || { grim -s 0.25 "$run_dir/after-welcome.png"; return 1; }
    t3 project add "$HOME/project" --base-dir "$HOME/.t3" || return 1
    sleep 5
}

subject_open() {
    launch "$1" 0 1 2 || return 1
    wait_for 600 finished "$1" 2 0 1 2
}

subject_stream() {
    launch "$1" 3 || return 1
    wait_for 120 finished "$1" 0 3 && sleep 3
}

# The newest thread opened in the window while it streams: T3 Code's default binding `mod+1`
# (`thread.jump.1`), through the keyboard as for Pluto.
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
