# Research notes

## AnkiConnect
- Default endpoint `http://127.0.0.1:8765`; Anki must be open.
- `addNote`: deck, model, fields, tags; returns the id or `null` on failure. Accepts `audio`/`picture` with `url` or `path`/`data`, `filename` and `fields`.
- `storeMediaFile`: writes a file to the media folder (base64, path or URL).
- Check the current official documentation before implementing (the source consulted was an old mirror).

## Tokenizers
- MeCab: baseline; Vibrato: Rust reimplementation, about 2x faster than MeCab but sensitive to dictionary size; Sudachi: multi-level segmentation (A/B/C) and normalization.
- **Verified (spike P1-04, ADR-005):** Lindera 6 with the embedded UniDic loads in under 1 ms and tokenizes a sentence in about 3 µs, returning lemma, reading and part of speech. Vibrato was not benchmarked.
- Not verified: how Yomitan picks the lemma; our deinflection is an own rule set (`dictionary/deinflect.rs`) checked against the JMdict part of speech.

## OCR
- Chosen: MeikiOCR (ADR-006). Models: detection `meiki.text.detect.v0.1.960x544.onnx` (input 960x544 BGR, outputs up to 64 boxes), recognition `meiki.text.rec.v0.960x32.onnx` (input 960x32, up to 48 characters per line, `char_codes` are `i32`). Both use D-FINE style operators (`GridSample`, `ScatterND`, `TopK`), so pure-Rust runtimes were not tried.
- **Learned the hard way:** (1) ONNX Runtime allows one environment per process, so all engines share it; (2) the pyke prebuilt library needs glibc 2.38+ and does not link on Ubuntu 22.04, so the official ONNX Runtime 1.31 library (needs glibc 2.28) is loaded at run time; (3) the recognition model garbles lines longer than about 48 characters, so long lines are cut into overlapping segments; (4) the models expect BGR input (OpenCV order); (5) small text is not a problem (12-48 px read fine on a 1920x1080 screen).
- Measured on synthetic subtitles: 99.9% character accuracy on degraded bands, 45 ms per image on CPU. Real YouTube checks: clean on a big burned-in subtitle; overlapping subtitle layers get mixed; stylised kanji can be misread.
- Other candidates seen: manga-ocr (a few hundred MB, recognition only), PaddleOCR Rust ports (not benchmarked), owocr (websocket hub; may listen on 0.0.0.0, never expose outside 127.0.0.1).

## X11
- `GetImage` on the root window captures what is on screen (also with a compositor, as scrot does); the cursor is not part of the image. `QueryPointer` gives the position and modifier keys without any permission. A window decides the cursor shape, so an app cannot change the cursor over other apps' windows (the pointer grab trick blocks clicks).
- Under xvfb, `xdotool` (XTEST) can move the pointer and press keys, which is how `scripts/e2e-hover.sh` works.

## Wayland
- Capture via xdg-desktop-portal ScreenCast + PipeWire; overlay via layer-shell; global shortcuts via the GlobalShortcuts portal. Support varies by compositor. Research when reaching task P6-03.

## Reference projects
- Yomitan (deinflection logic and dictionary format)
- asbplayer (audio/screenshot capture and Anki integration)
- Kamite (immersion companion architecture with OCR on Linux)
- LunaTranslator and Textractor (text websocket protocol)
