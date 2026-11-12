# shellcheck shell=bash
# Open WebUI in its lean configuration (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md):
# the one its vendor documents for one user chatting with a remote OpenAI-compatible endpoint, its
# slim image (no PyTorch, no local embedder), run without Docker (vendors.sh), its page in Chromium
# (chromium.sh). Its figure is the whole: the server and the browser, each also measured apart. It
# reaches the fake model over Chat Completions; the fake model plays chat turns (--chat).
#
# The changes from the image's own configuration, each for a reason (openwebui_environment):
# - the endpoint, OPENAI_API_BASE_URL and OPENAI_API_KEY, as its documentation shows them;
# - ENABLE_OLLAMA_API=false: no Ollama, the documented switch for an OpenAI-compatible endpoint
#   alone;
# - WEBUI_SECRET_KEY_FILE in the data folder: the image writes its key beside its code, which each
#   run starts afresh, so the frozen sign-in would not hold; kept with the data, it lasts as in a
#   container that is restarted, not recreated;
# - WEBUI_ADMIN_EMAIL, _PASSWORD and _NAME: the first user made at the first start, as documented
#   for a headless install, rather than through the sign-up page;
# - SCARF_NO_ANALYTICS, DO_NOT_TRACK, ANONYMIZED_TELEMETRY: telemetry off; the image already sets
#   them.
#
# Its first start, at prepare: signed in through the page, its "What's New" closed; the fake model,
# its only one, is its default. A new chat is opened by the button of its side bar: its shortcut,
# Ctrl+Shift+O, is also Chromium's bookmark manager's, which opens a window of its own.
#
# Probed on 2026-11-12 (M1.6b2a): its server starts in the sandbox, the page renders on the GPU in
# one window, and 15 chats stream at once.

# shellcheck source=../chromium.sh
source "$(dirname "${BASH_SOURCE[0]}")/../chromium.sh"
# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

# A chat: the fake model plays chat turns.
FAKE_CHAT=1
# openwebui-default.sh changes it.
OPENWEBUI_IMAGE=open-webui-slim
OPENWEBUI_PORT=8080
OPENWEBUI_URL=http://127.0.0.1:$OPENWEBUI_PORT
OPENWEBUI_EMAIL=bench@example.com
OPENWEBUI_PASSWORD=bench

subject_version() {
    printf '%s; %s\n' "$(vendor_version "$OPENWEBUI_IMAGE")" "$(chromium_version)"
}

subject_binds() { :; }

subject_agents() { echo; }

subject_parts() { echo server browser; }

# Its sign-in lasts four weeks (JWT_EXPIRES_IN's default).
subject_session_days() { echo 28; }

openwebui_environment() {
    printf '%s\n' \
        "OPENAI_API_BASE_URL=$FAKE_URL/v1" \
        "OPENAI_API_KEY=fake" \
        "ENABLE_OLLAMA_API=false" \
        "WEBUI_SECRET_KEY_FILE=/app/backend/data/.webui_secret_key" \
        "WEBUI_ADMIN_EMAIL=$OPENWEBUI_EMAIL" \
        "WEBUI_ADMIN_PASSWORD=$OPENWEBUI_PASSWORD" \
        "WEBUI_ADMIN_NAME=Bench" \
        "SCARF_NO_ANALYTICS=true" \
        "DO_NOT_TRACK=true" \
        "ANONYMIZED_TELEMETRY=false"
}

subject_setup() {
    mkdir -p "$HOME/openwebui"
}

# The server, from its image's own command, then the browser once the server answers: the start to
# the first window includes the server's start.
subject_start() {
    local environment
    mapfile -t environment < <(openwebui_environment)
    (
        in_part subject/server
        run_image "$OPENWEBUI_IMAGE" --bind "$HOME/openwebui" /app/backend/data -- \
            "${environment[@]}" bash start.sh
    ) > "$run_dir/server.log" 2>&1 &
    wait_for 300 served "$OPENWEBUI_URL/health" || { echo "Open WebUI's server did not answer"; return 1; }
    in_part subject/browser
    start_chromium "$OPENWEBUI_URL/"
}

# Its first start, by the pointer and the keyboard, at the places its page has on the 3840 × 2160
# output; each step's screenshot is kept.
subject_prepare() {
    wait_for 300 served "$OPENWEBUI_URL/health" || return 1
    sleep 15
    click 1880 1038
    input "type $OPENWEBUI_EMAIL"
    click 1880 1096
    input "type $OPENWEBUI_PASSWORD" "key Return"
    sleep 8
    shot prepare-1
    # "What's New": "Okay, Let's Go!".
    click 2296 1680
    sleep 3
    shot prepare-2
    # The page keeps its sign-in in its local storage, which Chromium writes to the profile some
    # seconds later.
    sleep 30
}

subject_open() {
    launch "$1" 0 1 2 || return 1
    wait_for 600 finished "$1" 0 0 1 2
}

subject_stream() {
    launch "$1" 3 || return 1
    wait_for 120 started "$1" 3
}

# The threads, one a chat: Open WebUI's new chat (the first button of its side bar), then its
# message box clicked.
launch() {
    local threads=$1
    shift
    local turns=("$@") n
    wait_for 120 served "$OPENWEBUI_URL/health" || return 1
    sleep 10
    for n in $(seq 0 $((threads - 1))); do
        typed_turn "${turns[$((n % ${#turns[@]}))]}" "move 20 52" "sleep 500" "click" "sleep 1500" \
            "move 1900 1040" "sleep 500" "click" "sleep 300" || return 1
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
