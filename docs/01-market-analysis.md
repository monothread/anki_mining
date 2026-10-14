# Market analysis

## Existing tools

| Tool | Type | Strengths | Limitations |
|---|---|---|---|
| Yomitan (ex-Yomichan) | Browser extension | De facto standard; Yomitan-format dictionaries (JMdict, frequency, pitch); deinflection; export via AnkiConnect | DOM text only; cannot read video, games or images |
| asbplayer | Extension + web app (MIT) | Local and streaming subtitles; automatic audio and screenshot; uses Yomitan for lookup | Browser-dependent; 4-piece setup (extension, Yomitan, Anki, AnkiConnect) |
| Migaku | Paid; Chromium + mobile | Easy setup; Netflix, Disney+, Viki; ready-made cards | Paid and closed; no games or desktop apps |
| Language Reactor | Freemium; Chrome | Netflix and YouTube | Anki export only on Pro; Chrome only |
| LunaTranslator / Textractor | Windows; hook, OCR, clipboard | Visual novels and games; AnkiConnect and Yomitan integrated | Windows-first; translation-focused |
| owocr / manga-ocr / meikiocr | Local OCR via clipboard/websocket | Good Japanese OCR quality | Isolated piece, no mining UI |
| Kamite | Desktop (Linux/Windows) | Immersion companion with OCR; closest to this vision | Niche; less polished |
| Anki Miner | Python; media → batch cards | Per-episode automation | No interactive lookup |

## How the community studies (sentence mining)
1. Watch content (anime, YouTube, games, visual novels).
2. Pause on an unknown subtitle or sentence.
3. Hover over the word (dictionary popup).
4. Send it to Anki with the context sentence, sentence audio and screenshot ("i+1" cards).
5. Separate decks per media or type; standardized fields (Expression, Reading, Meaning, Sentence, Audio, Picture, Frequency, Pitch).

## Gap
No tool covers web, local video, games and desktop apps **on Linux** in a single experience, with its own tokenization and decks per source. This project fills that gap.

## Product differentiators
- Pluggable text sources (clipboard, websocket, OCR, extension, mpv).
- Global overlay independent of the browser.
- Per-application profiles → deck and note type.
- Integrated audio and screenshot capture.
- Works offline (local dictionary and OCR).

## Sources
- https://lucas.art/blog/japanese-immersion-options-for-entence-mining/
- https://lexirise.app/blog/article/language-reactor-alternative
- https://alternativeto.net/software/asbplayer
- https://github.com/fauu/Kamite/
- https://github.com/0xzerolight/anki_miner_game
- https://www.wares.gg/apps/lunatranslator
- https://arxiv.org/pdf/2406.17185 (Vaporetto/Vibrato)
- https://pypi.org/project/anki-miner/2.6.6/

Note: the sources are largely comparison blogs and promotional material; prices and platform support must be re-verified on the official sites.
