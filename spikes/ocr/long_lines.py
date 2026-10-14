"""Do long lines of text (web pages) get read completely?"""
import cv2
import numpy as np
from meikiocr import MeikiOCR
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
BASE = "日本語は主にひらがなカタカナ漢字の三種類の文字を組み合わせて書かれる言語であり、世界でも珍しい表記体系を持っている。"


def page(chars_per_line: int, size: int) -> tuple[Image.Image, list[str]]:
    text = (BASE * 3)[: chars_per_line]
    image = Image.new("RGB", (1920, 400), (255, 255, 255))
    draw = ImageDraw.Draw(image)
    font = ImageFont.truetype(FONT, size)
    draw.text((100, 100), text, font=font, fill=(30, 30, 30))
    return image, [text]


def main() -> None:
    ocr = MeikiOCR()
    print(f"{'chars':>5} {'px':>3} {'width':>6}  read / expected  boxes")
    for size in (16, 24):
        for chars in (20, 40, 48, 60, 80, 100):
            image, expected = page(chars, size)
            bgr = cv2.cvtColor(np.asarray(image), cv2.COLOR_RGB2BGR)
            lines = ocr.run_ocr(bgr, det_threshold=0.5, rec_threshold=0.1)
            read = "".join(line["text"] for line in lines)
            width = max((line["chars"][-1]["bbox"][2] for line in lines if line["chars"]), default=0)
            print(f"{chars:>5} {size:>3} {width:>6}  {len(read):>3}/{len(expected[0]):<3} boxes={len(lines)}  {read[:14]}…{read[-6:]}")


if __name__ == "__main__":
    main()
