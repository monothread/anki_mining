"""Clean vertical text (regular weight, no outline), columns right to left, for the Rust tests."""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REGULAR = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
CASES = [
    (["昨日友達と映画を見に行きました"], 40),
    (["日本語を勉強しています"], 40),
    (["彼女は先生になりたがっている"], 40),
    (["美しかったので買ってしまった"], 40),
    (["世界で一番大切な人を守りたい"], 48),
    (["日本語を勉強しています", "世界で一番大切な人"], 40),
    (["明日の朝までに終わらせないと", "どうしてそんなことを言うの"], 40),
]


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    cases = []
    for index, (columns, size) in enumerate(CASES):
        width = 120 + len(columns) * (size + 40)
        height = 100 + max(len(text) for text in columns) * size
        image = Image.new("RGB", (width, height), (255, 255, 255))
        draw = ImageDraw.Draw(image)
        font = ImageFont.truetype(REGULAR, size)
        for column, text in enumerate(columns):
            x = width - 60 - size - column * (size + 40)
            for row, character in enumerate(text):
                draw.text((x, 50 + row * size), character, font=font, fill=(0, 0, 0))
        name = f"vertical_clean_{index}.png"
        image.save(out / name)
        cases.append({"file": name, "text": "".join(columns)})
    (out / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {len(cases)} clean vertical cases")


if __name__ == "__main__":
    main(sys.argv[1])
