# shellcheck shell=bash
# The browser of the web rivals and of their control, `chromium-blank`
# (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md): nixpkgs' Chromium in `--app` mode,
# one window, its profile in the subject's home, frozen with it. Sourced by their subjects' files.
#
# Only the switches that stop Chromium's own background traffic and its first-run pages, each for a
# reason; none changes how it schedules, places memory or renders. nixpkgs' wrapper adds its own
# for Wayland when NIXOS_OZONE_WL is set, as the sandbox sets it.
# shellcheck disable=SC2054 # the features' list is one switch, its commas Chromium's
CHROMIUM_SWITCHES=(
    # No first-run page, no question about the default browser, no default apps installed.
    --no-first-run
    --no-default-browser-check
    --disable-default-apps
    # No background traffic of its own: variations, the update and translation checks, the
    # components' updater (certificate lists, codecs), Google's sync.
    --disable-background-networking
    --disable-component-update
    --disable-sync
    # Saved passwords in the profile, never a prompt of the keyring.
    --password-store=basic
    # No search engine choice screen at first start.
    --disable-search-engine-choice-screen
    # No page hints fetched from Google, no network time queries, no AI mode in the omnibox, no
    # preconnection to the search engine, no casting's discovery on the network.
    --disable-features=OptimizationHints,NetworkTimeServiceQuerying,AimEnabled,PreconnectToSearch,MediaRouter
)

# Starts Chromium on a page, as an application's window; it must be the subject's main program, so
# it is exec'd. Its command line is kept in the run's folder, for the fingerprint.
start_chromium() {
    local command=(chromium --user-data-dir="$HOME/chromium" "${CHROMIUM_SWITCHES[@]}" --app="$1")
    printf '%q ' "${command[@]}" > "$run_dir/chromium-command.txt"
    exec "${command[@]}"
}

# Chromium's version, from its store path: never by running it.
chromium_version() {
    basename "$(dirname "$(dirname "$(readlink -f "$(command -v chromium)")")")"
}
