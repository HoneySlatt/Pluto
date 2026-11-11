#!/usr/bin/env bash
# The rivals' bench: one run of one subject, inside its sandbox (docs/adr/0026-the-rivals-measured-how.md).
# run.sh starts it in a systemd scope of its own with Delegate=yes; it never runs alone.
#
# Three steps, each re-entering this script:
#   netns   in a new user and network namespace: the loopback up, and a pair of virtual interfaces
#           with no address and no route, so that Chromium counts the machine online
#           (address_tracker_linux.cc) while nothing can leave it; then bubblewrap, back to the
#           owner's uid, sees the Nix store, the GPU and the run's folder, not /home.
#   inside  in bubblewrap: the cgroups of the run's parts, a session bus, a headless sway on the
#           GPU, the fake model, then the subject and its driver.
#
# usage: sandbox.sh netns <run-dir> <subject> <scenario>
# The run's folder holds, on the way out: fake.jsonl (the fake model's journal), compositor.log,
# subject.log, driver.log, and the phase files run.sh waits on (ready, done).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

step=$1
run_dir=$2
subject=$3
scenario=$4

# The parts of a run, each a cgroup under the run's scope: the subject is what is measured; the
# others are counted apart so that they never weigh on its figures.
parts=(bench compositor fake driver subject)

case "$step" in
netns)
    scope=$(sed -n 's/^0:://p' /proc/self/cgroup)
    # The bench's own processes leave the scope's root, which then only holds its parts: cgroup v2
    # lets a cgroup with controllers enabled below it hold no process of its own.
    cg=/sys/fs/cgroup$scope
    for part in "${parts[@]}"; do mkdir -p "$cg/$part"; done
    # A subject may split itself further (Pluto: its client and its server), each measured apart.
    # shellcheck source=/dev/null
    source "$here/subjects/$subject.sh"
    for part in $(subject_parts); do mkdir -p "$cg/subject/$part"; done
    echo $$ > "$cg/bench/cgroup.procs"
    echo "+memory +cpu +pids" > "$cg/cgroup.subtree_control"
    exec unshare --user --map-root-user --net -- "$0" in-netns "$run_dir" "$subject" "$scenario" \
        "$(id -u)" "$(id -g)" "$cg"
    ;;
in-netns)
    uid=$5 gid=$6 cg=$7
    ip link set lo up
    ip link add pluto0 type veth peer name pluto1
    ip link set pluto0 up
    ip link set pluto1 up
    binds=(--ro-bind /nix /nix --ro-bind /etc /etc --ro-bind /usr /usr --ro-bind /bin /bin
        --ro-bind /run/current-system /run/current-system --ro-bind /run/opengl-driver /run/opengl-driver
        --dev /dev --dev-bind /dev/dri /dev/dri --proc /proc --ro-bind /sys /sys --bind "$cg" "$cg"
        --tmpfs /home --tmpfs /tmp --ro-bind "$repo" "$repo" --bind "$run_dir" "$run_dir")
    # Read-only extras a subject needs (its package outside the store, the client's build), from
    # its file's `subject_binds`.
    # shellcheck source=/dev/null
    source "$here/subjects/$subject.sh"
    while read -r path; do
        [ -n "$path" ] && binds+=(--ro-bind "$path" "$path")
    done < <(subject_binds)
    exec bwrap --unshare-user --uid "$uid" --gid "$gid" "${binds[@]}" \
        --dir /home/subject --setenv HOME /home/subject \
        --dir /tmp/run --chmod 0700 /tmp/run --setenv XDG_RUNTIME_DIR /tmp/run \
        --setenv XDG_CONFIG_HOME /home/subject/.config --setenv XDG_CACHE_HOME /home/subject/.cache \
        --setenv XDG_DATA_HOME /home/subject/.local/share --setenv XDG_STATE_HOME /home/subject/.local/state \
        --setenv XDG_SESSION_TYPE wayland --setenv NIXOS_OZONE_WL 1 \
        --unsetenv WAYLAND_DISPLAY --unsetenv DISPLAY --unsetenv DBUS_SESSION_BUS_ADDRESS \
        --unsetenv AT_SPI_BUS_ADDRESS --unsetenv SSH_AUTH_SOCK --unsetenv GNUPGHOME \
        --setenv DBUS_SYSTEM_BUS_ADDRESS unix:path=/tmp/run/no-system-bus \
        --die-with-parent -- "$0" inside "$run_dir" "$subject" "$scenario" "$cg"
    ;;
inside)
    cg=$5
    in_part() { echo "$BASHPID" > "$cg/$1/cgroup.procs"; }
    # shellcheck source=/dev/null
    source "$here/subjects/$subject.sh"
    # shellcheck source=/dev/null
    source "$here/lib.sh"
    bench_inside "$run_dir" "$subject" "$scenario"
    ;;
*)
    echo "sandbox.sh: unknown step $step" >&2
    exit 2
    ;;
esac
