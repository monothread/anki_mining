#!/usr/bin/env bash
# Publishes a release from this machine:
#  - the Linux AppImage is built here, in Docker, and signed for the updater
#  - the Windows installer is built and tested by the "Windows" workflow on a real Windows machine
#    (run it first; this script takes the installer of its latest successful run)
#  - latest.json (what the installed apps read to find the new version) is made from both
#  - the GitHub release is created with all the files
#
#   scripts/release.sh <version> [notes file]
#
# The signing key is read from $TAURI_SIGNING_KEY_FILE (default ~/.tauri/anki-mining-updater.key).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:?usage: scripts/release.sh <version> [notes file]}"
NOTES_FILE="${2:-}"
KEY_FILE="${TAURI_SIGNING_KEY_FILE:-${HOME}/.tauri/anki-mining-updater.key}"
REPOSITORY="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
TAG="v${VERSION}"
WORK="${ROOT}/dist/release-${VERSION}"

cd "${ROOT}"

configured="$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")"
if [ "${configured}" != "${VERSION}" ]; then
    echo "src-tauri/tauri.conf.json says ${configured}, not ${VERSION}" >&2
    exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "the working tree has changes; commit them first" >&2
    exit 1
fi
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
    echo "HEAD is not origin/main; push first" >&2
    exit 1
fi
[ -f "${KEY_FILE}" ] || { echo "no signing key at ${KEY_FILE}" >&2; exit 1; }

mkdir -p "${WORK}"

echo "== Linux AppImage"
TAURI_SIGNING_PRIVATE_KEY="$(cat "${KEY_FILE}")" scripts/dev.sh appimage
cp dist/AnkiMining_*_amd64.AppImage dist/AnkiMining_*_amd64.AppImage.sig "${WORK}/"

echo "== Windows installer (latest successful run of the Windows workflow on this commit)"
run_id="$(gh run list --workflow windows.yml --commit "$(git rev-parse HEAD)" --status success --limit 1 --json databaseId --jq '.[0].databaseId')"
if [ -z "${run_id}" ] || [ "${run_id}" = "null" ]; then
    echo "no successful Windows run for this commit; run: gh workflow run windows.yml" >&2
    exit 1
fi
rm -rf "${WORK}/windows"
gh run download "${run_id}" --name windows-installer --dir "${WORK}/windows"
cp "${WORK}"/windows/*-setup.exe "${WORK}"/windows/*-setup.exe.sig "${WORK}/"

echo "== latest.json"
if [ -n "${NOTES_FILE}" ]; then
    cp "${NOTES_FILE}" "${WORK}/notes.md"
else
    printf 'AnkiMining %s\n' "${VERSION}" > "${WORK}/notes.md"
fi
linux="$(ls "${WORK}"/AnkiMining_*_amd64.AppImage)"
windows="$(ls "${WORK}"/AnkiMining_*-setup.exe)"
python3 scripts/latest_json.py "${VERSION}" "${REPOSITORY}" "${WORK}/notes.md" "${linux}" "${windows}" > "${WORK}/latest.json"
cat "${WORK}/latest.json"

echo "== GitHub release ${TAG}"
git tag -a "${TAG}" -m "AnkiMining ${VERSION}" 2>/dev/null || true
git push origin "${TAG}"
gh release create "${TAG}" \
    "${linux}" "${linux}.sig" "${windows}" "${windows}.sig" "${WORK}/latest.json" \
    --title "AnkiMining ${VERSION}" --notes-file "${WORK}/notes.md" --verify-tag
gh release view "${TAG}" --json url --jq .url
