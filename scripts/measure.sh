#!/usr/bin/env bash
# Takes the figures of docs/measurements.md that need the client running against the fixture:
#
# - the bench (examples/bench.rs): each frame's CPU cost, the 5,000-message thread, 15 streams;
# - the start, warm and cold, to the first useful frame (examples/live.rs, hyperfine);
# - at rest, the 15 "Flux" threads and the long one opened: the frames drawn, the resident memory
#   and the wake-ups of each thread (pidstat);
# - the heap at rest, split by what holds it (heaptrack);
# - the fork serving the fixture, at rest, before the client and after it: its memory and its
#   wake-ups.
#
# Run it in `nix develop .#measure`, with scripts/fixture-server.sh running. The client runs in a
# headless weston, with a throwaway home, configuration and runtime directory, a session bus of its
# own (the system's dbus-daemon) and without DISPLAY, so that it reaches neither the owner's
# session nor the owner's files; it blocks turns.
#
# usage: scripts/measure.sh [output-dir]           (default /tmp/pluto-measure)
# env:   SERVER (http://127.0.0.1:3999), TOKEN (/tmp/t3-egui-fixture/bearer), REST (60 seconds),
#        FIXTURE (/tmp/t3-egui-fixture, the fork's base directory is its home/),
#        SKIP_HEAP=1 leaves heaptrack out: it needs a second build, with line tables (on the HP).
set -euo pipefail
cd "$(dirname "$0")/.."

out=${1:-/tmp/pluto-measure}
server=${SERVER:-http://127.0.0.1:3999}
token=${TOKEN:-/tmp/t3-egui-fixture/bearer}
rest=${REST:-60}
fixture=${FIXTURE:-/tmp/t3-egui-fixture}
mkdir -p "$out"

step() { printf '\n== %s\n' "$1"; }

# Sums pidstat's per-thread averages: voluntary and involuntary context switches a second, each
# one a wake-up, then the threads that woke, busiest first.
wakeups() {
  awk '/^Average:/ && $3 == "-" && $4 ~ /^[0-9]+$/ {
         vol += $5; invol += $6; if ($5 + $6 > 0) print "  " $5 + $6 "/s  " $NF | "sort -rn" }
       END { printf "wake-ups: %.2f a second (%.2f voluntary)\n", vol + invol, vol }' "$1"
}

# Splits heaptrack's stacks of the memory still held at exit, the heap at rest, by what holds it.
# heaptrack's names often lose their module path, so the patterns also match source file names;
# every stack starts in eframe's loop (`run_wgpu`), so the GPU's patterns name wgpu's and the
# driver's internals only. The first match wins: the GPU's own allocations before images, text
# layout before fonts.
heap_split() {
  awk '{
    cost = $NF; stack = $0; sub(/ [0-9]+$/, "", stack); kind = "the rest"
    if (stack ~ /wgpu_core|wgpu_hal|naga|vk[A-Z]|radv|amdgpu|mesa|libdrm|nir_|LLVM|gpu_alloc|egl[A-Z]|begin_encoding|command_encoder|prepare_surface_texture|device\.rs|queue\.rs/)
      kind = "the GPU driver and wgpu (their CPU side)"
    else if (stack ~ /text_layout|[Gg]alley|layout_job|LayoutJob/)
      kind = "text laid out"
    else if (stack ~ /fonts?\.rs|install_fonts|FontDefinitions|ab_glyph|skrifa|ttf|fontdb|fontconfig|texture_atlas/)
      kind = "fonts, their files and the glyph atlas"
    else if (stack ~ /ColorImage|ImageDelta|tex_manager|image::|resvg|tiny_skia|usvg|png|decoder_to_vec|zune|jpeg|svg/)
      kind = "images and icons"
    else if (stack ~ /syntect|highlight\.rs|into_nfa|build_from_nfa|regex|syntax/)
      kind = "syntax highlighting"
    else if (stack ~ /model\.rs|timeline|pluto_core|serde_json|protocol\.rs|rows|markdown|md_[a-z]+\.rs|chat\.rs/)
      kind = "the timeline and the server'"'"'s state"
    held[kind] += cost; sum += cost
  }
  END {
    for (k in held) printf "%7.2f MiB  %4.1f %%  %s\n", held[k] / 1048576, 100 * held[k] / sum, k
    printf "%7.2f MiB         the heap at rest, in all\n", sum / 1048576
  }' "$1" | sort -rn
}

# Reads the fork's memory, its children's and its wake-ups over 30 s, from /proc and pidstat only,
# into $out/fork-$1.txt and $out/fork-$1-pidstat.txt.
fork_at_rest() {
  local fork
  fork=$(pgrep -f "[s]erve --base-dir $fixture/home" | head -1)
  grep -E '^(VmRSS|VmHWM|Threads)' "/proc/$fork/status" | tee "$out/fork-$1.txt"
  for child in $(pgrep -P "$fork"); do
    echo "child $(cat "/proc/$child/comm"): $(grep VmRSS "/proc/$child/status")" |
      tee -a "$out/fork-$1.txt"
  done
  LC_ALL=C pidstat -w -t -p "$fork" 1 30 > "$out/fork-$1-pidstat.txt"
  wakeups "$out/fork-$1-pidstat.txt"
}

step "The fork at rest, before the client"
fork_at_rest before

step "Build"
cargo build --release --locked --example bench --example live
live=$PWD/target/release/examples/live
if [ -z "${SKIP_HEAP:-}" ]; then
  # Line tables let heaptrack name inlined frames; the binary is otherwise the same.
  CARGO_TARGET_DIR=$PWD/target/measure-heap CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
    cargo build --release --locked --example live
fi

# From here on the client sees only the sandbox (the build above needed the real HOME): its own
# home, runtime directory, compositor and session bus.
sandbox=$(mktemp -d)
weston_pid=
bus_pid=
cleanup() {
  [ -n "$weston_pid" ] && kill "$weston_pid" 2>/dev/null || true
  [ -n "$bus_pid" ] && kill "$bus_pid" 2>/dev/null || true
  rm -rf "$sandbox"
}
trap cleanup EXIT
mkdir -p "$sandbox/home/.cache" "$sandbox/run"
chmod 700 "$sandbox/run"
export HOME=$sandbox/home XDG_CONFIG_HOME=$sandbox/home/.config XDG_CACHE_HOME=$sandbox/home/.cache
export XDG_DATA_HOME=$sandbox/home/.local/share XDG_RUNTIME_DIR=$sandbox/run
export WAYLAND_DISPLAY=pluto-measure
unset DISPLAY AT_SPI_BUS_ADDRESS
DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address=1 --print-pid=3 \
  3> "$sandbox/bus.pid")
export DBUS_SESSION_BUS_ADDRESS
bus_pid=$(cat "$sandbox/bus.pid")

weston --backend=headless --renderer=gl --width=1920 --height=1080 --socket=pluto-measure \
  --idle-time=0 --no-config > "$out/weston.log" 2>&1 &
weston_pid=$!
for _ in $(seq 50); do
  [ -S "$XDG_RUNTIME_DIR/pluto-measure" ] && break
  sleep 0.1
done

# A first run fills the font and shader caches of the throwaway home, as the owner's are.
"$live" "$server" "$token" start > /dev/null

step "Bench"
target/release/examples/bench "$server" "$token" | tee "$out/bench.txt"

# Starts `live` in `rest` mode under `$1` (empty, or a profiler), and prints its pid once the rest
# begins. Its output goes to $out/$2.txt and $out/$2.err.
rest_start() {
  local wrapper=$1 name=$2
  $wrapper "${@:3}" "$server" "$token" rest "$rest" > "$out/$name.txt" 2> "$out/$name.err" &
  for _ in $(seq 1200); do
    if grep -q "resting, pid" "$out/$name.err" 2>/dev/null; then
      sed -n 's/.*resting, pid \([0-9]*\).*/\1/p' "$out/$name.err"
      return
    fi
    sleep 0.1
  done
  echo "the client did not reach its rest; see $out/$name.err" >&2
  exit 1
}

step "At rest: frames, memory, wake-ups ($rest s)"
pid=$(rest_start "" rest "$live")
LC_ALL=C pidstat -w -t -p "$pid" 1 $((rest - 2)) > "$out/rest-pidstat.txt"
# The files the client maps: what the cold start drops from the page cache.
grep -o ' /.*' "/proc/$pid/maps" | sed 's/^ //' | sort -u > "$out/client-files.txt"
while kill -0 "$pid" 2>/dev/null; do sleep 0.5; done
cat "$out/rest.txt"
wakeups "$out/rest-pidstat.txt"

# Starts the client ten times, `$1` run before each start, and prints the median of when the first
# useful frame was painted, after `main`.
starts() {
  for _ in $(seq 10); do
    sh -c "$1"
    "$live" "$server" "$token" start
  done | tee "$out/$2.txt" |
    sed -n 's/.*painted \([0-9.]*\) ms.*/\1/p' | sort -n |
    awk '{ v[NR] = $1 } END { printf "first useful frame painted, median: %.1f ms after main\n", v[int((NR + 1) / 2)] }'
}

# The files the client mapped at rest, dropped from the page cache; those other processes map stay
# cached, as after a reboot into a running desktop.
drop="while read -r f; do [ -f \"\$f\" ] && dd if=\"\$f\" iflag=nocache count=0 status=none || true; done < $out/client-files.txt"

step "Start, warm"
# Before main: the binary loaded and linked (`live` without arguments exits at once).
hyperfine -N --ignore-failure --warmup 3 --runs 30 --export-json "$out/start-exec-warm.json" "$live"
starts true start-warm

step "Start, cold: the client's files dropped from the page cache"
hyperfine -N --ignore-failure --runs 10 --prepare "$drop" --export-json "$out/start-exec-cold.json" \
  "$live"
starts "$drop" start-cold

if [ -z "${SKIP_HEAP:-}" ]; then
  step "The heap at rest, by what holds it (heaptrack)"
  rm -f "$out"/heap.*
  rest=20
  pid=$(rest_start "heaptrack --record-only -o $out/heap" heap \
    "$PWD/target/measure-heap/release/examples/live")
  # heaptrack writes its file after the client exits: wait for heaptrack itself.
  while pgrep -f "[h]eaptrack --record-only -o $out/heap" > /dev/null; do sleep 0.5; done
  heaptrack_print -f "$out"/heap.*zst --print-flamegraph "$out/heap-stacks.txt" \
    --flamegraph-cost-type leaked > "$out/heap-print.txt"
  heap_split "$out/heap-stacks.txt" | tee "$out/heap-split.txt"
fi

step "The fork after serving the measurements"
fork_at_rest after
echo
echo "figures in $out"
