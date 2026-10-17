#!/usr/bin/env bash
# Regenerates data/frequency-ja.tsv.gz: how often each word is used in the Tatoeba Japanese
# sentences (CC BY 2.0 FR, https://tatoeba.org), counted with the app's own tokenizer.
# The result is kept in git; run this only to refresh it, then update FREQUENCY_SHA256 in
# scripts/build-dictionary.sh. Run it through the dev container:
#   scripts/dev.sh exec scripts/build-frequency.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${ROOT}/.cache/frequency-build"
mkdir -p "${WORK}"

curl --fail --location --silent --show-error \
    --output "${WORK}/jpn_sentences.tsv.bz2" \
    "https://downloads.tatoeba.org/exports/per_language/jpn/jpn_sentences.tsv.bz2"
bzip2 -dc "${WORK}/jpn_sentences.tsv.bz2" > "${WORK}/jpn_sentences.tsv"

cd "${ROOT}/src-tauri"
cargo run --release --example build_frequency -- "${WORK}/jpn_sentences.tsv" "${WORK}/frequency-ja.tsv"
gzip -9 -c "${WORK}/frequency-ja.tsv" > "${ROOT}/data/frequency-ja.tsv.gz"
sha256sum "${ROOT}/data/frequency-ja.tsv.gz"
