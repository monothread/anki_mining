# Progress

> Update this file at the end of EVERY session. It is the source of truth for the project state.

**Last updated:** 2026-10-14
**Version:** 0.1.0, first version published (hover lookup works on X11; Anki card creation deferred)
**Current phase:** Phase 1 and Phase 3 done (hover lookup and a first Anki flow: add the word from the popup); Phase 2 (media) next
**Next step:** media for the cards: screenshot of the window under the cursor (P2-01) and audio (P2-02..P2-04, with the exact subtitle timing from mpv in P4-03), then profiles, decks per source and duplicate handling (Phase 5). Also pending: try the Anki flow against a real Anki.

## Status per phase

| Phase | Name | Status |
|---|---|---|
| 1 | MVP: clipboard/websocket → popup → Anki | Done (the Anki flow is tested against a mock AnkiConnect, never tried against a real Anki) |
| 2 | Media on cards (audio + screenshot) — after Phase 3 | Not started |
| 3 | Screen OCR + hover lookup (core) | First version done and verified on the real desktop (YouTube, TV banner) and headless; P3-05 (preprocessing) open and optional |
| 4 | mpv exact subtitles (no browser extension, user decision) | In progress (P4-02 done) |
| 5 | Profiles and decks per source | Not started |
| 6 | Polish (pitch, frequency, packaging) | Not started |

Legend: Not started · In progress · Blocked · Done

## User environment
- Linux Mint, Cinnamon, **X11** (future migration to Wayland).
- Anki with AnkiConnect (default port 8765). The user has no defined note type; the app creates its own.

## Priority (user decision)
Anki integration comes last. The focus is recognizing Japanese words correctly in any situation: YouTube with subtitles, desktop video players with subtitles, games and other apps.

## Interaction (ADR-011)
Move the mouse over a subtitle anywhere on screen and a popup shows the word. Triggers: hold a key (default Shift) or auto when the cursor is still for ~300 ms, switchable. The user pauses the video first. Region selection stays as an optional mode.

## Product goal
Install one app and it works for everything, with no browser extension and no external tools. Screen OCR (Phase 3) is the core source. The browser extension was dropped by user decision (ADR-010).

## Decisions already made (summary; details in 04-decisions.md)
- Native desktop app (not an extension hub); Tauri (Rust) + vanilla TypeScript; Docker for build/tests, AppImage for distribution.
- OCR of the screen is the core source, bundled and offline (MeikiOCR ported to Rust, ONNX Runtime loaded at run time).
- Hover over the text is the main interaction: hold a key or let the cursor rest, the user pauses the video (ADR-011).
- X11 first; Wayland later through abstract interfaces.
- Anki integration last; own "AnkiMining" note type created through AnkiConnect.

## Tooling versions in use (from the first successful scaffold)
Node 22, Rust 1.99, Tauri 2.12, Vite 8, Vitest 5, TypeScript 6, ESLint 10 with `typescript-eslint` strictTypeChecked (`arrow-body-style: always`, `curly: all`). Frontend build output: `web-dist/`; AppImage output: `dist/`.

## Dictionary data (not in git)
`.cache/data/` is git-ignored. JMdict comes from https://github.com/scriptin/jmdict-simplified/releases (file `jmdict-eng-<version>.json.tgz`, license CC BY-SA, keep attribution). Build the db with the `import_jmdict` example. Shipping a prebuilt db inside the AppImage is still to be done (P6-04).

## OCR assets (not in git)
Run `scripts/fetch-ocr-models.sh` once: it puts the two MeikiOCR models and `libonnxruntime.so` into `src-tauri/resources/ocr/` (needed by the OCR tests). `scripts/dev.sh image` builds the main image; the Python spike has its own throwaway image (`spikes/ocr/Dockerfile`).

## Verifying the Anki flow headless
`scripts/dev.sh exec scripts/e2e-mine.sh <frame.png> <jmdict.db> <x> <y> <hotkey|click:X,Y> [out dir]` runs the app on a virtual display with a mock AnkiConnect (`scripts/mock-ankiconnect.py`, which logs every request) and prints the requests it received. The default note type and deck are created/used through AnkiConnect; a real Anki needs the AnkiConnect add-on.

## Verifying the hover lookup headless
Build the AppImage (`scripts/dev.sh appimage`), then `scripts/dev.sh exec scripts/e2e-hover.sh <1920x1080 frame.png> <jmdict.db> <x> <y> [hold|auto] [out dir]`. It shows the frame on a virtual display, starts the app, moves the mouse and saves `before/after/away.png`. `cargo run --release --example ocr_boxes -- src-tauri/resources/ocr <image.png>` prints where the OCR sees each character (to choose x, y).

## App icon
An original blue star being mined with a pickaxe (chips flying off). Source script: `tools/icon/make_icon.py <out.png>` (needs Pillow; run it in the `anki-mining-ocr-spike` image). The 1024 px result is `src-tauri/app-icon.png`; `scripts/dev.sh exec npx tauri icon src-tauri/app-icon.png` regenerates every size in `src-tauri/icons/`. The star is deliberately not the Anki logo (trademark).

## Known limitations
- OCR can mix up text when subtitle layers overlap (e.g. the YouTube caption box drawn over another subtitle) and misread rare kanji in stylised broadcast fonts. The word under the cursor is usually still right; the sentence shown may be noisy.
- Long lines get a stray `、` or `。` now and then at segment cuts and at the end of a line.
- Cards have no picture or audio yet (Phase 2), and all go to the default profile's deck.
- A text block is at most 3 lines (the hovered line plus one neighbour on each side), so a long paragraph gives a 3-line sentence.
- Wayland: the global pointer position is not available to apps; solved in P6-03.
- OCR accuracy was measured mostly on synthetic subtitles; real checks so far are three YouTube cases. Games and desktop players are still untested.
- Vertical text is read with the beta vertical model of MeikiOCR: 85% character accuracy on clean synthetic columns (94% on the best case, much worse on bold outlined text). Detection treats a box as a column only when it is at least 25% taller than wide.
- Deinflection ordering: entries for one text are listed by rule discovery order, so `行って` lists 行う before 行く. Frequency ranking (P6-01) will fix it.
- Grammar patterns that span several words (e.g. `なければならない`, `ざるを得ない`) are not handled; only single-word inflection chains.
- Noun + する (勉強して) resolves to the noun 勉強 via `scan`, not to a `勉強する` entry (JMdict has no such headword).

- The websocket server accepts any local client (no Origin check), so a web page open in the browser could push text into the app. Low impact (text only) but consider an allowlist/token.

## Test status (version 0.1.0)
392 Rust tests (8 need a display and run under xvfb via `scripts/dev.sh test`) and 112 Vitest tests; `tsc`, eslint (0 warnings), `cargo clippy -D warnings` and `cargo fmt` clean. OCR accuracy: 99.9% characters on 350 degraded synthetic subtitle bands (45 ms/image), 99.6% on 60-110 character lines. Real-screen checks: YouTube burned-in subtitle (clean), YouTube caption box over another subtitle (word right, sentence noisy), TV news banner (word right, one kanji misread).

## Blockers / open questions
- None at the moment. No license has been chosen for the project's own code (needs a decision before others can reuse it).

## Session log
| Date | Summary | Tasks completed |
|---|---|---|
| 2026-10-14 | P1-13/P1-14: mining from the hover popup. `HoverMemory` keeps the word of the popup; `card_input` builds the card with the sentence and the matched text highlighted (`Source` = `screen`); `AppState::mine_from_hover`; command `mine_hover(resultIndex, entryIndex)`; the popup has an "Add to Anki" button per entry and shows the result ("Added 友達 (note N)" or the reason); the global shortcut (Alt+M) adds the first entry of the word shown in the popup, or, with no popup, still triggers the main window's own mining. Verified end to end in a virtual display with `scripts/e2e-mine.sh` and a mock AnkiConnect (`scripts/mock-ankiconnect.py`): Shift over 友, Alt+M and a click on the button both produced the right `addNote`. The old `health` smoke-test module was removed. 480 Rust + 121 Vitest tests | P1-13, P1-14 |
| 2026-10-14 | P4-02 mpv source. `sources/mpv.rs`: connects to mpv's JSON IPC socket (default `/tmp/mpvsocket`, config `mpvSocket`, empty turns it off; mpv needs `input-ipc-server=/tmp/mpvsocket` in `mpv.conf`), observes `sub-text` and `media-title`, emits one event per subtitle (exact text, no OCR) with the media title, reconnects when mpv restarts, tolerates split messages and multi-byte characters. The header shows "mpv connected". Checked against a real mpv 0.34.1 (`examples/mpv_listen.rs`, generated video with Japanese subtitles in a throwaway root container): three subtitles received, multi-line kept, title right. The subtitle shows in the main overlay window (hover its words there). Timing for audio clips is not captured yet (P4-03). 471 Rust + 114 Vitest tests | P4-02 |
| 2026-10-14 | P3-15 vertical text. The vertical recognition model (`meiki.text.rec.v0.vertical.32x480.onnx`, 12.9 MB, optional: without it columns are skipped) is ported: columns are scaled to 32 px wide and read in overlapping segments of 420 scaled px, characters are merged by suppression along y, text runs top to bottom. `OcrLine.vertical`, `lines_to_text` reads columns right to left, `ocr::layout` groups neighbouring columns (right to left), `character_at` and the popup placement (`popup_position_beside`) handle columns. Spike: `spikes/ocr/make_vertical*.py`, `run_vertical.py`; accuracy depends strongly on the rendering (clean regular weight 87-94%, bold with outline and noise 76%). 450 Rust + 112 Vitest tests | P3-15 |
| 2026-10-14 | P3-13 and P3-14. Multi-line text: `ocr::layout` groups stacked lines of similar height and overlapping width into blocks (max 3 lines); `recognize_near` reads the whole block near the cursor and `character_at` joins its lines into one sentence with the right UTF-16 offset (a word may start on one line and end on the next). Real-OCR tests on a generated two-line subtitle (`twoline_1080p.png`). The trigger mode and the on/off switch changed from the header are saved to `config.json` (`hoverMode`, `hoverEnabled`); a file that could not be read is never overwritten and a failed save is reported while the change still applies. 426 Rust + 112 Vitest tests | P3-13, P3-14 |
| 2026-10-14 | **First version published** to https://github.com/monothread/anki_mining: README written, architecture doc rewritten to match the code, docs updated, models/ONNX Runtime/dictionary kept out of git (downloaded by `scripts/fetch-ocr-models.sh` and by the user) | — |
| 2026-10-14 | Market research, architecture choice and documentation | — |
| 2026-10-14 | Docker build/test environment (ADR-008). Image built; verified Node 22, Rust 1.99 + clippy, webkit2gtk-4.1, gtk3, xvfb; container runs as host UID and writes to the workspace. `appimage` command not yet exercised (needs the P1-01 scaffold) | P1-00 |
| 2026-10-14 | Tauri v2 + Vite + vanilla TS scaffold with a `health_check` command as smoke test (ADR-009). Verified in the container: `tsc` clean, eslint 0 warnings, Vitest 5/5, clippy `-D warnings` clean, `cargo fmt --check` clean, cargo test 2/2, AppImage built (77.5 MiB) and launched on the host (stayed running until the 10 s timeout). Placeholder blue icon generated at `src-tauri/app-icon.png`; `tauri icon` also generated unused android/ios icon folders | P1-01 |
| 2026-10-14 | Shared types in TS (`src/types/`) and Rust (`src-tauri/src/types/`): `TextSourceEvent`, `WindowInfo`, `Token`, `Entry`, `AnkiNote`, `AnkiMedia`, `Profile`, `ProfileMatch`, `MiningField`, `FieldMapping`; Rust `TextSource` trait + `SourceError` (added `thiserror`). Exact-JSON serde tests on the Rust side; TS has runtime constants only for `MINING_FIELDS` and `DEFAULT_FIELD_MAPPING` (tested), the rest are pure interfaces. Verified in container: tsc, eslint, Vitest 9/9, clippy `-D warnings`, fmt, cargo test 25/25. Field shapes are a first draft: refine them when the consuming tasks (P1-05, P1-06, P1-12) need more | P1-02 |
| 2026-10-14 | `Config` (`src-tauri/src/config/mod.rs`, `src/types/config.ts`): JSON file, defaults (AnkiConnect `http://127.0.0.1:8765`, websocket port 6677, default profile "Mining"/"AnkiMining"), missing file yields defaults, atomic save, validation that reports every problem. 20 tests | P1-03 |
| 2026-10-14 | Tokenizer spike (ADR-005, code in `spikes/tokenizer`) and `Tokenizer` in `src-tauri/src/tokenizer/mod.rs` using Lindera 6 + embedded UniDic; UTF-16 offsets verified with emoji; 14 tests | P1-04, P1-05 |
| 2026-10-14 | `Dictionary` (`src-tauri/src/dictionary/`): streaming importer for `jmdict-simplified` JSON into SQLite (rusqlite bundled), `lookup`, `tag_description`, `lookup_deinflected` (rule-based deinflection in `deinflect.rs`, validated against the entry's JMdict part of speech) and `scan` (longest-prefix scan, up to 20 chars). Real data check with `cargo run --release --example import_jmdict -- <json> <db> [text...]`: 218,898 entries / 499,430 terms imported in ~2 s, 41 MB db, scan 0.1-0.7 ms. 108 Rust tests total | P1-06, P1-07 |
| 2026-10-14 | Text sources (`src-tauri/src/sources/`): `WebSocketSource` (tungstenite, loopback only, 64 KiB message limit, plain text frames, several clients) and `ClipboardSource` (polling, injectable `ClipboardReader`, arboard-backed `system()`), plus a shared `Worker`. Found and fixed a race where text copied right after `start` was taken as the baseline. Two clipboard tests need a display and are `#[ignore]`d; `scripts/dev.sh test` runs everything under xvfb with `--include-ignored`. Stable over 5 repeated runs; 140 Rust tests total | P1-08 |
| 2026-10-14 | User ran the AppImage on the real X11 desktop (Linux Mint/Cinnamon): copied Japanese text, hover showed each word correctly segmented with its dictionary entry. Overlay, tokenizer and dictionary popup confirmed working end to end. Mining into Anki and the `Alt+M` hotkey were not reported as tried | P1-10, P1-11 |
| 2026-10-14 | User reported failures on websites (links, Wikipedia) and the YouTube caption. Findings: (1) small text is NOT the cause (12-48 px read fine, spike `small_text.py`); (2) real bug: lines longer than ~48 characters came out truncated/garbled (60 chars read 41, 80 chars read 30) because the recognition model reads at most 48 characters; web pages have 60-110 character lines. Fix: long lines are cut into overlapping segments (`pipeline::segment_boxes`, 26 line heights wide, 4 overlap) and merged with the existing overlap suppression; 99.6% character accuracy on 60-110 character lines, was garbage. (3) During hover only the line near the cursor is read (`OcrEngine::recognize_near`), so a page full of text costs one line. (4) The app's own window sits on top of the page; hovers over it are now ignored. (5) When nothing is shown the app now says why (`LookupOutcome::explanation`, event `hover-info`). New app icon (blue star mined with a pickaxe). Verified on the user's real screen with three YouTube cases: big burned-in subtitle (clean), YouTube CC box drawn over another subtitle (word correct, sentence polluted by the overlapped Chinese layer) and a TV news banner (word correct, 築 misread as 誕 in the stylised font). Tests: 392 Rust, 112 Vitest | — |
| 2026-10-14 | Hover lookup (ADR-011) implemented: `pointer/` (X11 QueryPointer), `hover/` (`locate`, `trigger` state machine for hold-a-key and auto-dwell, `placement`, `lookup_at`), `app/hover.rs` (watch thread, hidden popup window that never takes focus, `get_hover`/`set_hover_mode`/`set_hover_enabled`), popup page `src/popup/`, header buttons for trigger mode and on/off, config `hoverMode`/`hoverKey`/`hoverDwellMs`. Fixed a real bug found by tests: ONNX Runtime allows one environment per process, so engines now share it (`ocr::meiki::shared_environment`). AppImage now bundles the OCR models and `libonnxruntime.so` (158 MiB). Verified headless (`scripts/e2e-hover.sh`: Xvfb + ImageMagick + xdotool + screenshots): holding Shift on 友 shows 友達, auto mode on 行 shows 行く with `polite < past`, popup hides when the mouse leaves and goes above the line when there is no room below. 368 Rust + 110 Vitest tests pass | P3-07 to P3-12 |
| 2026-10-14 | Product decision (ADR-011): hover over the subtitle on the whole screen is the main interaction, two switchable triggers (hold a key or cursor still), the user pauses the video. The earlier region-selector flow (`sources/ocr.rs`, `src/selector/`, overlay buttons and `set_ocr_region`/`open_region_selector` commands) is kept as an optional mode. 313 Rust + 84 Vitest tests pass | — |
| 2026-10-14 | OCR spike (ADR-006, `spikes/ocr`) and Rust engine (`src-tauri/src/ocr/`: `OcrEngine` trait, `MeikiEngine` on `ort` with the ONNX Runtime library loaded dynamically, bilinear resize matching OpenCV, BGR input, NMS and swapped-pair fixes ported from meikiocr). Rust engine scores 99.90% character accuracy on 350 degraded synthetic subtitle bands at 45 ms/image (Python: 99.9%, 49 ms). `scripts/fetch-ocr-models.sh` downloads models and runtime with pinned hashes. 271 Rust tests pass. Not yet wired into the app and only tested on synthetic images | P3-01, P3-02 |
| 2026-10-14 | User decision: Anki integration last; focus on recognition in every situation. Dev image rebuilt with imagemagick, xdotool and x11-utils for screenshots under xvfb | — |
| 2026-10-14 | Product direction change (ADR-010): OCR becomes the core source and runs right after Phase 1; the browser extension becomes optional; mpv subtitles and AT-SPI added as exact-text sources; OCR must be bundled offline in the AppImage. Roadmap reordered (IDs unchanged) | — |
| 2026-10-14 | `Normalizer` (`src-tauri/src/normalizer/`): strips ruby/HTML/ASS markup, bracketed readings after kanji, NFKC, joins lines inside Japanese text, drops text without Japanese, debounces identical text (1 s, by event timestamp). 26 tests | P1-09 |
| 2026-10-14 | `AnkiClient` (`src-tauri/src/anki/`): version, deckNames, modelNames, createModel, `ensure_model`, storeMediaFile, canAddNotes, addNote (duplicates rejected per deck), retries only on connection errors, error classification (`Duplicate`, `Api`, `Http`, `Connection`). Default "AnkiMining" note type in `anki/model.rs`. Tested against a scripted local HTTP server (`anki/test_support.rs`) that checks the exact JSON sent. 25 tests | P1-12 |
| 2026-10-14 | Card builder (`cards/`), `text_offsets`, `Config.mining_hotkey`, app layer (`app/`: `AppState`, sentence `pipeline`, Tauri commands `get_status`, `scan_at`, `mine`, `import_dictionary`, `submit_text`, global shortcut event `mine-hotkey`) and frontend (`src/ui/`, `src/ipc/backend.ts`, `src/cards/`, styles). Frontend is a transparent always-on-top undecorated window. 230 Rust tests + 65 Vitest tests, tsc/eslint/clippy clean. NOT yet verified visually or end to end in the real app | (P1-10, P1-11, P1-13 pending visual check) |
| 2026-10-14 | P2-01: cards get a JPEG screenshot (max 1280 px wide) of the window under the cursor, captured with the lookup; `screenshotOnCards` config. Browser extension dropped by user decision | — |

## Backlog (ideas outside the current scope)
- Visual signal when a word is identified. Decided to leave it as is (the popup is the signal). The cursor shape cannot be changed over other apps' windows on X11; if wanted later, the clean option is a click-through highlight frame over the word, the alternatives (pointer grab, an input-capturing window) block clicks.
- Support for languages other than Japanese.
- Card review inside the app.
- Sync with Anki mobile (already handled by Anki itself; do not implement).
