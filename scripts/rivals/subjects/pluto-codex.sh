# shellcheck shell=bash
# Pluto with Codex as its agent: pluto.sh's subject, its threads on the fork's `codex` instance.
# Codex reads its provider from its home's config.toml, which pluto.sh writes for the titles: the
# fake model over OpenAI's Responses. Its model is any name; the fake provider answers every name.

# shellcheck source=pluto.sh
source "$(dirname "${BASH_SOURCE[0]}")/pluto.sh"

PLUTO_INSTANCE=codex
PLUTO_MODEL=fake-model

subject_version() {
    printf 'client sha256 %s; server %s; codex %s\n' "$(sha256sum "$repo/target/release/pluto" | cut -c1-16)" \
        "$(package_of pluto-server)" "$(package_of codex)"
}
