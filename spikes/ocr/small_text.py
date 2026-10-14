"""Does the OCR read small text? Compares the whole screen with a native-size crop around the cursor."""
import sys
import time

import cv2
import numpy as np
from meikiocr import MeikiOCR
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
SENTENCE = "昨日、友達と映画を見に行きました。"
SIZES = [12, 14, 16, 18, 20, 24, 32, 48]
STYLES = {
    "link": ((0, 0, 238), (255, 255, 255), True),    # blue underlined text on white (web link)
    "page": ((30, 30, 30), (255, 255, 255), False),  # black text on white
    "caption": ((255, 255, 255), (0, 0, 0), False),   # white on black box (YouTube caption)
}


def screen(size: int, style: str) -> tuple[Image.Image, tuple[int, int]]:
    fg, bg, underline = STYLES[style]
    image = Image.new("RGB", (1920, 1080), (200, 200, 200))
    font = ImageFont.truetype(FONT, size)
    draw = ImageDraw.Draw(image)
    left, top, right, bottom = draw.textbbox((0, 0), SENTENCE, font=font)
    x, y = 600, 700
    pad = max(4, size // 4)
    draw.rectangle((x - pad, y - pad, x + right - left + pad, y + bottom - top + pad), fill=bg)
    draw.text((x, y), SENTENCE, font=font, fill=fg)
    if underline:
        draw.line((x, y + bottom - top + 2, x + right - left, y + bottom - top + 2), fill=fg, width=1)
    cursor = (x + (right - left) // 3, y + (bottom - top) // 2)
    return image, cursor


def crop_around(image: Image.Image, cursor: tuple[int, int], width: int = 960, height: int = 544) -> np.ndarray:
    left = min(max(cursor[0] - width // 2, 0), image.width - width)
    top = min(max(cursor[1] - height // 2, 0), image.height - height)
    return cv2.cvtColor(np.asarray(image.crop((left, top, left + width, top + height))), cv2.COLOR_RGB2BGR)


def read(ocr: MeikiOCR, bgr: np.ndarray) -> str:
    return "".join(line["text"] for line in ocr.run_ocr(bgr, det_threshold=0.5, rec_threshold=0.1) if line["text"])


def main() -> None:
    ocr = MeikiOCR()
    print(f"{'style':<9}{'px':>4}  whole screen / crop around cursor (native size)")
    for style in STYLES:
        for size in SIZES:
            image, cursor = screen(size, style)
            whole = read(ocr, cv2.cvtColor(np.asarray(image), cv2.COLOR_RGB2BGR))
            near = read(ocr, crop_around(image, cursor))
            print(f"{style:<9}{size:>4}  {'OK ' if whole == SENTENCE else 'BAD'} {whole[:20]:<22} | {'OK ' if near == SENTENCE else 'BAD'} {near[:20]}")


if __name__ == "__main__":
    main()
