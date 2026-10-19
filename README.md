# anki_mining

Look up Japanese words anywhere on your screen and turn them into Anki cards, with no browser extension.
Pause a video, move the mouse over a subtitle and a popup shows the word under the cursor with its reading,
dictionary form and meaning. One click (or Alt+M) adds it to Anki with its sentence, a screenshot and audio.

<p align="center"><img src="src-tauri/app-icon.png" alt="App icon: a star being mined with a pickaxe" width="128"></p>

## Look up words on any text

- **Works on anything you can see:** YouTube subtitles, TV banners, web pages, video players, games and
  other applications. The screen is read with an offline OCR, so nothing needs to be installed in the
  browser or the player.
- **Two ways to trigger it**, switchable in the app and remembered: hold **Shift** over the text, or let the
  cursor rest on it for about 300 ms.
- **The popup** appears next to the word with its reading, dictionary form, meanings, pitch accent and
  frequency rank. A **▶** button on each entry reads the word aloud.
- **Inflected words are understood:** `行きました` is found as `行く`, `食べた` as `食べる`.
- **Long lines and multi-line subtitles:** long web-page lines are read in full, and a subtitle of
  up to three lines is joined into one sentence. Vertical text is supported too.
- **Everything is local.** OCR, tokenizer and dictionary run on your machine; nothing is sent anywhere.

## Other text sources

The same popup and main window also work with text that does not come from the screen:

- **mpv:** the exact subtitles of the video, with their timing (add `input-ipc-server=/tmp/mpvsocket` to
  `mpv.conf`; on Windows, `input-ipc-server=mpvsocket`).
- **Clipboard:** copied text is analyzed as it arrives.
- **Websocket:** `ws://127.0.0.1:6677`, compatible with Textractor and owocr-style senders.
- **A text box** in the main window, and an optional **read a screen region** mode.

## Dictionary

The dictionary is part of the app, with nothing to download or import:

- the 218,000 JMdict entries with English meanings;
- the pitch accent of about 124,000 words, drawn as a line over the reading and written as `[0]`, `[1]`...;
- a frequency rank for the 18,000 most used words (`#337`).

In **Settings → Dictionary** you can add more pitch accent or frequency data from any Yomitan dictionary
`.zip` that has `term_meta_bank` files.

## Adding words to Anki

Click **Add to Anki** on the entry you want, or press **Alt+M** to add the first one. The button shows
"Adding…" while the card is made and "Added ✓" when it is done.

A card has:

- the word, its reading and its meanings;
- the sentence, with the word highlighted;
- a **screenshot** of the window the word was found in;
- **audio**: the last seconds of sound that were playing (for mpv, the exact subtitle), and a spoken word and
  sentence made by a natural-sounding offline neural voice ([VOICEVOX](https://voicevox.hiroshiba.jp),
  春日部つむぎ);
- the pitch accent, the frequency and the source.

Screenshot, recorded audio, the spoken voice and the longest clip are switchable in **Settings → Cards**.
When the card already exists in the deck you choose whether it is refused or added again.

The first card creates the note type `AnkiMining` and uses the deck `Mining`. In it the audio is on the back,
below the reading and above the meaning, and the explanation is a left-aligned block with a maximum width, so
it stays readable on very wide screens. A note type you already have is never changed (changing fields or
templates makes Anki ask for a full sync with AnkiWeb).

Anki must be open, with the [AnkiConnect](https://ankiweb.net/shared/info/2055492159) add-on.

## Words you already have

Every word of a sentence is underlined: a solid green line means the word already has a card in the deck,
a dashed grey line means it does not. Inflected forms count (`食べた` is checked as `食べる`). After you add a
word it turns green at once.

## Decks per application (profiles)

In **Settings** a profile has a deck, a note type, a field mapping and rules that decide when it is used: the
application (its window class, e.g. `mpv` or `Navigator`), a word in the window title (e.g. `YouTube`) and the
text source. The profile with the most matching rules wins; the default profile is used when none matches.

## The window

- **Always on top:** the main window stays above the other applications so it is visible over a video. Turn it
  off in **Settings → Application** to let the system decide which window is on top (the popup still shows
  over the text it belongs to).
- **System tray:** with **Keep running in the system tray**, the close button hides the window instead of
  quitting; use the tray menu to quit.
- **Updates:** the app looks for a newer version on GitHub a little after it starts and offers
  **Install and restart**. You can turn this off, or check by hand, in **Settings → Application**. Every update
  is signed.

## Platforms

- **Windows 10/11** (64-bit): `AnkiMining_<version>_x64-setup.exe`. The installer is not signed with a Microsoft
  certificate, so SmartScreen may say "unknown publisher": choose *More info* and *Run anyway*.
- **Linux with X11:** `AnkiMining_<version>_amd64.AppImage`; make it executable and run it. The sentence audio is
  recorded with `parec` (PulseAudio or PipeWire) and the ▶ buttons play through `paplay`, `pw-play` or `aplay`.
  Wayland and macOS are not supported.

Downloads are on the [releases page](https://github.com/monothread/anki_mining/releases/latest).

## Limitations

- Overlapping subtitle layers can confuse the OCR, and rare kanji in stylised fonts can be misread. The word
  under the cursor is usually right; the sentence may be noisy.
- Vertical text is less accurate than horizontal text.
- Content without subtitles on screen is not supported (there is nothing to point at).

## Credits

MeikiOCR (text recognition), ONNX Runtime, Lindera with UniDic (tokenizer), JMdict (CC BY-SA 4.0), Kanjium
(pitch accent, CC BY-SA 4.0), Tatoeba (frequency ranks, CC BY 2.0 FR), Tauri, and the VOICEVOX voice
**VOICEVOX:春日部つむぎ**, which must be credited wherever audio made with it is published. For development,
architecture and decisions see [docs/](docs/PROGRESS.md).
