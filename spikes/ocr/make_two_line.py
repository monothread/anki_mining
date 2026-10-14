"""A two-line outlined subtitle over a busy background, like a TV or anime subtitle."""
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
LINES = ["昨日、友達と映画を見に", "行きました。とても楽しかったです"]


def main(path: str) -> None:
    width, height, size = 1920, 340, 56
    noise = np.random.default_rng(4).normal(0, 26, (height, width, 3))
    gradient = np.tile(np.linspace(50, 210, width, dtype=np.float32), (height, 1))[:, :, None].repeat(3, axis=2)
    image = Image.fromarray(np.clip(gradient + noise, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1))
    draw = ImageDraw.Draw(image)
    font = ImageFont.truetype(FONT, size)
    for index, text in enumerate(LINES):
        left, _, right, _ = draw.textbbox((0, 0), text, font=font)
        x = (width - (right - left)) // 2
        y = 70 + index * 90
        draw.text((x, y), text, font=font, fill=(255, 255, 255), stroke_width=5, stroke_fill=(0, 0, 0))
    image.save(path)
    print("wrote", path)


if __name__ == "__main__":
    main(sys.argv[1])
