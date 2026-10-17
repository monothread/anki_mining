# Roadmap

Task format: `[ ] ID — description`. Tick `[x]` when done. A task is only done when it meets `05-conventions-and-testing.md` (section "Definition of done").
Dependencies are shown as `(dep: ID)`.

## Product goal and execution order
The product must work after installing a single app, with no browser extension or external tools. Screen OCR is therefore the core text source for anything that is not copyable. **Execution order: Phase 3 (OCR + hover lookup) → Phase 4 (mpv/exact-text sources) → Phase 2 (media) → Phase 5 → Phase 6.** Anki integration is deliberately **last** (user decision): P1-13 real-Anki verification, P1-14 and Phase 5 are deferred until word recognition works in every situation (YouTube with subtitles, desktop video players with subtitles, games, other apps). Phase numbers and task IDs are kept stable to avoid churn; only the order changed.

## Phase 1 — MVP: clipboard/websocket → popup → Anki
**Acceptance criteria:** copying a Japanese sentence to the clipboard (or sending it to the websocket) shows the overlay with highlighted words; clicking a word shows the popup with reading and meaning; a shortcut creates the card in the configured deck with the sentence.

- [x] P1-00 — Docker build/test environment (`docker/Dockerfile`, `scripts/dev.sh`); image build verified (see PROGRESS.md)
- [x] P1-01 — Tauri + TypeScript scaffold, strict tsconfig, eslint, clippy, folder structure and local CI (lint/test scripts)
- [x] P1-02 — Shared interfaces (`TextSource`, `Token`, `Entry`, `AnkiNote`, `Profile`) in `src/types/` and Rust equivalents (dep: P1-01)
- [x] P1-03 — `Config`: read/write/validation, defaults (dep: P1-02)
- [x] P1-04 — Tokenizer spike: compare Vibrato vs Lindera (accuracy, size, speed) and record an ADR (dep: P1-01)
- [x] P1-05 — `Tokenizer` with the spike winner, returning lemma, reading and POS (dep: P1-04)
- [x] P1-06 — JMdict-to-SQLite importer + `Dictionary.lookup` (dep: P1-02)
- [x] P1-07 — Deinflection (verb and adjective forms) with extensive tests (dep: P1-06)
- [x] P1-08 — `ClipboardSource` and `WebSocketSource` (127.0.0.1) (dep: P1-02)
- [x] P1-09 — `Normalizer` (cleanup, debounce) (dep: P1-08)
- [x] P1-10 — X11 `OverlayWindow`: transparent, always-on-top, word highlighting (dep: P1-05, P1-09)
- [x] P1-11 — Dictionary popup (hover/click) (dep: P1-07, P1-10)
- [x] P1-12 — Typed `AnkiClient` + creation of the "AnkiMining" note type (dep: P1-02)
- [x] P1-13 — (checked against a mock AnkiConnect end to end; not yet against a real Anki) `CardBuilder` + global mining shortcut (dep: P1-11, P1-12)
- [x] P1-14 — End-to-end tests of the full flow with mocked AnkiConnect; document usage in the README; remove the `health` smoke-test module

## Phase 2 — Media on cards
**Acceptance criteria:** the card automatically includes a screenshot and the sentence audio (last N seconds). Starts after Phase 3.
- [x] P2-01 — Screenshot of the window/region for the card, reusing the `ScreenCapture` from P3-03 (dep: P3-03)
- [x] P2-02 — `AudioCapture` with circular buffer (PulseAudio/PipeWire) (dep: P1-01)
- [x] P2-03 — Audio trimming and encoding (ffmpeg or native crate; decide via ADR)
- [x] P2-04 — `CardBuilder` integration and `storeMediaFile` (dep: P2-01, P2-03)
- [ ] P2-06 — Decide whether to offer speech recognition (e.g. Whisper) as an extra text source when no subtitles are on screen; record an ADR (not committed to the roadmap yet)
- [x] P2-05 — Unit/e2e tests with exact payloads

## Phase 3 — Screen OCR with hover lookup (core, right after Phase 1)
**Acceptance criteria:** with a fresh install and nothing else installed, pausing a video (YouTube, a desktop player, a game) and moving the mouse over a Japanese subtitle shows a popup next to the cursor with the word under the cursor (reading, meaning, dictionary form). No window or region has to be selected. Playing video is not supported: the user pauses it (ADR-011).
- [x] P3-01 — OCR spike on subtitle material and engine choice (ADR-006)
- [x] P3-02 — Bundled OCR engine behind an `OcrEngine` trait (`src-tauri/src/ocr/`)
- [x] P3-03 — X11 `ScreenCapture` (`src-tauri/src/capture/`)
- [x] P3-04 — `OcrSource` for continuous reading of a selected region (`sources/ocr.rs`); now the optional "follow region" mode
- [x] P3-07 — Pointer tracking: `PointerTracker` trait with an X11 backend (cursor position and modifier keys)
- [x] P3-08 — Hover trigger state machine: hold-a-key mode (default Shift), auto mode (cursor still for ~300 ms), switchable at runtime and in the config
- [x] P3-09 — Word under the cursor: pick the line and character from the OCR boxes, map it to the token and scan the dictionary from there
- [x] P3-10 — `HoverService`: capture the whole screen, run OCR, resolve the word; popup hidden during the capture; stale or failed lookups never show
- [x] P3-11 — Popup window next to the cursor (below the line, above if there is no room), does not take focus from the video, hides when the cursor leaves
- [x] P3-12 — Bundle the OCR models and ONNX Runtime library in the AppImage and verify headless (xvfb + xdotool + screenshot) and on the real desktop
- [ ] P3-05 — (optional) Image preprocessing for subtitles over video, measured on real frames (only if P3-12 shows accuracy problems)
- [x] P3-06 — Tests (including fixed subtitle and long-line screenshots with expected text) and documentation
- [x] P3-13 — Multi-line subtitles: join the lines of one subtitle block into the sentence used for the lookup (only the hovered line is used today)
- [x] P3-14 — Persist the trigger mode and on/off state changed from the header (saved to `config.json`)
- [x] P3-15 — Vertical text (games, manga): port the vertical recognition model of MeikiOCR (beta quality, see PROGRESS)
- [ ] P3-16 — Visual signal on the word when it is identified (click-through highlight frame); decided to skip for now, see PROGRESS backlog

## Phase 4 — Exact-text sources (mpv)
**Acceptance criteria:** mpv subtitles appear in the overlay with timestamps and cards use the exact clip. There is no browser extension (user decision).
- [x] P4-02 — `MpvSource` via IPC socket (exact subtitle text, no OCR)
- [x] P4-03 — Sync audio trimming with subtitle timestamps
- [x] P4-05 — Evaluate AT-SPI (accessibility) as a text source for apps that expose text; keep only if coverage is useful (evaluated, not built: ADR-014)
- [-] P4-01 — Browser extension: dropped by user decision (the app works without one)
- [x] P4-04 — Tests and documentation

## Phase 5 — Profiles and decks per source
**Acceptance criteria:** mining on YouTube goes to the "Youtube" deck; in game X, to the "Games" deck.
- [x] P5-01 — `ProfileResolver` (by application/window/source)
- [x] P5-02 — Profile configuration UI and field mapping
- [x] P5-03 — Duplicate detection (`canAddNotes`) and policy (skip/update)
- [x] P5-04 — E2E tests

## Phase 6 — Polish
- [x] P6-01 — Pitch accent and frequency (Yomitan-format dictionaries)
- [x] P6-02 — Yomitan dictionary importer
- [ ] P6-03 — Wayland backends (`ScreenCapture` portal/PipeWire, `GlobalHotkey`, `OverlayWindow` layer-shell)
- [ ] P6-04 — Packaging: AppImage is built from the start via `scripts/dev.sh appimage`; here add Flatpak, release automation and installation docs
