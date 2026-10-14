"""Scores MeikiOCR (vertical path included) on the vertical cases: character accuracy."""
import json
import sys
import time
from pathlib import Path

import cv2
from meikiocr import MeikiOCR


def edit_distance(a: str, b: str) -> int:
    previous = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        current = [i]
        for j, cb in enumerate(b, 1):
            current.append(min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + (ca != cb)))
        previous = current
    return previous[-1]


def main(cases_dir: str) -> None:
    root = Path(cases_dir)
    cases = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    ocr = MeikiOCR()
    total_chars = total_errors = 0
    started = time.perf_counter()
    for case in cases:
        image = cv2.imread(str(root / case["file"]))
        lines = ocr.run_ocr(image, det_threshold=0.5, rec_threshold=0.1)
        # Columns are read right to left: order the lines by their right edge.
        ordered = sorted((line for line in lines if line["text"] and line["chars"]), key=lambda line: -max(c["bbox"][2] for c in line["chars"]))
        found = "".join(line["text"] for line in ordered).replace("。", "").replace("、", "")
        expected = case["text"].replace("。", "").replace("、", "")
        errors = edit_distance(found, expected)
        total_chars += len(expected)
        total_errors += errors
        flags = [bool(line["is_vertical"]) for line in lines if line["text"]]
        marker = "ok " if errors == 0 else "BAD"
        print(f"{marker} {case['file']:<34} vertical={flags} {found}")
    print(f"\nchar accuracy {100 * (1 - total_errors / total_chars):.1f}% over {len(cases)} cases, {1000 * (time.perf_counter() - started) / len(cases):.0f} ms/image")


if __name__ == "__main__":
    main(sys.argv[1])
