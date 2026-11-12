# shellcheck shell=bash
# The vendors' packages and images, as manifest.txt lists them and fetch.sh unpacks them, for the
# subjects' files that run them: where each is, its version, and how an image's program runs
# without a container runtime (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md).

RIVALS=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# An image's folder, unpacked by fetch.sh, by its name in the manifest.
image_dir() {
    local digest
    digest=$(awk -v name="$1" '!/^#/ && $1 == name { print $5 }' "$RIVALS/manifest.txt")
    echo "$repo/target/rivals/images/sha256-${digest#sha256:}"
}

# A package's folder, an AppImage or an archive unpacked by fetch.sh, by its name in the manifest.
package_dir() {
    local version
    version=$(awk -v name="$1" '!/^#/ && $1 == name { print $3 }' "$RIVALS/manifest.txt")
    echo "$repo/target/rivals/packages/$1-$version/unpacked"
}

# A file of the manifest (a vendor's template), by its name.
package_file() {
    awk -v name="$1" -v repo="$repo" '!/^#/ && $1 == name {
        n = split($4, parts, "/"); print repo "/target/rivals/packages/" $1 "-" $3 "/" parts[n] }' "$RIVALS/manifest.txt"
}

# A package's or an image's name, version and the start of its digest, for a subject's version.
vendor_version() {
    awk -v name="$1" '!/^#/ && $1 == name { printf "%s %s (%s)", $1, $3, substr($5, 1, 23) }' "$RIVALS/manifest.txt"
}

# Runs a program of a vendor's image as a container would, without a container runtime
# (ADR-0028): in bubblewrap, as the image's user, its files seen through a throwaway layer (what it
# writes there vanishes with it), in the run's network namespace and in the caller's cgroup. Its
# environment is the image's own, then the variables given; its data is what the options bind from
# the subject's home. Without a program, it runs the image's own command. It is exec'd: call it in
# a subshell that has entered its part.
# usage: run_image <name> [bubblewrap options...] -- [NAME=value...] [<program> [arguments...]]
run_image() {
    local dir options=() environment=() variable command=() user
    dir=$(image_dir "$1")
    shift
    [ -f "$dir/config.json" ] || { echo "no image $dir: scripts/rivals/fetch.sh" >&2; return 1; }
    while [ "$#" -gt 0 ] && [ "$1" != -- ]; do options+=("$1"); shift; done
    shift
    while read -r variable; do
        environment+=(--setenv "${variable%%=*}" "${variable#*=}")
    done < <(jq -r '.process.env[]?' "$dir/config.json")
    while [ "$#" -gt 0 ] && [[ "$1" == [A-Za-z_]*=* ]]; do
        environment+=(--setenv "${1%%=*}" "${1#*=}")
        shift
    done
    [ "$#" -gt 0 ] || mapfile -t -d '' command < <(jq -j '.process.args[] | ., "\u0000"' "$dir/config.json")
    [ "$#" -gt 0 ] || set -- "${command[@]}"
    # The image's own user, unless the options name another (`--uid N --gid N`), as a compose file's
    # `user:` does.
    user=(--uid "$(jq -r '.process.user.uid // 0' "$dir/config.json")" --gid "$(jq -r '.process.user.gid // 0' "$dir/config.json")")
    [[ " ${options[*]} " == *" --uid "* ]] && user=()
    exec bwrap --unshare-user "${user[@]}" --unshare-pid --unshare-ipc --die-with-parent \
        --overlay-src "$dir/rootfs" --tmp-overlay / --proc /proc --dev /dev --tmpfs /tmp --ro-bind /sys /sys \
        --ro-bind /etc/hosts /etc/hosts --ro-bind /etc/resolv.conf /etc/resolv.conf \
        --clearenv "${environment[@]}" --chdir "$(jq -r '.process.cwd // "/"' "$dir/config.json")" \
        "${options[@]}" -- "$@"
}
