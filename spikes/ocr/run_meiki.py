"""Scores MeikiOCR on the generated subtitle cases: character accuracy and speed on CPU."""
import json
import sys
import time
from collections import defaultdict
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


def normalize(text: str) -> str:
    return "".join(text.split()).replace("?", "？").replace(",", "、")


def main(cases_dir: str) -> None:
    root = Path(cases_dir)
    cases = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    ocr = MeikiOCR()
    ocr.run_ocr(cv2.imread(str(root / cases[0]["file"])))  # warm up

    by_group = defaultdict(lambda: {"chars": 0, "errors": 0, "exact": 0, "count": 0, "time": 0.0})
    failures = []
    for case in cases:
        image = cv2.imread(str(root / case["file"]))
        started = time.perf_counter()
        lines = ocr.run_ocr(image, det_threshold=0.5, rec_threshold=0.1)
        elapsed = time.perf_counter() - started
        found = normalize("".join(line["text"] for line in lines if line["text"]))
        expected = normalize(case["text"])
        errors = edit_distance(found, expected)
        for key in ("all", f"{case['label']}", f"{case['style']}", f"bg:{case['background']}"):
            group = by_group[key]
            group["chars"] += len(expected)
            group["errors"] += errors
            group["exact"] += int(found == expected)
            group["count"] += 1
            group["time"] += elapsed
        if errors:
            failures.append((case["file"], expected, found))

    print(f"{'group':<14}{'cases':>6}{'char acc':>10}{'exact':>8}{'ms/img':>8}")
    for key, group in sorted(by_group.items(), key=lambda item: (item[0] != "all", item[0])):
        accuracy = 100 * (1 - group["errors"] / group["chars"])
        print(f"{key:<14}{group['count']:>6}{accuracy:>9.1f}%{100 * group['exact'] / group['count']:>7.0f}%{1000 * group['time'] / group['count']:>8.0f}")
    print(f"\n{len(failures)} imperfect cases, first 12:")
    for name, expected, found in failures[:12]:
        print(f"  {name}\n    expected: {expected}\n    found:    {found}")


if __name__ == "__main__":
    main(sys.argv[1])
