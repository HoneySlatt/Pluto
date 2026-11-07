#!/usr/bin/env bash
# Starts a disposable Pluto server seeded with synthetic data.
# Never point this at a real T3 data directory: it is wiped on every run.
#
# usage: scripts/fixture-server.sh [root] [port]
# Prints the bearer token file and the server URL once ready.
#
# The seed writes protocol 1 events, which only a v0.0.45 server projects:
# that server builds the fixture, then the installed v0.1.0 server imports it
# (as it imports any v0.0.45 data directory) and serves it. T3_V1 names the
# v0.0.45 `t3`; by default it is the flake's `fixture-t3` package, built once
# (a download of about 3.5 GB) and kept from garbage collection by the link
# target/fixture-t3. PLUTO_SERVER names the Pluto server (default:
# `pluto-server` on PATH).
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
# web-demo has a remote on a reserved host (never resolvable, so no source
# control lookup leaves the machine) and a package directory, so its
# projects share one repository identity.
git -C "$root/web-demo" remote add origin https://git.example.invalid/example/web-demo.git
mkdir -p "$root/web-demo/packages/ui"
echo "# ui" > "$root/web-demo/packages/ui/README.md"
git -C "$root/web-demo" add .
git -C "$root/web-demo" -c user.name=fixture -c user.email=fixture@example.invalid commit -qm ui

if [ -z "${T3_V1:-}" ]; then
  nix build "$here/..#fixture-t3" --out-link "$here/../target/fixture-t3"
fi
v1=${T3_V1:-$here/../target/fixture-t3/bin/t3}
if [ ! -x "$v1" ]; then
  echo "no v0.0.45 t3 at $v1" >&2
  exit 1
fi

serve() {
  "$1" serve --base-dir "$root/home" --port "$port" --host 127.0.0.1 "$root/demo-project" \
    > "$root/serve.log" 2>&1 &
  # Projecting the seeded events, the 5,000-message thread among them, takes
  # the v0.0.45 server well over the half-minute it needed before.
  for _ in $(seq 600); do
    grep -q "server is ready" "$root/serve.log" 2>/dev/null && return 0
    sleep 0.5
  done
  echo "server did not start; see $root/serve.log" >&2
  return 1
}

# First start runs the migrations; seeding needs the schema. The second
# projects the seeded events; the third imports them into protocol 2; the
# fourth serves them.
serve "$v1"
pkill -f "serve --base-dir $root/home"
sleep 1
node "$here/seed-fixture.mjs" "$root/home" "$root"
serve "$v1"
pkill -f "serve --base-dir $root/home"
sleep 1
pluto=${PLUTO_SERVER:-pluto-server}
serve "$pluto"
# The import's memory stays with the process that ran it, about 300 MB: a
# restart serves the fixture as a server does day to day (docs/measurements.md).
pkill -f "serve --base-dir $root/home"
sleep 1
serve "$pluto"

"$pluto" auth session issue --base-dir "$root/home" --ttl 30d --label egui-fixture --token-only \
  > "$root/bearer" 2>/dev/null
echo "url:    http://127.0.0.1:$port"
echo "bearer: $root/bearer"
