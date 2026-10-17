#!/usr/bin/env bash
# Runs project tooling inside the build container so nothing but Docker is needed on the host.
set -euo pipefail

IMAGE="anki-mining-dev"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${ROOT}/.cache"

usage() {
    cat <<EOF
Usage: scripts/dev.sh <command>

Commands:
  image        Build the Docker image
  shell        Open an interactive shell in the container
  install      Install JS dependencies (npm ci)
  typecheck    npx tsc --noEmit --project tsconfig.json
  lint         eslint (src/ test/) and cargo clippy
  test         JS tests and cargo test (under xvfb)
  appimage     Build the release AppImage into dist/
  exec <cmd>   Run any command inside the container
EOF
}

run_in_container() {
    mkdir -p "${CACHE_DIR}/cargo" "${CACHE_DIR}/home" "${ROOT}/dist"
    docker run --rm --init \
        --user "$(id -u):$(id -g)" \
        --env HOME=/cache/home \
        --env TAURI_SIGNING_PRIVATE_KEY \
        --env TAURI_SIGNING_PRIVATE_KEY_PASSWORD \
        --volume "${ROOT}:/workspace" \
        --volume "${CACHE_DIR}:/cache" \
        --workdir /workspace \
        $([ -t 0 ] && echo "--interactive --tty") \
        "${IMAGE}" "$@"
}

command="${1:-}"
shift || true

case "${command}" in
    image)
        docker build --tag "${IMAGE}" --file "${ROOT}/docker/Dockerfile" "${ROOT}/docker"
        ;;
    shell)
        run_in_container bash
        ;;
    install)
        run_in_container npm ci
        ;;
    typecheck)
        run_in_container npx tsc --noEmit --project tsconfig.json
        ;;
    lint)
        run_in_container bash -c "npx eslint src/ test/ --max-warnings=0 && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings"
        ;;
    test)
        run_in_container xvfb-run --auto-servernum --server-args="-screen 0 1920x1080x24 -noreset" bash -c "npm test && cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored"
        ;;
    appimage)
        # With TAURI_SIGNING_PRIVATE_KEY set (a release build) the build also makes the signature
        # that the updater of the app checks.
        run_in_container bash -c '[ -f src-tauri/resources/dictionary/jmdict.db ] || scripts/build-dictionary.sh; extra=(); if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then extra=(--config "{\"bundle\":{\"createUpdaterArtifacts\":true}}"); fi; npx tauri build --bundles appimage "${extra[@]}" && for file in src-tauri/target/release/bundle/appimage/*.AppImage src-tauri/target/release/bundle/appimage/*.AppImage.sig; do [ -f "$file" ] || continue; name="$(basename "$file")"; cp "$file" "dist/${name}.part" && mv -f "dist/${name}.part" "dist/${name}"; done'
        ;;
    exec)
        run_in_container "$@"
        ;;
    *)
        usage
        exit 1
        ;;
esac
