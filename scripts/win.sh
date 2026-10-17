#!/usr/bin/env bash
# Builds and tests the Windows version inside a Linux container, so nothing but Docker is needed:
# the app is cross-compiled (cargo-xwin), the installer is made with NSIS and the tests and the
# app self-test run under Wine. Mirrors scripts/dev.sh.
set -euo pipefail

IMAGE="anki-mining-win"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${ROOT}/.cache"
TARGET="x86_64-pc-windows-msvc"

usage() {
    cat <<EOT
Usage: scripts/win.sh <command>

Commands:
  image        Build the Docker image (about 5 GB)
  shell        Open an interactive shell in the container
  check        cargo check for Windows (all targets, with tests)
  test         Run the Rust tests of the Windows build under Wine
  resources    Fetch the Windows ONNX Runtime and OCR models, build the dictionary and the speech files
  installer    Build the NSIS installer into dist/
  selftest     Run the installed-layout app self-test under Wine
  exec <cmd>   Run any command inside the container
EOT
}

run_in_container() {
    mkdir -p "${CACHE_DIR}/cargo" "${CACHE_DIR}/home-win" "${ROOT}/dist"
    docker run --rm --init \
        --user "$(id -u):$(id -g)" \
        --env HOME=/cache/home-win \
        --env XWIN_ACCEPT_LICENSE=1 \
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
        docker build -f "${ROOT}/docker/Dockerfile.windows" -t "${IMAGE}" "${ROOT}/docker"
        ;;
    shell)
        run_in_container bash
        ;;
    check)
        run_in_container cargo xwin check --target "${TARGET}" --manifest-path src-tauri/Cargo.toml --all-targets
        ;;
    exec)
        run_in_container "$@"
        ;;
    *)
        usage
        exit 1
        ;;
esac
