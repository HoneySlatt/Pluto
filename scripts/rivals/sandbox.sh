#!/usr/bin/env bash
# The rivals' bench: one run of one subject, inside its sandbox (docs/adr/0026-the-rivals-measured-how.md).
# run.sh starts it in a systemd scope of its own with Delegate=yes; it never runs alone.
#
# Three steps, each re-entering this script:
#   netns     in the run's scope: the cgroups of its parts, then a new user and network namespace,
#             with a route out through pasta when ROUTE_OUT=1 (`prepare` only), none otherwise;
#   in-netns  there: the loopback up, and a pair of virtual interfaces with no address and no
#             route, so that Chromium counts the machine online (address_tracker_linux.cc) while
#             nothing can leave it; then bubblewrap, back to the owner's uid, in a process
#             namespace of its own, with a cleared environment, seeing the Nix store, the GPU, the
#             run's folder and the bench's files, not /home;
#   inside    in bubblewrap: a session bus, a headless sway on the GPU, the fake model, then the
#             subject and its driver, each in its cgroup.
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

# With a route out, the address the sandbox sends its DNS queries to, which pasta forwards to the
# machine's own resolver: a link-local address, which nothing else in the sandbox uses.
PASTA_DNS=169.254.53.53

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
    if [ "${ROUTE_OUT:-0}" = 1 ]; then
        # A profile prepared with the network (ADR-0028): pasta gives the namespace a route out
        # through its own user-mode stack, and nothing in: no port of the namespace is forwarded
        # from the machine (-t, -u), none of the machine's into the namespace (-T, -U), and no
        # address of the namespace stands for the machine's loopback or its own address, so that a
        # subject's 127.0.0.1 is its own and it never meets the owner's services (the fork on
        # 3773, an Ollama on 11434).
        exec pasta --quiet --config-net -t none -u none -T none -U none \
            --map-host-loopback none --map-guest-addr none \
            --dns-forward "$PASTA_DNS" -- "$0" in-netns "$run_dir" "$subject" "$scenario" \
            "$(id -u)" "$(id -g)" "$cg"
    fi
    exec unshare --user --map-root-user --net -- "$0" in-netns "$run_dir" "$subject" "$scenario" \
        "$(id -u)" "$(id -g)" "$cg"
    ;;
in-netns)
    uid=$5 gid=$6 cg=$7
    ip link set lo up
    ip link add pluto0 type veth peer name pluto1
    ip link set pluto0 up
    ip link set pluto1 up
    # What the subject sees: the system's read-only parts, the GPU, its cgroups, its run's folder,
    # and of the repository only the bench, the three programs it runs, the vendors' packages and the
    # subjects' frozen profiles.
    binds=(--ro-bind /nix /nix --ro-bind /etc /etc --ro-bind /usr /usr --ro-bind /bin /bin
        --ro-bind /run/current-system /run/current-system --ro-bind /run/opengl-driver /run/opengl-driver
        --dev /dev --dev-bind /dev/dri /dev/dri --proc /proc --ro-bind /sys /sys --bind "$cg" "$cg"
        --tmpfs /home --tmpfs /tmp --bind "$run_dir" "$run_dir" --ro-bind "$here" "$here"
        --ro-bind "$repo/crates/pluto-fake-model/scripts" "$repo/crates/pluto-fake-model/scripts"
        --ro-bind-try "$repo/target/release/pluto" "$repo/target/release/pluto"
        --ro-bind-try "$repo/target/release/pluto-fake-model" "$repo/target/release/pluto-fake-model"
        --ro-bind-try "$repo/target/release/pluto-input" "$repo/target/release/pluto-input"
        --ro-bind-try "$repo/target/release/examples/drive" "$repo/target/release/examples/drive"
        --ro-bind-try "$repo/target/rivals/packages" "$repo/target/rivals/packages"
        --ro-bind-try "$repo/target/rivals/images" "$repo/target/rivals/images"
        --ro-bind-try "$repo/target/rivals/profiles" "$repo/target/rivals/profiles")
    # The sandbox's own users: the subject's login shell is /bin/sh in its own home, never the
    # owner's shell and profile, which a rival that reads its login shell's environment (OpenCode
    # Desktop, Nimbalyst) would run, with the owner's own versions of the agents' CLIs.
    printf 'root:x:0:0::/root:/bin/sh\nsubject:x:%s:%s:subject:/home/subject:/bin/sh\nnobody:x:65534:65534::/var/empty:/bin/sh\n' \
        "$uid" "$gid" > "$run_dir/passwd"
    printf 'root:x:0:\nsubject:x:%s:\nnogroup:x:65534:\n' "$gid" > "$run_dir/group"
    binds+=(--ro-bind "$run_dir/passwd" /etc/passwd --ro-bind "$run_dir/group" /etc/group)
    # With a route out, names are resolved through pasta's forwarder; offline, a name resolves to
    # nothing at once, as for a user without a network.
    if [ "${ROUTE_OUT:-0}" = 1 ]; then
        printf 'nameserver %s\n' "$PASTA_DNS" > "$run_dir/resolv.conf"
        binds+=(--ro-bind "$run_dir/resolv.conf" /etc/resolv.conf)
    fi
    # Read-only extras a subject needs (its package outside the store), from its file's
    # `subject_binds`.
    # shellcheck source=/dev/null
    source "$here/subjects/$subject.sh"
    while read -r path; do
        [ -n "$path" ] && binds+=(--ro-bind "$path" "$path")
    done < <(subject_binds)
    # A cleared environment, then only what a run needs: the owner's variables (tokens, sockets,
    # paths) never reach the subject.
    # LD_LIBRARY_PATH is the measure shell's: the libraries eframe loads at runtime (Wayland, Vulkan).
    env=(--setenv PATH "$PATH" --setenv LD_LIBRARY_PATH "${LD_LIBRARY_PATH:-}" --setenv LANG "${LANG:-C.UTF-8}"
        --setenv LOCALE_ARCHIVE "${LOCALE_ARCHIVE:-}" --setenv XDG_DATA_DIRS "${XDG_DATA_DIRS:-}"
        --setenv HOME /home/subject --setenv XDG_RUNTIME_DIR /tmp/run
        --setenv XDG_CONFIG_HOME /home/subject/.config --setenv XDG_CACHE_HOME /home/subject/.cache
        --setenv XDG_DATA_HOME /home/subject/.local/share --setenv XDG_STATE_HOME /home/subject/.local/state
        --setenv XDG_SESSION_TYPE wayland --setenv NIXOS_OZONE_WL 1 --setenv BROWSER false
        --setenv SHELL /bin/sh --setenv USER subject --setenv LOGNAME subject
        --setenv DBUS_SYSTEM_BUS_ADDRESS unix:path=/tmp/run/no-system-bus
        --setenv PLUTO_FAKE_MODEL "$PLUTO_FAKE_MODEL" --setenv PLUTO_FAKE_SCRIPT "$PLUTO_FAKE_SCRIPT"
        --setenv PLUTO_INPUT "$PLUTO_INPUT")
    for name in THREADS FAKE_SPEED FRAMES PROBE_WAIT PROBE_INPUT PROBE_CLICK PROBE_CONSOLE BENCH_MAX_WAIT ROUTE_OUT; do
        [ -n "${!name:-}" ] && env+=(--setenv "$name" "${!name}")
    done
    # Its own process namespace: /proc shows the run's processes only, and its first process
    # ending stops every other one, the bus and the compositor among them. A new session: no
    # input injected into the caller's terminal.
    exec bwrap --unshare-user --uid "$uid" --gid "$gid" --unshare-pid --unshare-ipc --new-session \
        --clearenv "${env[@]}" "${binds[@]}" \
        --dir /home/subject --dir /tmp/run --chmod 0700 /tmp/run \
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
