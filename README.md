# anki_mining

Look up Japanese words anywhere on your screen, with no browser extension: pause a video, move the
mouse over a subtitle and a popup shows the word under the cursor with its reading, dictionary form and
meaning. The goal is to turn those words into Anki cards, which is the last part of the plan.

<p align="center"><img src="src-tauri/app-icon.png" alt="App icon: a star being mined with a pickaxe" width="128"></p>

**Status: first version (0.1.0).** Hover lookup works on X11 (YouTube subtitles, TV banners, web pages,
any text on screen). Creating Anki cards is implemented and unit-tested but deliberately not finished or
tried against a real Anki yet. See [docs/PROGRESS.md](docs/PROGRESS.md).

## How it works

```
 mouse over text ──► screenshot ──► OCR ──► character under the cursor ──► tokenizer ──► dictionary ──► popup
 (hold Shift, or     (whole screen)  (MeikiOCR,  (OCR boxes)                (Lindera +     (JMdict +        (next to the
  rest the cursor)                    offline)                               UniDic)        deinflection)    cursor)
```

- **Triggers**, switchable in the app (and remembered): hold **Shift** over the text, or let the cursor
  rest for about 300 ms. Pause the video first: a playing video changes the subtitle faster than a lookup.
- **Everything is local.** OCR, tokenizer and dictionary run on your machine; nothing is sent anywhere.
- **Other text sources** also feed the same overlay: the clipboard, a local websocket
  (`ws://127.0.0.1:6677`, compatible with Textractor/owocr-style senders), a text box, and an optional
  "read a screen region" mode.
- Handles inflected verbs and adjectives (`行きました` → `行く`, `polite < past`) and long lines (web pages).

## Requirements

- Linux with **X11** (tested on Linux Mint / Cinnamon). Wayland is planned (see the roadmap).
- A Japanese-capable font for the popup (for example `fonts-noto-cjk`).
- Anki with the AnkiConnect add-on, only once card creation is finished.

## Run

There is no published release yet: build the AppImage yourself (see "Develop" below; it takes a few
minutes and only needs Docker), then:

```bash
chmod +x AnkiMining_0.1.0_amd64.AppImage
./AnkiMining_0.1.0_amd64.AppImage
```

The first time, the dictionary is empty and the app asks for a JMdict file. Download
`jmdict-eng-<version>.json.tgz` from <https://github.com/scriptin/jmdict-simplified/releases>, extract it
and paste the path of the `.json` file in the app (about 2 seconds to import 218,000 entries).

## Develop

Only Docker is needed on your machine. Everything (Rust, Node, Tauri libraries) lives in a container:

```bash
scripts/dev.sh image          # build the dev image once
scripts/fetch-ocr-models.sh   # download the OCR models and ONNX Runtime (SHA-256 checked)
scripts/dev.sh install        # npm ci
scripts/dev.sh typecheck      # tsc
scripts/dev.sh lint           # eslint + clippy
scripts/dev.sh test           # Vitest + cargo test (under a virtual X11 display)
scripts/dev.sh appimage       # build dist/AnkiMining_0.1.0_amd64.AppImage
```

`scripts/e2e-hover.sh` checks the hover lookup end to end without a real screen (virtual display,
screenshot, mouse move). More in [docs/PROGRESS.md](docs/PROGRESS.md).

## Documentation

| | |
|---|---|
| [docs/PROGRESS.md](docs/PROGRESS.md) | Current state, next steps, known limitations, session log |
| [docs/02-architecture.md](docs/02-architecture.md) | Modules, data flow, folder structure |
| [docs/03-roadmap.md](docs/03-roadmap.md) | Phases and tasks |
| [docs/04-decisions.md](docs/04-decisions.md) | Why things are the way they are (ADRs) |
| [docs/01-market-analysis.md](docs/01-market-analysis.md) | Existing tools and how people mine words |

## Known limitations

- Overlapping subtitle layers can confuse the OCR, and rare kanji in stylised fonts can be misread. The
  word under the cursor is usually right; the sentence shown may be noisy.
- Vertical text is supported but less accurate than horizontal text (beta model). Multi-line subtitles and neighbouring columns are joined (up to 3 lines around the hovered one).
- X11 only for now (the global pointer position is not available to apps on Wayland).
- The popup does not create Anki cards yet.

## Third-party components and licenses

| Component | Use | License |
|---|---|---|
| [MeikiOCR](https://github.com/rtr46/meikiocr) models ([detection](https://huggingface.co/rtr46/meiki.text.detect.v0), [recognition](https://huggingface.co/rtr46/meiki.txt.recognition.v0)) | Text recognition; the pipeline was ported to Rust | Code Apache-2.0; models LGPL-3.0 (per their model cards) |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) 1.31 | Runs the OCR models | MIT |
| [Lindera](https://github.com/lindera/lindera) with UniDic | Japanese tokenizer | MIT; UniDic has its own license |
| [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) via [jmdict-simplified](https://github.com/scriptin/jmdict-simplified) | Dictionary (not included, downloaded by the user) | CC BY-SA 4.0 |
| [Tauri](https://tauri.app) | App framework | MIT / Apache-2.0 |

The models and the ONNX Runtime library are not stored in this repository; `scripts/fetch-ocr-models.sh`
downloads them. The app icon is original artwork (a star, not the Anki logo, which is a trademark).

No license has been chosen for this project's own code yet.
