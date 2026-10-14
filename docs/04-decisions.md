# Decision records (ADR)

Format: ID, date, status (Accepted / Superseded / Pending), context, decision, consequences. Append new decisions at the end.

## ADR-001 — Native desktop overlay instead of an extension hub
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Alternatives: (A) native app with its own overlay and tokenization; (B) hub orchestrating existing Yomitan, asbplayer, owocr and hookers.
- **Decision:** A. Gives real global coverage and a single UX.
- **Consequences:** More upfront work (tokenizer, dictionary, overlay), but no dependency on several third-party projects.

## ADR-002 — Tauri (Rust) + TypeScript
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Alternatives: Electron + TS, Python + Qt.
- **Decision:** Tauri. Lightweight, good for overlays, and the tokenizer (Vibrato/Lindera) is Rust.
- **Consequences:** Screen and audio capture in Rust; the OCR ecosystem (Python) comes in as a sidecar.

## ADR-003 — X11 first, Wayland through abstraction
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** The user runs Linux Mint/Cinnamon on X11 and will migrate to Wayland in the future.
- **Decision:** MVP on X11. Capture, global shortcut, overlay and audio sit behind interfaces (`ScreenCapture`, `GlobalHotkey`, `OverlayWindow`, `AudioCapture`) with X11 and Wayland backends.
- **Consequences:** No core code may call X11 directly; Wayland backends arrive in task P6-03.

## ADR-004 — Own "AnkiMining" note type
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** The user has no defined note type.
- **Decision:** The app creates the note type through `createModel` with default fields and allows custom mapping per profile.
- **Consequences:** Must detect whether the model already exists (`modelNames`) and handle model versions.

## ADR-008 — Docker for build/test, AppImage for distribution
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** The host (and end users) should not need Rust or `-dev` packages installed. A GUI overlay with global hotkeys, screen/audio capture and clipboard does not run well inside a container.
- **Decision:** Build, lint and tests run in a container (Ubuntu 22.04, Rust, Node 22, Tauri deps, xvfb) through `scripts/dev.sh`. The app is distributed as an **AppImage** built in that container and runs natively on the host. Flatpak is a later option (P6-04).
- **Consequences:** Only Docker is required on the dev machine. The container runs as the host UID/GID; caches live in the git-ignored `.cache/`. Ubuntu 22.04 keeps the AppImage glibc baseline low. Running the dev window inside the container is out of scope.

## ADR-009 — Frontend: Vite + vanilla TypeScript
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Overlay and popup are small screens where latency is dominated by the Rust side and WebKitGTK, not by the UI framework. Vanilla TS has the smallest bundle and no runtime overhead; Svelte is nearly as light; React is the heaviest.
- **Decision:** Vite + vanilla TypeScript for the overlay and popup. The Phase 5 settings UI may use Svelte in a separate Tauri window without touching the core.
- **Consequences:** DOM and state handled by hand; keep UI modules small and pure where possible so they are easy to test with jsdom. Vite builds to `web-dist/` (not `dist/`, which holds the AppImage).

## ADR-005 — Tokenizer: Lindera 6 with embedded UniDic
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Candidates were Vibrato, Lindera and Sudachi. Spike in `spikes/tokenizer` ran Lindera 6.2 with the embedded UniDic and IPADIC dictionaries over 8 real sentences (conjugations, stacked auxiliaries, kana-spelled verbs).
- **Findings:** Both dictionaries load in under 1 ms (memory-mapped) and tokenize about 8000 sentences in ~17-24 ms (about 3 µs each). Dictionaries are compiled into the binary at build time (needs network on the first build), so nothing is downloaded at runtime. UniDic gives lemma, lemma reading and written lemma (`orthBase`) per token; IPADIC merges more compounds (日本語 as one token) but has less detail. Vibrato was not benchmarked: it needs a separately downloaded prebuilt dictionary and, with the same MeCab-style dictionary, would segment identically; Lindera already meets the performance need and is simpler to ship in an AppImage.
- **Decision:** Lindera 6 with the embedded UniDic dictionary. Compounds are handled in the dictionary layer by longest-match scanning (P1-07), so UniDic's shorter units are not a problem.
- **Consequences:** `Token.lemma` is UniDic `orthBase` (the written dictionary form, e.g. `しまう`), `Token.reading` is the katakana reading of the lemma (`lForm`), `Token.pos` is the top-level POS. Unknown words get the surface as lemma, an empty reading and `未知語`. Binary size with UniDic only should be re-measured at packaging time (the spike binary with both dictionaries was ~248 MB unstripped).

## ADR-006 — OCR engine: MeikiOCR (ONNX) ported to Rust
- **Date:** 2026-10-14 · **Status:** Accepted (provisional until checked on real video frames)
- **Context:** The OCR must be bundled in the app and work offline with no extra installs (ADR-010). Candidates looked at: meikiocr (ONNX detector + recognizer trained on Japanese game text), manga-ocr (a few hundred MB, recognition only), PaddleOCR-based Rust ports.
- **Findings:** On 80 synthetic subtitle images (1080p/720p/480p; outline, yellow, box, thin and serif styles; dark, bright and busy backgrounds) meikiocr read 99.8% of the characters; on 350 degraded subtitle bands (JPEG q25, blur, noise, half resolution and a combination) 99.9%. The only mistake was an extra `。` on one sentence. Speed on CPU is ~47 ms per full 1080p frame in Python and ~45 ms in the Rust port. The two models total 33 MB. The models use `GridSample`/`ScatterND`/`TopK`, so pure-Rust runtimes were not tried; `ort` (ONNX Runtime) is used.
- **Decision:** Port the MeikiOCR pipeline to Rust (`src-tauri/src/ocr/`) behind an `OcrEngine` trait and run it with `ort`. Load the official ONNX Runtime 1.31.0 shared library at runtime (`load-dynamic`, requires only glibc 2.28) instead of the pyke prebuilt static library, which needs glibc 2.38+ and would not link in the Ubuntu 22.04 build image. Model and library files are fetched by `scripts/fetch-ocr-models.sh` with pinned SHA-256 and are not stored in git.
- **Licenses:** meikiocr code is Apache-2.0; both models are LGPL-3.0 per their Hugging Face model cards; ONNX Runtime is MIT. Ship attribution and re-check the model pages before redistributing.
- **Limits:** The test images are synthetic (clean fonts, no motion, no hard-coded video artifacts), so real accuracy must be confirmed on actual frames. Vertical text is not ported (the model supports it in beta). Detection returns at most 64 boxes and recognition 48 characters per line.
- **Consequences:** +~62 MB in the AppImage (models 33 MB + ONNX Runtime 29 MB). The Python spike lives in `spikes/ocr` and is only a reference.

## ADR-010 — Screen OCR is the core source; the browser extension is optional
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** The product promise is "install one app and it works for everything". A browser extension only covers browsers and needs a separate install, so it cannot be the base. Copyable text (clipboard, websocket) already works but needs user action.
- **Decision:** Screen OCR of a user-selected region is the core source (Phase 3, right after Phase 1). It must ship inside the app (offline, no Python or system packages for the end user). The browser extension (P4-01) becomes optional and last; mpv subtitles (P4-02) and AT-SPI (P4-05) are added as exact-text sources that need no OCR.
- **Consequences:** Larger AppImage (OCR model files). OCR accuracy on subtitles over video is the main risk and is measured first (P3-01) on real material. CPU use is controlled by re-running OCR only when the captured region changes (P3-04). Wayland needs the screen-capture portal, deferred to P6-03.

## ADR-007 — Audio encoding (ffmpeg vs native crate)
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Cards need the sentence audio. The user pauses the video first, so the sound must be recorded beforehand.
- **Decision:** No ffmpeg and no encoder library. The app records the default output's monitor with `parec` (PulseAudio and PipeWire's Pulse layer) as mono 16 kHz 16-bit into a 60 s circular buffer; when a card is made the last stretch of speech (at most `audioSeconds`, default 8) is cut out, skipping the silence at the end and starting after the last pause longer than 600 ms, and stored as WAV (`storeMediaFile` through `addNote`'s `audio`). WAV is about 32 KB/s, so a clip is a few hundred KB, which Anki plays everywhere.
- **Consequences:** `parec` must be installed on the host (it is on every desktop with PulseAudio or PipeWire-pulse); without it the app starts, warns once and makes cards without audio. Sound older than 60 s of pause is lost. Audio of one application (not the whole output) and exact subtitle timing (P4-03) are not done; a speech-recognition source stays undecided (P2-06).

## ADR-011 — Hover over the subtitle is the main interaction; the user pauses the video
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Selecting a region and reading the sentence inside the app's own window (first OCR implementation) is not how the product should feel. The intended use is the same as Yomitan on web pages: move the mouse over the text where it is shown.
- **Decision:** The main interaction is hover lookup on the whole screen. Two triggers, switchable by the user: (a) hold a key (default Shift) while the cursor is over the text, which reads at once and is the default; (b) auto mode, which reads when the cursor has been still for about 300 ms. The app captures the whole screen, runs OCR (about 45 ms per Full HD frame), finds the OCR line and character under the cursor, maps it to a token and shows a popup next to the cursor. Region selection stays as an optional "follow region" mode (`OcrSource`).
- **Video:** The user pauses the video before looking a word up. A playing video changes the subtitle faster than the lookup, and this is accepted; the app does not control players.
- **Consequences:** Needs the cursor position and modifier keys (X11 QueryPointer; on Wayland apps cannot read the global pointer, to be solved in P6-03). The popup must not take focus from the video and must be hidden while the screen is captured so it never appears in the OCR input. Reuses the existing capture, OCR, tokenizer and dictionary modules.

## ADR-012 — Profiles chosen by application, and the duplicate policy
- **Date:** 2026-10-14 · **Status:** Accepted
- **Context:** Mining on YouTube should go to one deck and in a game to another. In hover mode the text comes from the screen, so the only thing known about where it came from is the window under the cursor.
- **Decision:** A profile has rules: the text source, the application (the `WM_CLASS` class of the window under the cursor, any case) and a part of the window title. `profiles::resolve` picks, among the profiles whose every rule fits, the one with the most rules (the first on a tie), else the default profile. The window is read when the lookup is triggered and kept with the popup's word (`HoverMemory`). The settings panel (button "Settings") edits profiles, field mappings, screenshot/audio options and the duplicate policy and saves them to `config.json` at once; all writers share one `ConfigFile` so none saves an older copy over another's change.
- **Duplicates:** `skip` (default): `addNote` with `allowDuplicate: false` refuses a card that already exists in the deck and the user is told; `allow`: the card is added again. "Update the existing card" was not built: it would overwrite cards the user may have edited.
- **Consequences:** Under Wayland the window under the cursor is not available (P6-03). A config file that could not be read is never overwritten, so settings made in that case last only until the app closes.

