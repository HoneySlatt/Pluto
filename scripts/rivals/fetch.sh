#!/usr/bin/env bash
# Fetches the vendors' packages and images the rivals' bench runs, by their line in manifest.txt,
# each once, into a folder of its own under target/rivals/, checked against its digest
# (scripts/rivals/README.md). Run it in `nix develop .#measure`, by hand, with the owner's go for
# each download (docs/adr/0026-the-rivals-measured-how.md, docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md).
#
# usage: scripts/rivals/fetch.sh <name>...
#
# - appimage: target/rivals/packages/<name>-<version>/, the file, its SHA256, and `unpacked/`
#   (appimage-run -x);
# - archive: the same, the .tar.gz unpacked into `unpacked/`;
# - file: the same, the file alone (a vendor's configuration template);
# - image: copied by skopeo for linux/amd64 into an OCI layout, unpacked by umoci (rootless) into
#   target/rivals/images/sha256-<digest>/ (its `config.json` and `rootfs/`), the layout then
#   removed. An image whose digest is `-` in the manifest is copied by its tag, and its digest
#   printed, to be written into the manifest.
#
# A package already there, with its digest, is left alone. A download whose digest differs from
# the manifest's is removed, and the script fails.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
manifest=$here/manifest.txt

# A name's line in the manifest: kind, version, source, digest.
line_of() {
    awk -v name="$1" '!/^#/ && $1 == name { print $2, $3, $4, $5; found = 1 } END { exit !found }' "$manifest"
}

fetch_file() {
    local name=$1 kind=$2 version=$3 source=$4 digest=$5
    local folder=$repo/target/rivals/packages/$name-$version file
    file=$(basename "$source")
    if [ -f "$folder/SHA256" ] && [ "sha256:$(cut -d' ' -f1 "$folder/SHA256")" = "$digest" ] &&
        { [ "$kind" = file ] || [ -d "$folder/unpacked" ]; }; then
        echo "$name $version: already there"
        return 0
    fi
    # An empty folder of its own: what is downloaded is never mixed with anything else.
    rm -rf "$folder"
    mkdir -p "$folder"
    curl -fsSL --proto '=https' --retry 3 -o "$folder/$file.part" "$source"
    local got
    got=$(sha256sum "$folder/$file.part" | cut -d' ' -f1)
    if [ "sha256:$got" != "$digest" ]; then
        rm -rf "$folder"
        echo "$name: its digest is sha256:$got, not the manifest's $digest" >&2
        return 1
    fi
    mv "$folder/$file.part" "$folder/$file"
    echo "$got  $file" > "$folder/SHA256"
    case "$kind" in
    appimage) (cd "$folder" && appimage-run -x unpacked "$file") ;;
    archive) mkdir "$folder/unpacked" && tar -xzf "$folder/$file" -C "$folder/unpacked" ;;
    esac
    echo "$name $version: fetched in $folder"
}

fetch_image() {
    local name=$1 version=$2 source=$3 digest=$4
    local images=$repo/target/rivals/images
    if [ "$digest" != - ] && [ -f "$images/sha256-${digest#sha256:}/config.json" ]; then
        echo "$name $version: already there"
        return 0
    fi
    local reference=docker://$source
    # Pinned by its digest once known; by its tag the first time.
    [ "$digest" = - ] || reference=docker://${source%:*}@$digest
    local work
    mkdir -p "$images"
    work=$(mktemp -d "$images/.fetch-$name.XXXXXX")
    # The digest kept is the registry's: a manifest skopeo would have to convert fails instead.
    skopeo copy --preserve-digests --override-os linux --override-arch amd64 "$reference" "oci:$work/oci:image"
    local got
    got=$(jq -r '.manifests[0].digest' "$work/oci/index.json")
    if [ "$digest" != - ] && [ "$got" != "$digest" ]; then
        rm -rf "$work"
        echo "$name: its digest is $got, not the manifest's $digest" >&2
        return 1
    fi
    local folder=$images/sha256-${got#sha256:}
    rm -rf "$folder"
    umoci unpack --rootless --image "$work/oci:image" "$folder"
    rm -rf "$work"
    printf '%s %s %s\n' "$name" "$version" "$source" > "$folder/image.txt"
    echo "$name $version: unpacked in $folder"
    [ "$digest" != - ] || echo "$name: write its digest into the manifest: $got"
}

[ "$#" -gt 0 ] || { echo "usage: scripts/rivals/fetch.sh <name>..." >&2; exit 2; }
for name in "$@"; do
    read -r kind version source digest < <(line_of "$name") || { echo "$name is not in $manifest" >&2; exit 1; }
    case "$kind" in
    appimage | archive | file) fetch_file "$name" "$kind" "$version" "$source" "$digest" ;;
    image) fetch_image "$name" "$version" "$source" "$digest" ;;
    *) echo "$name: unknown kind $kind" >&2; exit 1 ;;
    esac
done
