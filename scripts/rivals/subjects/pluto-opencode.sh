# shellcheck shell=bash
# Pluto with OpenCode as its agent: pluto.sh's subject, its threads on the fork's `opencode`
# instance, which starts `opencode serve`. OpenCode reads the file OPENCODE_CONFIG names, as the
# fork's own Ollama companion has it (the fork's provider-ollama/src/server/codeModels.ts): the
# provider id `ollama`, whose package is compiled into OpenCode (an id of the bench's own would
# hang on a package install), through AI SDK's OpenAI-compatible provider, so the fake model is
# reached over Chat Completions. No catalog is fetched (OPENCODE_DISABLE_MODELS_FETCH).

# shellcheck source=pluto.sh
source "$(dirname "${BASH_SOURCE[0]}")/pluto.sh"

PLUTO_INSTANCE=opencode
PLUTO_MODEL=ollama/fake-model

subject_version() {
    printf 'client sha256 %s; server %s; opencode %s\n' "$(sha256sum "$repo/target/release/pluto" | cut -c1-16)" \
        "$(package_of pluto-server)" "$(package_of opencode)"
}

subject_setup() {
    pluto_setup
    cat > "$HOME/opencode.json" <<JSON
{
  "\$schema": "https://opencode.ai/config.json",
  "enabled_providers": ["ollama"],
  "provider": {
    "ollama": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Fake",
      "options": {"baseURL": "$FAKE_URL/v1", "apiKey": "fake"},
      "models": {
        "fake-model": {"name": "Fake", "tool_call": true, "limit": {"context": 200000, "output": 32000}}
      }
    }
  },
  "model": "ollama/fake-model"
}
JSON
    local settings=$HOME/pluto/userdata/settings.json
    jq --arg config "$HOME/opencode.json" '.providerInstances.opencode = {
        driver: "opencode", enabled: true,
        environment: [
          {name: "OPENCODE_CONFIG", value: $config},
          {name: "OPENCODE_DISABLE_MODELS_FETCH", value: "1"}
        ]}' "$settings" > "$settings.tmp" && mv "$settings.tmp" "$settings"
}
