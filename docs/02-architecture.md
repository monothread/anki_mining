# Architecture

## Overview
Hover lookup (the main interaction, ADR-011):
```
 pointer + key ──► Trigger ──► whole-screen capture ──► OCR (near the cursor) ──► character under cursor
 (X11 query)       (hold key    (X11 GetImage)            (MeikiOCR, segments        (OCR boxes)
                    or dwell)                              for long lines)               │
                                                                                          ▼
        popup window ◄── HoverResult ◄── Dictionary.scan ◄── Tokenizer ◄── token at that offset
        (next to the cursor)             (JMdict + deinflection) (Lindera + UniDic)
```
Other text sources feed the same overlay window through a channel:
```
 TextSource (clipboard, websocket, OCR region, manual text) ──► Normalizer ──► Tokenizer ──► overlay "sentence" event
```
Card creation (last part of the plan) is `CardBuilder` → `AnkiClient` → AnkiConnect.

## Stack
- **Shell/backend:** Tauri 2 (Rust). **Frontend:** vanilla TypeScript built with Vite, three pages: main overlay (`index.html`), popup (`popup.html`), region selector (`selector.html`).
- **Tokenizer:** Lindera 6 with the embedded UniDic (ADR-005).
- **Dictionary:** JMdict in SQLite (rusqlite) with rule-based deinflection.
- **OCR:** MeikiOCR ONNX models ported to Rust and run with `ort`; the ONNX Runtime library is loaded at run time (ADR-006).
- **Anki:** AnkiConnect over HTTP (`ureq`).
- **Build/distribution:** Docker for build and tests, AppImage for distribution (ADR-008).

## Folder structure
```
anki_mining/
├── README.md, CLAUDE.md, docs/
├── scripts/                 dev.sh (Docker driver), fetch-ocr-models.sh, e2e-hover.sh
├── docker/Dockerfile        build/test image (Ubuntu 22.04)
├── src/                     TypeScript
│   ├── main.ts, ui/         main overlay (sentence view, lookup view, controls)
│   ├── popup/               hover popup page
│   ├── selector/            full-screen region selector page
│   ├── ipc/                 Backend interface and Tauri bindings
│   ├── cards/, types/       card input and the shared interfaces
├── src-tauri/               Rust
│   ├── src/app/             Tauri glue: state, sentence pipeline, hover thread, commands
│   ├── src/hover/           locate (character/word under the cursor), trigger, placement, lookup
│   ├── src/ocr/             OcrEngine trait, MeikiEngine, pipeline (pre/post-processing), raster helpers
│   ├── src/capture/         ScreenCapture trait + X11 backend
│   ├── src/pointer/         PointerTracker trait + X11 backend
│   ├── src/sources/         TextSource implementations: clipboard, websocket, ocr (region)
│   ├── src/dictionary/      SQLite dictionary, JMdict importer, deinflection
│   ├── src/tokenizer/, normalizer/, cards/, anki/, config/, types/, text_offsets.rs
│   ├── resources/ocr/       models + ONNX Runtime (downloaded, git-ignored)
│   ├── tests/fixtures/ocr/  subtitle and long-line images with their expected text
│   └── examples/            import_jmdict, ocr_eval, ocr_boxes (command-line tools)
├── spikes/                  throwaway experiments kept for reference (tokenizer, OCR)
├── tools/icon/              script that draws the app icon
└── test/unit/               Vitest tests (frontend)
```

## Hover lookup details
- **Trigger** (`hover/trigger.rs`): a state machine fed with pointer samples. `Hold` fires when the key is pressed and again when the cursor rests (150 ms) after moving with the key held; `Auto` fires when the cursor has been still for the dwell time (default 300 ms). Jitter up to 4 px counts as still. The mode and the on/off switch can be changed at run time (`HoverControl`) and are saved to `config.json` (`hoverMode`, `hoverEnabled`).
- **Capture and OCR:** the whole screen is captured; only the detected lines near the cursor and the other lines of their block are recognized (`OcrEngine::recognize_near`). Lines longer than 26 line heights are cut into overlapping segments, read separately and merged with overlap suppression, because the recognition model reads at most about 48 characters. Boxes at least 25% taller than wide are vertical columns: they go to the vertical model (scaled to 32 px wide, segments of 420 scaled px) and their characters are ordered top to bottom.
- **Locating the word** (`hover/locate.rs`): the line nearest to the cursor (ties by horizontal distance), then the nearest character (accepted within half a line height). The sentence is the text of the line's block (`ocr/layout.rs`: stacked lines of similar height that overlap horizontally, or neighbouring vertical columns read right to left, at most 3), so a two-line subtitle is one sentence; the character's UTF-16 offset in that text is mapped to a token, punctuation is not a word. `Dictionary::scan` then looks up the text from the token start.
- **Popup** (`app/hover.rs`, `hover/placement.rs`): a hidden, non-focusable, always-on-top window shown below the line (above when there is no room), or beside a vertical column (right first, then left). It is hidden before each capture so it never reaches the OCR, and again when the cursor leaves the word and the popup. Triggers over the app's own windows are ignored.
- **Explanations:** when nothing is shown the app says why (`LookupOutcome::explanation`, event `hover-info`).

## Shared types (IPC contract)
Defined twice and kept in sync by hand: TypeScript in `src/types/` and Rust in `src-tauri/src/types/` (plus types next to their module, such as `HoverResult`). Rust structs use `#[serde(rename_all = "camelCase")]`, optional values are `null`, and the Rust tests assert the exact JSON. Token offsets are UTF-16 code units so the frontend can slice strings directly; the Rust side converts from bytes (`text_offsets.rs`).

## Dictionary and deinflection
- `Dictionary` (SQLite): tables `entry`, `term`, `tag`; `lookup(term)` is an exact match on a written form or reading, common entries first. `Entry.meanings` has one string per sense, `Entry.parts_of_speech` the raw JMdict tags.
- `deinflect::deinflect(text)` returns candidate dictionary forms with a category (V1, V5, VS, VK, ADJ_I plus intermediate STEM/TE/TA/MASU) and the inflections undone (outermost first).
- `lookup_deinflected` accepts a deinflected candidate only if the entry's tag fits the candidate's category; `scan` tries prefixes longest first (max 20 characters).

## Platform abstraction (ADR-003)
`ScreenCapture` and `PointerTracker` are traits with X11 backends; a Wayland backend (portals, layer-shell) is planned for P6-03. Global shortcuts use the Tauri plugin. The core never calls X11 directly.

## Text sources
Each emits `{ text, sourceId, timestamp, windowInfo? }` into one channel. Implemented: `ClipboardSource`, `WebSocketSource` (loopback only, 64 KiB limit), `OcrSource` (selected region, reads only when the pixels change and the text is confirmed by two reads). Planned: mpv IPC (P4-02) and an optional browser extension (P4-01).

## Default "AnkiMining" note type
Fields: `Expression`, `Reading`, `Meaning`, `Sentence`, `SentenceAudio`, `Picture`, `Frequency`, `Pitch`, `Source`. Created through `createModel` if it does not exist; the field mapping is configurable per profile.

## Security and privacy
- Local servers listen on `127.0.0.1` only; the websocket accepts any local client (see PROGRESS).
- No data leaves the machine: OCR, tokenizer and dictionary are local. Any external call must be an explicit opt-in.
- The app reads the whole screen when a lookup is triggered; nothing is stored.

## Technical risks
- Wayland: pointer position, screen capture and overlays depend on portals and the compositor.
- OCR on video: overlapping subtitle layers, stylised fonts, hardware-accelerated players that may capture black.
- Licenses: the OCR models are LGPL-3.0, JMdict is CC BY-SA; keep attribution (see README).
