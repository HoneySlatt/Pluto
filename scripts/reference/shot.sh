#!/usr/bin/env bash
# Screenshots the upstream web client of a fixture server (see fixture-server.sh).
# usage: scripts/reference/shot.sh <out.png> [path] [js] [root] [port]
# Set ELECTRON to an Electron binary; pairs and skips onboarding on first use.
set -euo pipefail
out=$1
route=${2:-/}
js=${3:-}
root=${4:-/tmp/t3-egui-fixture}
port=${5:-3999}
here=$(cd "$(dirname "$0")" && pwd)
electron=${ELECTRON:-electron}
run() { env -u ELECTRON_RUN_AS_NODE "$electron" "$here/main.js" "$@" 2>/dev/null; }

marker="$root/.reference-paired"
if [ ! -f "$marker" ]; then
  token=$(t3 auth pairing create --base-dir "$root/home" --ttl 10m --json 2>/dev/null | node -e \
    'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).credential))')
  run "http://127.0.0.1:$port/pair#token=$token" /dev/null 8000 \
    '(()=>{const k="t3code:client-settings:v1";const c=JSON.parse(localStorage.getItem(k)||"{}");c.onboardingCompletedAt=new Date().toISOString();localStorage.setItem(k,JSON.stringify(c));})()' >/dev/null
  touch "$marker"
fi
run "http://127.0.0.1:$port$route" "$out" 6000 "$js"
