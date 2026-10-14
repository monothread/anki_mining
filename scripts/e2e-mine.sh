#!/usr/bin/env bash
# End-to-end check of mining from the hover popup, without a real screen or a real Anki. Runs INSIDE
# the dev container:
#   scripts/dev.sh exec scripts/e2e-mine.sh <image.png> <jmdict.db> <x> <y> <hotkey|click:X,Y> [out dir]
# A virtual display shows <image.png>, a mock AnkiConnect records what it receives, the AppImage is
# started, Shift is held over (x, y) to open the popup and then the card is added with the mining
# shortcut (Alt+M) or by clicking the button at the given screen position.
set -euo pipefail

IMAGE="$1"
DICTIONARY="$2"
X="$3"
Y="$4"
ACTION="$5"
OUT="${6:-.cache/e2e-mine}"
APPIMAGE="$(ls dist/*.AppImage | head -1)"

mkdir -p "${OUT}"
rm -f "${OUT}/anki-requests.jsonl"
export HOME="$(mktemp -d)"
export DISPLAY=:99
export APPIMAGE_EXTRACT_AND_RUN=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

mkdir -p "${HOME}/.local/share/dev.ankimining.app"
if [ -n "${E2E_CONFIG:-}" ]; then
    mkdir -p "${HOME}/.config/dev.ankimining.app"
    cp "${E2E_CONFIG}" "${HOME}/.config/dev.ankimining.app/config.json"
fi
cp "${DICTIONARY}" "${HOME}/.local/share/dev.ankimining.app/jmdict.db"

Xvfb :99 -screen 0 1920x1080x24 >"${OUT}/xvfb.log" 2>&1 &
XVFB=$!
python3 scripts/mock-ankiconnect.py "${OUT}/anki-requests.jsonl" 8765 &
MOCK=$!
trap 'kill ${APP:-0} ${MOCK} ${XVFB} 2>/dev/null || true' EXIT
sleep 1

# E2E_WINDOW=1 shows the frame in a window of its own (so the app can tell which application the
# word is in) instead of on the root window; E2E_CONFIG is a config.json to start the app with.
if [ "${E2E_WINDOW:-0}" = "1" ]; then
    display -geometry +0+0 "${IMAGE}" &
else
    display -window root "${IMAGE}" &
fi
sleep 1
"${APPIMAGE}" >"${OUT}/app.log" 2>&1 &
APP=$!
sleep 14
xprop -root _NET_ACTIVE_WINDOW >/dev/null 2>&1 || true
if [ "${E2E_WINDOW:-0}" = "1" ]; then
    xprop -id "$(xdotool search --onlyvisible --class . | head -1)" WM_CLASS WM_NAME >"${OUT}/window-class.txt" 2>&1 || true
fi

xdotool mousemove "${X}" "${Y}"
xdotool keydown shift
sleep 4
import -window root "${OUT}/popup.png"
xdotool keyup shift
sleep 0.5

if [ "${ACTION}" = "hotkey" ]; then
    xdotool key alt+m
else
    coordinates="${ACTION#click:}"
    xdotool mousemove "${coordinates%,*}" "${coordinates#*,}"
    sleep 0.5
    xdotool click 1
fi
sleep 3
import -window root "${OUT}/after-mining.png"
echo "--- requests received by the mock AnkiConnect:"
cat "${OUT}/anki-requests.jsonl" 2>/dev/null || echo "(none)"
