# anki_mining

Look up Japanese words anywhere on your screen, with no browser extension: pause a video, move the
mouse over a subtitle and a popup shows the word under the cursor with its reading, dictionary form and
meaning, and one click (or Alt+M) adds it to Anki with its sentence.

<p align="center"><img src="src-tauri/app-icon.png" alt="App icon: a star being mined with a pickaxe" width="128"></p>

**Status: first version (0.1.0).** Hover lookup works on X11 (YouTube subtitles, TV banners, web pages,
any text on screen). Adding a word to Anki works against a mock AnkiConnect and has not been tried against
a real Anki yet; cards have no picture or audio yet. See [docs/PROGRESS.md](docs/PROGRESS.md).

## Adding words to Anki

With the popup open, click **Add to Anki** on the entry you want, or press **Alt+M** to add the first one.
The card gets the word, its reading and meanings, the sentence with the word highlighted a screenshot
of the window the word was found in and the last seconds of sound that were playing (the app records the
computer's audio output with `parec`, which comes with PulseAudio and PipeWire). Turn them off with
`screenshotOnCards: false` or `audioOnCards: false` in `config.json`; `audioSeconds` sets the longest clip. Duplicates are
refused. The popup tells you what happened.

## Spoken word and sentence

Every card gets a spoken example from a natural-sounding neural Japanese voice that runs offline inside the app
([VOICEVOX](https://voicevox.hiroshiba.jp), the voice 春日部つむぎ): the word goes to the `WordAudio` field and
the sentence to `SentenceAudio` (when the sentence was heard in a video the real recording is kept there instead).
It takes about 2 seconds per card. Every entry in the popup (and in the main window) also has a **▶** button that reads the
word aloud (about 2 seconds the first time, instant after that); it plays through `paplay`, `pw-play` or `aplay`. Turn it off in **Settings → Cards → Spoken word and sentence**. If your
"AnkiMining" note type is from an older version, the app adds the `WordAudio` field to it; add `{{WordAudio}}` to
its card template to hear it on the card. The voice's terms ask for the credit **VOICEVOX:春日部つむぎ** wherever
you publish audio made with it.

## Decks per application (profiles)

Open **Settings** to choose where cards go. A profile has a deck, a note type, a field mapping and rules that
decide when it is used: the application (its window class, e.g. `mpv` or `Navigator`), a word in the window title
(e.g. `YouTube`, to tell sites apart in a browser) and the text source. The profile with the most matching rules
wins; the default profile is used when none matches. You can also choose whether the card is refused or added
again when it already exists in the deck. **Keep running in the system tray** makes the close button hide the window instead of quitting; use the tray menu to quit.

## Releases

Pushing a tag like `v0.2.0` makes GitHub Actions (`.github/workflows/release.yml`) build the AppImage and attach it,
with a `SHA256SUMS` file, to a release. The app needs an X11 session; Wayland is not supported yet.

## How it works

```
 mouse over text ──► screenshot ──► OCR ──► character under the cursor ──► tokenizer ──► dictionary ──► popup
 (hold Shift, or     (whole screen)  (MeikiOCR,  (OCR boxes)                (Lindera +     (JMdict +        (next to the
  rest the cursor)                    offline)                               UniDic)        deinflection)    cursor)
```

- **Triggers**, switchable in the app (and remembered): hold **Shift** over the text, or let the cursor
  rest for about 300 ms. Pause the video first: a playing video changes the subtitle faster than a lookup.
- **Everything is local.** OCR, tokenizer and dictionary run on your machine; nothing is sent anywhere.
- **Other text sources** also feed the same overlay: the exact subtitles of **mpv** (add
  `input-ipc-server=/tmp/mpvsocket` to `mpv.conf`; the header shows "mpv connected"), the clipboard, a local websocket
  (`ws://127.0.0.1:6677`, compatible with Textractor/owocr-style senders), a text box, and an optional
  "read a screen region" mode.
- Handles inflected verbs and adjectives (`行きました` → `行く`, `polite < past`) and long lines (web pages).

## Requirements

- Linux with **X11** (tested on Linux Mint / Cinnamon). Wayland is planned (see the roadmap).
- A Japanese-capable font for the popup (for example `fonts-noto-cjk`).
- Anki with the [AnkiConnect](https://ankiweb.net/shared/info/2055492159) add-on (open while mining). The first card creates the note type `AnkiMining` and uses the deck `Mining` (configurable in `config.json`).

## Run

There is no published release yet: build the AppImage yourself (see "Develop" below; it takes a few
minutes and only needs Docker), then:

```bash
chmod +x AnkiMining_0.1.0_amd64.AppImage
./AnkiMining_0.1.0_amd64.AppImage
```

The dictionary is part of the app: nothing has to be downloaded or imported. It has the 218,000 JMdict entries
with English meanings, the pitch accent of about 124,000 words (shown as a line over the reading and as `[0]`/`[1]`...
in the popup and in the `Pitch` field of the card) and a frequency rank for the 18,000 most used words (`#337`, also in the
`Frequency` field). You can add more pitch accent or frequency data in **Settings → Dictionary** from any Yomitan
dictionary `.zip` that has `term_meta_bank` files (the lowest frequency rank of a word is kept).

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
- Cards have no picture or audio yet.

## Third-party components and licenses

| Component | Use | License |
|---|---|---|
| [MeikiOCR](https://github.com/rtr46/meikiocr) models ([detection](https://huggingface.co/rtr46/meiki.text.detect.v0), [recognition](https://huggingface.co/rtr46/meiki.txt.recognition.v0)) | Text recognition; the pipeline was ported to Rust | Code Apache-2.0; models LGPL-3.0 (per their model cards) |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) 1.31 | Runs the OCR models | MIT |
| [Lindera](https://github.com/lindera/lindera) with UniDic | Japanese tokenizer | MIT; UniDic has its own license |
| [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) via [jmdict-simplified](https://github.com/scriptin/jmdict-simplified) | Dictionary, bundled in the app (English meanings) | CC BY-SA 4.0 |
| [VOICEVOX](https://voicevox.hiroshiba.jp) CORE, ONNX Runtime and the voice 春日部つむぎ | Spoken word and sentence, bundled in the app (`scripts/fetch-tts.sh`) | CORE: MIT; voice and runtime: VOICEVOX terms of use (credit "VOICEVOX:春日部つむぎ" required) |
| [Kanjium](https://github.com/mifunetoshiro/kanjium) `accents.txt` (pitch accents, by Uros O.) | Pitch accent, bundled in the app | CC BY-SA 4.0 |
| [Tatoeba](https://tatoeba.org) Japanese sentences | Word frequency ranks counted from them (`data/frequency-ja.tsv.gz`) | CC BY 2.0 FR |
| [Tauri](https://tauri.app) | App framework | MIT / Apache-2.0 |

The models and the ONNX Runtime library are not stored in this repository; `scripts/fetch-ocr-models.sh`
downloads them. The bundled dictionary file is built by `scripts/build-dictionary.sh` (`scripts/dev.sh appimage`
does it when it is missing). The app icon is original artwork (a star, not the Anki logo, which is a trademark).

No license has been chosen for this project's own code yet.
