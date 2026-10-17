#!/usr/bin/env bash
# Builds src-tauri/resources/dictionary/jmdict.db, the dictionary that is bundled with the app:
#  - JMdict (English meanings), downloaded from jmdict-simplified (CC BY-SA 4.0)
#  - pitch accents from Kanjium's accents.txt at a pinned commit (CC BY-SA 4.0)
#  - word frequencies in data/frequency-ja.tsv.gz, counted from the Tatoeba sentences with
#    scripts/build-frequency.sh (kept in git)
# Every download is verified by SHA-256.
# Run it through the dev container: scripts/dev.sh exec scripts/build-dictionary.sh
set -euo pipefail

# `python3` on Linux; Windows has `python`.
PYTHON="$(command -v python3 || command -v python)"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${ROOT}/src-tauri/resources/dictionary"
WORK="${ROOT}/.cache/dictionary-build"

JMDICT_VERSION="3.6.2+20261012204142"
JMDICT_URL="https://github.com/scriptin/jmdict-simplified/releases/download/${JMDICT_VERSION//+/%2B}/jmdict-eng-${JMDICT_VERSION//+/%2B}.json.zip"
JMDICT_ZIP_SHA256="9a04fc1d0facf87645f8ec7cdaa370647d4a8f59131f6e2a614bcd680f855b85"
KANJIUM_COMMIT="9ebca4589565c696e7c39e089e2e928ad65f678c"
ACCENTS_URL="https://raw.githubusercontent.com/mifunetoshiro/kanjium/${KANJIUM_COMMIT}/data/source_files/raw/accents.txt"
ACCENTS_SHA256="8bd0dd127dab32ceec94cb03ab1ba6b68858ea73421dfa1731af2f373deb4f20"
FREQUENCY_GZ="${ROOT}/data/frequency-ja.tsv.gz"
FREQUENCY_SHA256="b74dcb9a79c22cdec5f5970fa76c4247ef0dbe291ce6e48442b66e52c698bf6b"

sha256_of() {
    sha256sum "$1" | cut -d' ' -f1
}

# fetch_verified <url> <destination> <sha256>
fetch_verified() {
    local url="$1" destination="$2" expected="$3"
    if [ -f "${destination}" ] && [ "$(sha256_of "${destination}")" = "${expected}" ]; then
        return
    fi
    curl --fail --location --silent --show-error --output "${destination}.part" "${url}"
    if [ "$(sha256_of "${destination}.part")" != "${expected}" ]; then
        rm -f "${destination}.part"
        echo "checksum mismatch for ${url}" >&2
        exit 1
    fi
    mv "${destination}.part" "${destination}"
}

mkdir -p "${WORK}" "${TARGET}"

if [ "$(sha256_of "${FREQUENCY_GZ}")" != "${FREQUENCY_SHA256}" ]; then
    echo "data/frequency-ja.tsv.gz does not match the expected checksum" >&2
    exit 1
fi

fetch_verified "${JMDICT_URL}" "${WORK}/jmdict-eng.zip" "${JMDICT_ZIP_SHA256}"
fetch_verified "${ACCENTS_URL}" "${WORK}/accents.txt" "${ACCENTS_SHA256}"
"${PYTHON}" -m zipfile -e "${WORK}/jmdict-eng.zip" "${WORK}"
JSON="$(ls "${WORK}"/jmdict-eng-*.json | head -1)"
gzip -dc "${FREQUENCY_GZ}" > "${WORK}/frequency-ja.tsv"

# Tauri's build script insists that the files the bundle lists exist, even to compile the program
# that makes them: an empty placeholder (replaced by the real file) satisfies it.
[ -e "${TARGET}/jmdict.db" ] || : > "${TARGET}/jmdict.db"

cd "${ROOT}/src-tauri"
cargo run --release --example build_dictionary -- \
    "${JSON}" "${WORK}/accents.txt" "${WORK}/frequency-ja.tsv" "${TARGET}/jmdict.db"
chmod 644 "${TARGET}/jmdict.db"
ls -la "${TARGET}/jmdict.db"
