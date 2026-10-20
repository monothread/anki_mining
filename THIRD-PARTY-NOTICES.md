# Third-party notices

The source code of AnkiMining is released under the [MIT License](LICENSE). The application also bundles or
loads the components below, each under its own license. They are separate files, not part of the MIT-licensed
source code. Licenses were checked against the upstream pages on 2026-10-19.

## OCR

| Component | License | Notes |
|---|---|---|
| MeikiOCR models (`meiki.text.detect.v0.1.960x544.onnx`, `meiki.text.rec.v0.960x32.onnx`, `meiki.text.rec.v0.vertical.32x480.onnx`) | LGPL-3.0 | By rtr46, https://huggingface.co/rtr46/meiki.text.detect.v0 and https://huggingface.co/rtr46/meiki.txt.recognition.v0. Shipped unmodified as separate files in the `ocr/` folder of the application, which can be replaced by another build of the same models. License text: https://www.gnu.org/licenses/lgpl-3.0.txt |
| MeikiOCR pipeline (ported to Rust in `src-tauri/src/ocr/`) | Apache-2.0 | https://github.com/rtr46/meikiocr |
| ONNX Runtime | MIT | https://github.com/microsoft/onnxruntime |

## Text analysis and dictionary data

| Component | License | Notes |
|---|---|---|
| Lindera | MIT | https://github.com/lindera/lindera |
| UniDic (embedded in the binary) | GPL 2.0 / LGPL 2.1 / New BSD (the three are offered by the UniDic Consortium); this application uses it under the New BSD terms | https://clrd.ninjal.ac.jp/unidic/ |
| JMdict, by the Electronic Dictionary Research and Development Group (EDRDG), through jmdict-simplified | EDRDG License; derived files under CC BY-SA 4.0 | https://www.edrdg.org/edrdg/licence.html and https://github.com/scriptin/jmdict-simplified. `jmdict.db` is a derived work (converted to SQLite) and stays under CC BY-SA 4.0 |
| Kanjium pitch accents (`accents.txt`) | CC BY-SA 4.0 | https://github.com/mifunetoshiro/kanjium. Its author asks that the pitch accent notation be credited to Uros O. The `pitch` table of `jmdict.db` is derived from it |
| Frequency list (`data/frequency-ja.tsv.gz`) | CC BY 2.0 FR | Word counts made with this application's tokenizer from the Japanese sentences of Tatoeba, https://tatoeba.org (some sentences are CC0) |

CC BY-SA 4.0: https://creativecommons.org/licenses/by-sa/4.0/ · CC BY 2.0 FR: https://creativecommons.org/licenses/by/2.0/fr/

## Speech

| Component | License | Notes |
|---|---|---|
| VOICEVOX CORE | MIT | https://github.com/VOICEVOX/voicevox_core (license copied as `tts/VOICEVOX-CORE-LICENSE`) |
| VOICEVOX voice model and VOICEVOX ONNX Runtime | VOICEVOX terms of use | Copied as `tts/VOICEVOX-TERMS.txt` and `tts/VOICEVOX-ONNXRUNTIME-TERMS.txt`. Commercial and non-commercial use and redistribution inside an application are allowed; the credit **VOICEVOX:春日部つむぎ** is required wherever audio made with the voice is published or shared; reverse engineering is forbidden; anyone who is allowed to use the generated audio must in turn follow items 3 and 4 of the terms |
| Voice 春日部つむぎ | https://tsumugi-official.studio.site/rule | Credit "VOICEVOX:春日部つむぎ" |
| Open JTalk dictionary (NAIST Japanese Dictionary) | BSD-style | Copyright (c) 2009 Nara Institute of Science and Technology; the license text is `tts/open_jtalk_dic_utf_8-1.11/COPYING` |

## Application framework and libraries

- Tauri and its plugins: MIT or Apache-2.0, https://tauri.app
- The Rust crates and the npm package used are under permissive licenses (MIT, Apache-2.0, BSD, ISC, Zlib,
  Unicode-3.0, CC0, CDLA-Permissive-2.0 and similar), plus MPL-2.0 for five unmodified crates (`cssparser`,
  `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`), whose source is available on crates.io. None
  of the Rust crates is under the GPL or LGPL (checked with `cargo metadata` for the Linux target).
