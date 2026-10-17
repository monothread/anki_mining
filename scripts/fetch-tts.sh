#!/usr/bin/env bash
# Prepares src-tauri/resources/tts, the offline speech that reads words and sentences for the cards:
#  - VOICEVOX CORE (MIT), the VOICEVOX ONNX Runtime, the Open JTalk dictionary and ONE voice model
#    (0.vvm: 四国めたん, ずんだもん, 春日部つむぎ, 雨晴はう), fetched with VOICEVOX's own downloader
#  - tools/tts/voicevox_say.c, the small program the app runs to make a WAV, compiled here
# The voice models and the ONNX Runtime come with terms of use (credit "VOICEVOX:<character>" is
# required, redistribution inside an application is allowed; read them in
# .cache/tts-build/core/models/TERMS.txt after the first download). The downloader asks to accept
# them; this script only answers for you when VOICEVOX_TERMS_ACCEPTED=1 is set, which means you
# have read and accepted them:
#   VOICEVOX_TERMS_ACCEPTED=1 scripts/dev.sh exec scripts/fetch-tts.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${ROOT}/src-tauri/resources/tts"
WORK="${ROOT}/.cache/tts-build"

CORE_VERSION="0.17.0"
DOWNLOADER_URL="https://github.com/VOICEVOX/voicevox_core/releases/download/${CORE_VERSION}/download-linux-x64"
DOWNLOADER_SHA256="3615f2bba7ff3d4396c02dffb7ce594fde8100dbaf757869bf32d0517d34dc03"
VOICE_MODEL="0.vvm"

if [ "${VOICEVOX_TERMS_ACCEPTED:-}" != "1" ]; then
    cat >&2 <<'MESSAGE'
The VOICEVOX voice models and ONNX Runtime have terms of use (credit "VOICEVOX:<character>" is
required in what you publish with the voice). Read them at
https://github.com/VOICEVOX/voicevox_vvm/blob/main/README.md and run again with
VOICEVOX_TERMS_ACCEPTED=1 if you accept them.
MESSAGE
    exit 1
fi

mkdir -p "${WORK}" "${TARGET}"

DOWNLOADER="${WORK}/download"
if [ ! -x "${DOWNLOADER}" ] || [ "$(sha256sum "${DOWNLOADER}" | cut -d' ' -f1)" != "${DOWNLOADER_SHA256}" ]; then
    curl --fail --location --silent --show-error --output "${DOWNLOADER}.part" "${DOWNLOADER_URL}"
    if [ "$(sha256sum "${DOWNLOADER}.part" | cut -d' ' -f1)" != "${DOWNLOADER_SHA256}" ]; then
        rm -f "${DOWNLOADER}.part"
        echo "checksum mismatch for ${DOWNLOADER_URL}" >&2
        exit 1
    fi
    chmod +x "${DOWNLOADER}.part"
    mv "${DOWNLOADER}.part" "${DOWNLOADER}"
fi

if [ ! -d "${WORK}/core/models/vvms" ]; then
    (cd "${WORK}" && yes | "${DOWNLOADER}" -o ./core \
        --only c-api onnxruntime dict models --models-pattern "${VOICE_MODEL}")
fi

find "${TARGET}" -mindepth 1 -maxdepth 1 ! -name README.md -exec rm -rf {} +
cp "${WORK}/core/c_api/lib/libvoicevox_core.so" "${TARGET}/"
cp "${WORK}"/core/onnxruntime/lib/libvoicevox_onnxruntime.so* "${TARGET}/"
cp -r "${WORK}/core/dict/open_jtalk_dic_utf_8-1.11" "${TARGET}/"
cp -r "${WORK}/core/models/vvms" "${TARGET}/"
cp "${WORK}/core/models/TERMS.txt" "${TARGET}/VOICEVOX-TERMS.txt"
cp "${WORK}/core/c_api/LICENSE" "${TARGET}/VOICEVOX-CORE-LICENSE"

gcc -O2 -Wall -o "${TARGET}/voicevox_say" "${ROOT}/tools/tts/voicevox_say.c" \
    -I"${WORK}/core/c_api/include" -L"${TARGET}" -lvoicevox_core -Wl,-rpath,'$ORIGIN'
chmod 755 "${TARGET}/voicevox_say"

# A quick check: say a short word and make sure a sound file comes out.
"${TARGET}/voicevox_say" "${TARGET}" 8 "こんにちは" "${WORK}/check.wav"
test "$(head -c 4 "${WORK}/check.wav")" = "RIFF"
du -sh "${TARGET}"
