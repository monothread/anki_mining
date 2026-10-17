#!/usr/bin/env bash
# Builds src-tauri/resources/dictionary/jmdict.db, the dictionary that is bundled with the app:
#  - JMdict (English meanings), downloaded from jmdict-simplified and verified by SHA-256
#  - Portuguese meanings from the Portuguese Wiktionary (data/ptwiktionary-ja.jsonl.gz, kept in git)
# Run it through the dev container: scripts/dev.sh exec scripts/build-dictionary.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${ROOT}/src-tauri/resources/dictionary"
WORK="${ROOT}/.cache/dictionary-build"

JMDICT_VERSION="3.6.2+20261012204142"
JMDICT_URL="https://github.com/scriptin/jmdict-simplified/releases/download/${JMDICT_VERSION//+/%2B}/jmdict-eng-${JMDICT_VERSION//+/%2B}.json.zip"
JMDICT_ZIP_SHA256="9a04fc1d0facf87645f8ec7cdaa370647d4a8f59131f6e2a614bcd680f855b85"
PORTUGUESE_SHA256="8b64f115e740df5592e68785ee34bcd95685febd1e9079876da2915032fc18c8"
PORTUGUESE_GZ="${ROOT}/data/ptwiktionary-ja.jsonl.gz"

sha256_of() {
    sha256sum "$1" | cut -d' ' -f1
}

mkdir -p "${WORK}" "${TARGET}"

if [ "$(sha256_of "${PORTUGUESE_GZ}")" != "${PORTUGUESE_SHA256}" ]; then
    echo "data/ptwiktionary-ja.jsonl.gz does not match the expected checksum" >&2
    exit 1
fi

ZIP="${WORK}/jmdict-eng.zip"
if [ ! -f "${ZIP}" ] || [ "$(sha256_of "${ZIP}")" != "${JMDICT_ZIP_SHA256}" ]; then
    curl --fail --location --silent --show-error --output "${ZIP}.part" "${JMDICT_URL}"
    if [ "$(sha256_of "${ZIP}.part")" != "${JMDICT_ZIP_SHA256}" ]; then
        rm -f "${ZIP}.part"
        echo "checksum mismatch for ${JMDICT_URL}" >&2
        exit 1
    fi
    mv "${ZIP}.part" "${ZIP}"
fi

python3 -m zipfile -e "${ZIP}" "${WORK}"
JSON="$(ls "${WORK}"/jmdict-eng-*.json | head -1)"
gzip -dc "${PORTUGUESE_GZ}" > "${WORK}/ptwiktionary-ja.jsonl"

cd "${ROOT}/src-tauri"
cargo run --release --example build_dictionary -- \
    "${JSON}" "${WORK}/ptwiktionary-ja.jsonl" "${TARGET}/jmdict.db"
chmod 644 "${TARGET}/jmdict.db"
ls -la "${TARGET}/jmdict.db"
