# shellcheck shell=bash
# LibreChat in its lean configuration (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md):
# the one its vendor documents for one user chatting with a remote OpenAI-compatible endpoint, its
# API and MongoDB, from their images (`librechat:v0.8.8`, `mongo:8.0.20`, as its compose file
# runs them, without Docker: vendors.sh), its page in Chromium (chromium.sh). Its figure is the
# whole: the server, the database and the browser, each also measured apart. It reaches the fake
# model as a custom endpoint, over Chat Completions; the fake model plays chat turns (--chat).
#
# Its configuration is the vendor's template, `.env.example` of v0.8.8 (manifest.txt), copied as
# its install documents, with these changes, each for a reason (librechat_setup):
# - CREDS_KEY, CREDS_IV, JWT_SECRET and JWT_REFRESH_SECRET: the template leaves them empty, and its
#   install asks for them to be made; fixed test values here;
# - LIMIT_CONCURRENT_MESSAGES=false: the template allows two messages in flight per user, and bans
#   a user over it; the workload runs fifteen. A deviation the workload needs (ADR-0028);
# - librechat.yaml: one custom endpoint, the fake model, as its documentation shows one.
# Both programs run as the compose file runs them: as the user 1000, the API by its image's own
# command, MongoDB by `mongod --noauth`.
#
# Its first start, at prepare: its user made by its own script (`npm run create-user`, its email
# verified), signed in through the page, and the fake model chosen in the model menu, which new
# chats keep. A new chat is opened by the button of its side rail: its shortcut, Ctrl+Shift+O, is
# also Chromium's bookmark manager's, which opens a window of its own.
#
# Probed on 2026-11-11 (M1.6b2a): its three parts start in the sandbox, the page renders on the
# GPU in one window, and 15 chats stream at once.

# shellcheck source=../chromium.sh
source "$(dirname "${BASH_SOURCE[0]}")/../chromium.sh"
# shellcheck source=../vendors.sh
source "$(dirname "${BASH_SOURCE[0]}")/../vendors.sh"

FAKE_CHAT=1
LIBRECHAT_PORT=3080
LIBRECHAT_URL=http://127.0.0.1:$LIBRECHAT_PORT
LIBRECHAT_EMAIL=bench@example.com
LIBRECHAT_PASSWORD=bench-password
# The user the compose file runs its programs as.
LIBRECHAT_USER=(--uid 1000 --gid 1000)

subject_version() {
    printf '%s; %s; %s\n' "$(vendor_version librechat)" "$(vendor_version mongo)" "$(chromium_version)"
}

subject_binds() { :; }

subject_agents() { echo; }

subject_parts() { echo server db browser; }

# Its sign-in lasts seven days (REFRESH_TOKEN_EXPIRY in the template).
subject_session_days() { echo 7; }

subject_setup() {
    librechat_setup
}

# The configuration, librechat-default.sh's too.
librechat_setup() {
    local folder=$HOME/librechat
    mkdir -p "$folder"/{data-node,logs,uploads,images,data}
    sed -e 's/^CREDS_KEY=$/CREDS_KEY=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/' \
        -e 's/^CREDS_IV=$/CREDS_IV=0123456789abcdef0123456789abcdef/' \
        -e 's/^JWT_SECRET=$/JWT_SECRET=bench-jwt-secret/' \
        -e 's/^JWT_REFRESH_SECRET=$/JWT_REFRESH_SECRET=bench-jwt-refresh-secret/' \
        -e 's/^LIMIT_CONCURRENT_MESSAGES=true$/LIMIT_CONCURRENT_MESSAGES=false/' \
        "$(package_file librechat-env)" > "$folder/.env"
    cat > "$folder/librechat.yaml" <<YAML
version: 1.3.17
cache: true
endpoints:
  custom:
    - name: "Fake"
      apiKey: "fake"
      baseURL: "$FAKE_URL/v1"
      models:
        default: ["fake-model"]
        fetch: false
      titleConvo: true
      titleModel: "current_model"
YAML
}

# The database, the API once the database answers, then the browser once the API answers.
subject_start() {
    local folder=$HOME/librechat
    (
        in_part subject/db
        run_image mongo "${LIBRECHAT_USER[@]}" --bind "$folder/data-node" /data/db -- mongod --noauth
    ) > "$run_dir/db.log" 2>&1 &
    wait_for 60 listening 27017 || { echo "MongoDB did not answer"; return 1; }
    (
        in_part subject/server
        run_image librechat "${LIBRECHAT_USER[@]}" --bind "$folder/.env" /app/.env \
            --bind "$folder/librechat.yaml" /app/librechat.yaml --bind "$folder/logs" /app/logs \
            --bind "$folder/uploads" /app/uploads --bind "$folder/images" /app/client/public/images \
            --bind "$folder/data" /app/data -- docker-entrypoint.sh npm run backend
    ) > "$run_dir/server.log" 2>&1 &
    wait_for 300 served "$LIBRECHAT_URL/health" || { echo "LibreChat's server did not answer"; return 1; }
    in_part subject/browser
    start_chromium "$LIBRECHAT_URL/"
}

# Its user, by its own script, against the database the server uses.
create_user() {
    local folder=$HOME/librechat
    run_image librechat "${LIBRECHAT_USER[@]}" --bind "$folder/.env" /app/.env \
        --bind "$folder/librechat.yaml" /app/librechat.yaml -- \
        node config/create-user.js "$LIBRECHAT_EMAIL" Bench bench "$LIBRECHAT_PASSWORD" --email-verified=true
}

# Its first start, by the pointer and the keyboard, at the places its page has on the 3840 × 2160
# output; each step's screenshot is kept.
subject_prepare() {
    wait_for 300 served "$LIBRECHAT_URL/health" || return 1
    (create_user) > "$run_dir/create-user.log" 2>&1 || { echo "LibreChat's user was not made"; return 1; }
    sleep 10
    # Signed in: its email, its password.
    click 1920 1028
    input "type $LIBRECHAT_EMAIL"
    click 1900 1088
    input "type $LIBRECHAT_PASSWORD" "key Return"
    sleep 8
    shot prepare-1
    # The model menu: the endpoint Fake, then its model.
    click 448 24
    sleep 2
    click 440 286
    sleep 2
    click 740 304
    sleep 3
    shot prepare-2
    # LibreChat keeps the choice in the page's local storage, which Chromium writes to the profile
    # some seconds later.
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

# The threads, one a chat: LibreChat's new chat (the first button of its side rail), then the
# message box's focus (Shift+Escape).
launch() {
    local threads=$1
    shift
    local turns=("$@") n
    sleep 10
    for n in $(seq 0 $((threads - 1))); do
        typed_turn "${turns[$((n % ${#turns[@]}))]}" "move 24 68" "sleep 500" "click" "sleep 1500" \
            "key shift+Escape" "sleep 300" || return 1
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
