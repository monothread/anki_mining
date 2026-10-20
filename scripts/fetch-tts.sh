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

# `linux` (the default) builds src-tauri/resources/tts; `windows` builds src-tauri/resources/tts-windows
# (the Windows libraries come from the same downloader and the helper is built with mingw-w64):
#   VOICEVOX_TERMS_ACCEPTED=1 scripts/fetch-tts.sh [linux|windows]
PLATFORM="${1:-linux}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${ROOT}/.cache/tts-build"
case "${PLATFORM}" in
    linux)
        TARGET="${ROOT}/src-tauri/resources/tts"
        CORE="${WORK}/core"
        DOWNLOAD_OS="linux"
        ;;
    windows)
        TARGET="${ROOT}/src-tauri/resources/tts-windows"
        CORE="${WORK}/core-windows"
        DOWNLOAD_OS="windows"
        ;;
    *)
        echo "unknown platform ${PLATFORM} (use linux or windows)" >&2
        exit 2
        ;;
esac

CORE_VERSION="0.17.0"
# The downloader that runs on this machine (it can fetch the files of any system).
case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
        DOWNLOADER_NAME="download-windows-x64.exe"
        DOWNLOADER_SHA256="4f0ae2758f3149f084cc91556065009553bf81010f58498c89acb6e2289546b6"
        ;;
    *)
        DOWNLOADER_NAME="download-linux-x64"
        DOWNLOADER_SHA256="3615f2bba7ff3d4396c02dffb7ce594fde8100dbaf757869bf32d0517d34dc03"
        ;;
esac
DOWNLOADER_URL="https://github.com/VOICEVOX/voicevox_core/releases/download/${CORE_VERSION}/${DOWNLOADER_NAME}"
# The compiler that makes the Windows helper (mingw-w64); on a Windows machine it is plain gcc.
MINGW_GCC="${MINGW_GCC:-x86_64-w64-mingw32-gcc}"
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

DOWNLOADER="${WORK}/${DOWNLOADER_NAME}"
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

if [ ! -d "${CORE}/models/vvms" ]; then
    # The downloader fetches files for any system, so Linux can fetch the Windows ones.
    # `yes` is cut off by SIGPIPE when the downloader ends, which `pipefail` would report as an
    # error; what matters is that the files are there.
    (cd "${WORK}" && yes | "${DOWNLOADER}" --os "${DOWNLOAD_OS}" -o "${CORE}" \
        --only c-api onnxruntime dict models --models-pattern "${VOICE_MODEL}") || true
    if [ ! -d "${CORE}/models/vvms" ]; then
        echo "the VOICEVOX downloader did not fetch the voice model" >&2
        exit 1
    fi
fi

mkdir -p "${TARGET}"
find "${TARGET}" -mindepth 1 -maxdepth 1 ! -name README.md -exec rm -rf {} +
cp -r "${CORE}/dict/open_jtalk_dic_utf_8-1.11" "${TARGET}/"
cp -r "${CORE}/models/vvms" "${TARGET}/"
cp "${CORE}/models/TERMS.txt" "${TARGET}/VOICEVOX-TERMS.txt"
cp "${CORE}/onnxruntime/TERMS.txt" "${TARGET}/VOICEVOX-ONNXRUNTIME-TERMS.txt"
cp "${CORE}/c_api/LICENSE" "${TARGET}/VOICEVOX-CORE-LICENSE"

if [ "${PLATFORM}" = "windows" ]; then
    cp "${CORE}/c_api/lib/voicevox_core.dll" "${TARGET}/"
    cp "${CORE}/onnxruntime/lib/voicevox_onnxruntime.dll" "${TARGET}/"
    # mingw-w64's linker links against the DLL itself.
    "${MINGW_GCC}" -O2 -Wall -o "${TARGET}/voicevox_say.exe" "${ROOT}/tools/tts/voicevox_say.c" \
        -I"${CORE}/c_api/include" -L"${TARGET}" -lvoicevox_core -lshell32
    test -s "${TARGET}/voicevox_say.exe"
    du -sh "${TARGET}"
    exit 0
fi

cp "${CORE}/c_api/lib/libvoicevox_core.so" "${TARGET}/"
cp "${CORE}"/onnxruntime/lib/libvoicevox_onnxruntime.so* "${TARGET}/"

gcc -O2 -Wall -o "${TARGET}/voicevox_say" "${ROOT}/tools/tts/voicevox_say.c" \
    -I"${CORE}/c_api/include" -L"${TARGET}" -lvoicevox_core -Wl,-rpath,'$ORIGIN'
chmod 755 "${TARGET}/voicevox_say"

# A quick check: say a short word and make sure a sound file comes out.
"${TARGET}/voicevox_say" "${TARGET}" 8 "こんにちは" "${WORK}/check.wav"
test "$(head -c 4 "${WORK}/check.wav")" = "RIFF"
du -sh "${TARGET}"
