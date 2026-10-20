#!/usr/bin/env bash
# Starts a disposable T3 Code server seeded with synthetic data.
# Never point this at a real T3 data directory: it is wiped on every run.
#
# usage: scripts/fixture-server.sh [root] [port]
# Prints the bearer token file and the server URL once ready.
set -euo pipefail

root=${1:-/tmp/t3-egui-fixture}
port=${2:-3999}
here=$(cd "$(dirname "$0")" && pwd)

pkill -f "serve --base-dir $root/home" 2>/dev/null || true
sleep 1
rm -rf "$root"
mkdir -p "$root"

for dir in demo-project nixos-demo web-demo; do
  mkdir -p "$root/$dir"
  git -C "$root/$dir" init -q -b main
  echo "# $dir" > "$root/$dir/README.md"
  git -C "$root/$dir" add .
  git -C "$root/$dir" -c user.name=fixture -c user.email=fixture@example.invalid commit -qm init
done

serve() {
  t3 serve --base-dir "$root/home" --port "$port" --host 127.0.0.1 "$root/demo-project" \
    > "$root/serve.log" 2>&1 &
  for _ in $(seq 60); do
    grep -q "server is ready" "$root/serve.log" 2>/dev/null && return 0
    sleep 0.5
  done
  echo "server did not start; see $root/serve.log" >&2
  return 1
}

# First start runs the migrations; seeding needs the schema.
serve
pkill -f "serve --base-dir $root/home"
sleep 1
node "$here/seed-fixture.mjs" "$root/home" "$root"
serve

t3 auth session issue --base-dir "$root/home" --ttl 30d --label egui-fixture --token-only \
  > "$root/bearer" 2>/dev/null
echo "url:    http://127.0.0.1:$port"
echo "bearer: $root/bearer"
