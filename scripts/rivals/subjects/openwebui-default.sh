# shellcheck shell=bash
# Open WebUI in its default deployment, measured at rest only, beside its lean row
# (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md): its standard image, which loads
# PyTorch and its local embedding model at start, the model downloaded at prepare. Everything else
# is openwebui.sh's, with its changes.

# shellcheck source=openwebui.sh
source "$(dirname "${BASH_SOURCE[0]}")/openwebui.sh"

OPENWEBUI_IMAGE=open-webui

subject_stream() {
    echo "the default deployment is measured at rest only (ADR-0028)"
    return 1
}
