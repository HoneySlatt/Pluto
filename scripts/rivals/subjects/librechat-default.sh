# shellcheck shell=bash
# LibreChat in its default deployment, measured at rest only, beside its lean row
# (docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md): the six services of its compose
# file of v0.8.8, each from its image without Docker (vendors.sh), with librechat.sh's
# configuration and changes. The compose file's API is `librechat-dev:latest`; the release's own
# image, `librechat:v0.8.8`, runs in its place, so that the version is the one measured. The
# services reach each other on the sandbox's loopback, at their compose ports, where the compose
# file names them by host (`mongodb`, `meilisearch`, `rag_api`, `vectordb`).
#
# More changes, each for a reason:
# - MEILI_MASTER_KEY and the admin panel's SESSION_SECRET: the template leaves them empty, and the
#   compose file needs them; fixed test values here;
# - the vector database's own user (999) rather than root: its image's entrypoint drops root by
#   switching user, which a sandbox of one user cannot do; it then runs as that user directly.

# shellcheck source=librechat.sh
source "$(dirname "${BASH_SOURCE[0]}")/librechat.sh"

subject_version() {
    printf '%s; %s; %s; %s; %s; %s; %s\n' "$(vendor_version librechat)" "$(vendor_version mongo)" \
        "$(vendor_version librechat-admin-panel)" "$(vendor_version meilisearch)" \
        "$(vendor_version pgvector)" "$(vendor_version librechat-rag-api)" "$(chromium_version)"
}

subject_parts() { echo server db admin search vectordb rag browser; }

subject_setup() {
    librechat_setup
    local folder=$HOME/librechat
    mkdir -p "$folder"/{meili_data,pgdata}
    sed -i -e 's|^MEILI_MASTER_KEY=$|MEILI_MASTER_KEY=bench-meili-master-key|' "$folder/.env"
}

# A service of the compose file, from its image, in its part, its output in the run's folder.
compose_service() {
    local part=$1 image=$2
    shift 2
    (
        in_part "subject/$part"
        run_image "$image" "$@"
    ) > "$run_dir/$part.log" 2>&1 &
}

subject_start() {
    local folder=$HOME/librechat
    compose_service db mongo "${LIBRECHAT_USER[@]}" --bind "$folder/data-node" /data/db -- mongod --noauth
    compose_service search meilisearch "${LIBRECHAT_USER[@]}" --bind "$folder/meili_data" /meili_data -- \
        MEILI_HOST=http://127.0.0.1:7700 MEILI_NO_ANALYTICS=true MEILI_MASTER_KEY=bench-meili-master-key
    compose_service vectordb pgvector --uid 999 --gid 999 --bind "$folder/pgdata" /var/lib/postgresql/data -- \
        POSTGRES_DB=mydatabase POSTGRES_USER=myuser POSTGRES_PASSWORD=mypassword docker-entrypoint.sh postgres
    wait_for 60 listening 27017 || { echo "MongoDB did not answer"; return 1; }
    wait_for 60 listening 5432 || { echo "the vector database did not answer"; return 1; }
    compose_service rag librechat-rag-api --bind "$folder/.env" /app/.env -- \
        DB_HOST=127.0.0.1 RAG_PORT=8000 POSTGRES_DB=mydatabase POSTGRES_USER=myuser POSTGRES_PASSWORD=mypassword
    compose_service server librechat "${LIBRECHAT_USER[@]}" --bind "$folder/.env" /app/.env \
        --bind "$folder/librechat.yaml" /app/librechat.yaml --bind "$folder/logs" /app/logs \
        --bind "$folder/uploads" /app/uploads --bind "$folder/images" /app/client/public/images \
        --bind "$folder/data" /app/data -- \
        MEILI_HOST=http://127.0.0.1:7700 RAG_PORT=8000 RAG_API_URL=http://127.0.0.1:8000 \
        docker-entrypoint.sh npm run backend
    wait_for 300 served "$LIBRECHAT_URL/health" || { echo "LibreChat's server did not answer"; return 1; }
    compose_service admin librechat-admin-panel -- PORT=3000 SESSION_SECRET=bench-admin-session-secret-0123456789 \
        API_SERVER_URL=http://127.0.0.1:3080 VITE_API_BASE_URL=http://localhost:3080 \
        SESSION_COOKIE_SECURE=false
    in_part subject/browser
    start_chromium "$LIBRECHAT_URL/"
}

subject_stream() {
    echo "the default deployment is measured at rest only (ADR-0028)"
    return 1
}
