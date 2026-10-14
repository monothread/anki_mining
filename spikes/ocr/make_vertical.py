"""Renders vertical Japanese text (top to bottom, columns right to left) with their expected text.

Characters are placed one by one; small punctuation goes to the upper right of its cell, like in
vertical typesetting.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
SMALL_PUNCTUATION = set("、。")
COLUMNS = [
    ["昨日、友達と映画を見に行きました。"],
    ["彼女は先生になりたがっている", "美しかったので買ってしまった"],
    ["日本語を勉強しています"],
]


def draw_column(draw: ImageDraw.ImageDraw, text: str, x: int, y: int, size: int, font, light: bool) -> int:
    fill, stroke = ((255, 255, 255), (0, 0, 0)) if light else ((20, 20, 20), (255, 255, 255))
    for character in text:
        position = (x, y)
        if character in SMALL_PUNCTUATION:
            position = (x + int(size * 0.55), y - int(size * 0.45))
        draw.text(position, character, font=font, fill=fill, stroke_width=max(2, size // 14), stroke_fill=stroke)
        y += size
    return y


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    cases = []
    rng = np.random.default_rng(9)
    for index, columns in enumerate(COLUMNS):
        for size in (36, 56):
            for light in (True, False):
                width = 120 + len(columns) * (size + 30)
                longest = max(len(text) for text in columns)
                height = 100 + longest * size
                base = 200 if light is False else 60
                noise = rng.normal(0, 18, (height, width, 3))
                image = Image.fromarray(np.clip(base + noise, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1))
                draw = ImageDraw.Draw(image)
                font = ImageFont.truetype(FONT, size)
                # Columns are read right to left.
                for column, text in enumerate(columns):
                    x = width - 60 - size - column * (size + 30)
                    draw_column(draw, text, x, 50, size, font, light)
                name = f"vertical_{index}_{size}px_{'light' if light else 'dark'}.png"
                image.save(out / name)
                cases.append({"file": name, "text": "".join(columns)})
    (out / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {len(cases)} vertical cases")


if __name__ == "__main__":
    main(sys.argv[1])
