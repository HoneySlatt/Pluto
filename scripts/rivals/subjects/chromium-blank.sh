# shellcheck shell=bash
# The web rivals' control: the same Chromium, with the same switches and in the same mode as theirs
# (scripts/rivals/chromium.sh), on an empty local page. A web rival's figure less this one
# approaches its cost in a browser already open
# (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md). It has no thread: it is measured
# as it rests, in every scenario.

# shellcheck source=../chromium.sh
source "$(dirname "${BASH_SOURCE[0]}")/../chromium.sh"

subject_version() { chromium_version; }

subject_binds() { :; }

subject_agents() { echo; }

subject_parts() { :; }

# No answer streams in it: the stream scenario's check expects none.
subject_threads() { echo 0; }

subject_setup() {
    printf '<!doctype html>\n<title>Blank</title>\n' > "$HOME/blank.html"
}

subject_start() {
    start_chromium "file://$HOME/blank.html"
}

subject_open() { sleep 10; }

subject_stream() { sleep 10; }

subject_stop() {
    local pid
    pid=$(cat "$run_dir/subject.pid" 2>/dev/null) || return 0
    pkill -TERM -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
}
