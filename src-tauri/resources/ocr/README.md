# OCR models

MeikiOCR ONNX models used for on-screen text recognition, downloaded by `scripts/fetch-ocr-models.sh`
(not stored in git).

| File | Source | License |
|---|---|---|
| `meiki.text.detect.v0.1.960x544.onnx` | https://huggingface.co/rtr46/meiki.text.detect.v0 | LGPL-3.0 (per the model card) |
| `meiki.text.rec.v0.960x32.onnx` | https://huggingface.co/rtr46/meiki.txt.recognition.v0 | LGPL-3.0 (per the model card) |
| `meiki.text.rec.v0.vertical.32x480.onnx` (vertical text, beta) | https://huggingface.co/rtr46/meiki.txt.recognition.v0 | LGPL-3.0 (per the model card) |

The MeikiOCR pipeline code (https://github.com/rtr46/meikiocr, Apache-2.0) was ported to Rust in
`src-tauri/src/ocr/`. The licenses were re-checked on the model pages on 2026-10-19 (both cards say LGPL-3.0; the meikiocr README
only says "permissive"). The full list is in `THIRD-PARTY-NOTICES.md`.
