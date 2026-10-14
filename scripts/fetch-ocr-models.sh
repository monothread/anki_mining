#!/usr/bin/env bash
# Downloads what the OCR needs into src-tauri/resources/ocr and verifies the SHA-256 of every file:
#  - the MeikiOCR ONNX models (LGPL-3.0, see src-tauri/resources/ocr/README.md)
#  - the official ONNX Runtime shared library (MIT), loaded at runtime by the `ort` crate
# None of these files is stored in git.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${ROOT}/src-tauri/resources/ocr"

ORT_VERSION="1.31.0"
ORT_ARCHIVE_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz"
ORT_ARCHIVE_SHA256="cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57"
ORT_LIBRARY="libonnxruntime.so"
ORT_LIBRARY_SHA256="469849977cb60744ea160927e1a3a0643fd4c55c0aff743251c6d293402e334e"

# name | repository | sha256
MODELS=(
    "meiki.text.detect.v0.1.960x544.onnx|rtr46/meiki.text.detect.v0|40b6a016667745cae7d3055929ae3b8b1e7716aac795f5904cd3c2c7c3b8404b"
    "meiki.text.rec.v0.960x32.onnx|rtr46/meiki.txt.recognition.v0|3e96bc772fbee9717e536a6353032bb944c3382dd2f6960ef4890decda43b000"
)

sha256_of() {
    sha256sum "$1" | cut -d' ' -f1
}

verified() {
    [ -f "$1" ] && [ "$(sha256_of "$1")" = "$2" ]
}

fetch_verified() {
    local url="$1" destination="$2" expected="$3"
    curl --fail --location --silent --show-error --output "${destination}.part" "${url}"
    local actual
    actual="$(sha256_of "${destination}.part")"
    if [ "${actual}" != "${expected}" ]; then
        rm -f "${destination}.part"
        echo "checksum mismatch for ${url}: expected ${expected}, got ${actual}" >&2
        exit 1
    fi
    chmod 644 "${destination}.part"
    mv "${destination}.part" "${destination}"
}

mkdir -p "${TARGET}"

for entry in "${MODELS[@]}"; do
    IFS='|' read -r name repository expected <<<"${entry}"
    if verified "${TARGET}/${name}" "${expected}"; then
        echo "ok      ${name}"
        continue
    fi
    echo "fetch   ${name}"
    fetch_verified "https://huggingface.co/${repository}/resolve/main/${name}" "${TARGET}/${name}" "${expected}"
done

if verified "${TARGET}/${ORT_LIBRARY}" "${ORT_LIBRARY_SHA256}"; then
    echo "ok      ${ORT_LIBRARY}"
else
    echo "fetch   ${ORT_LIBRARY} (ONNX Runtime ${ORT_VERSION})"
    workdir="$(mktemp -d)"
    trap 'rm -rf "${workdir}"' EXIT
    fetch_verified "${ORT_ARCHIVE_URL}" "${workdir}/ort.tgz" "${ORT_ARCHIVE_SHA256}"
    tar -xzf "${workdir}/ort.tgz" -C "${workdir}" "onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION}"
    mv "${workdir}/onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION}" "${TARGET}/${ORT_LIBRARY}.part"
    if [ "$(sha256_of "${TARGET}/${ORT_LIBRARY}.part")" != "${ORT_LIBRARY_SHA256}" ]; then
        rm -f "${TARGET}/${ORT_LIBRARY}.part"
        echo "checksum mismatch for ${ORT_LIBRARY}" >&2
        exit 1
    fi
    mv "${TARGET}/${ORT_LIBRARY}.part" "${TARGET}/${ORT_LIBRARY}"
fi
