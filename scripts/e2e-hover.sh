#!/usr/bin/env bash
# End-to-end check of the hover lookup without a real screen. Runs INSIDE the dev container:
#   scripts/dev.sh exec scripts/e2e-hover.sh <image.png> <jmdict.db> <x> <y> [hold|auto] [output dir]
# A virtual X11 display shows <image.png> (a Full HD frame with a subtitle) as the "screen", the
# AppImage is started, the mouse is moved to (x, y) and the screen is captured.
set -euo pipefail

IMAGE="$1"
DICTIONARY="$2"
X="$3"
Y="$4"
MODE="${5:-hold}"
OUT="${6:-.cache/e2e}"
APPIMAGE="$(ls dist/*.AppImage | head -1)"

mkdir -p "${OUT}"
export HOME="$(mktemp -d)"
export DISPLAY=:99
export APPIMAGE_EXTRACT_AND_RUN=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

mkdir -p "${HOME}/.local/share/dev.ankimining.app" "${HOME}/.config/dev.ankimining.app"
cp "${DICTIONARY}" "${HOME}/.local/share/dev.ankimining.app/jmdict.db"
if [ "${MODE}" = "auto" ]; then
    echo '{ "hoverMode": "auto" }' > "${HOME}/.config/dev.ankimining.app/config.json"
fi

Xvfb :99 -screen 0 1920x1080x24 >"${OUT}/xvfb.log" 2>&1 &
XVFB=$!
trap 'kill ${APP:-0} ${XVFB} 2>/dev/null || true' EXIT
sleep 1

display -window root "${IMAGE}" &
sleep 1
"${APPIMAGE}" >"${OUT}/app.log" 2>&1 &
APP=$!

# The app loads the tokenizer, the dictionary and the OCR models first.
sleep 14
import -window root "${OUT}/before.png"

xdotool mousemove "${X}" "${Y}"
if [ "${MODE}" = "hold" ]; then
    xdotool keydown shift
    sleep 4
else
    sleep 5
fi
import -window root "${OUT}/after.png"
xdotool keyup shift 2>/dev/null || true
xdotool mousemove 5 5
sleep 2
import -window root "${OUT}/away.png"
echo "screenshots in ${OUT}; app log:"
grep -v "Gtk-Message\|Theme parsing" "${OUT}/app.log" | head -20 || true
