"""Renders single long lines of Japanese text, like a web page paragraph, with their expected text."""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
TEXT = (
    "日本語は主にひらがなカタカナ漢字の三種類の文字を組み合わせて書かれる言語であり、"
    "世界でも珍しい表記体系を持っている。古くは中国から漢字が伝わり、その後に独自の仮名文字が生まれた。"
    "現在では新聞や書籍だけでなく、インターネット上でも多くの人々が日々日本語を使って情報を発信している。"
)


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    cases = []
    # Only lengths that fit in the 1920 px wide image (full-width characters are `size` px wide).
    lengths = {16: (60, 80, 100, 110), 20: (50, 70, 85), 24: (40, 60, 72)}
    for size, sizes in lengths.items():
        for length in sizes:
            text = TEXT[:length]
            width = 1920
            image = Image.new("RGB", (width, 200), (255, 255, 255))
            draw = ImageDraw.Draw(image)
            font = ImageFont.truetype(FONT, size)
            draw.text((60, 80), text, font=font, fill=(30, 30, 30))
            name = f"long_{size}px_{length}c.png"
            image.save(out / name)
            cases.append({"file": name, "text": text})
    (out / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {len(cases)} long-line cases")


if __name__ == "__main__":
    main(sys.argv[1])
