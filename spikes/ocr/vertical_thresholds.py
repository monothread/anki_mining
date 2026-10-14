import json
import sys
from pathlib import Path

import cv2
from meikiocr import MeikiOCR

from run_vertical import edit_distance


def score(ocr, root, cases, det, rec):
    total = errors = 0
    for case in cases:
        image = cv2.imread(str(root / case["file"]))
        lines = ocr.run_ocr(image, det_threshold=det, rec_threshold=rec)
        ordered = sorted((l for l in lines if l["text"] and l["chars"]), key=lambda l: -max(c["bbox"][2] for c in l["chars"]))
        found = "".join(l["text"] for l in ordered).replace("。", "").replace("、", "")
        expected = case["text"].replace("。", "").replace("、", "")
        total += len(expected)
        errors += edit_distance(found, expected)
    return 100 * (1 - errors / total)


root = Path(sys.argv[1])
cases = json.loads((root / "cases.json").read_text(encoding="utf-8"))
ocr = MeikiOCR()
print("det  rec   accuracy")
for det in (0.3, 0.5, 0.7):
    for rec in (0.02, 0.1, 0.3):
        print(f"{det:<4} {rec:<5} {score(ocr, root, cases, det, rec):.1f}%")
