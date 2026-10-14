"""Renders Japanese subtitle test images in styles seen on video and in games.

Each case is a PNG plus the exact text drawn, so OCR output can be scored.
"""
import json
import random
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT_BOLD = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
FONT_REGULAR = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
FONT_SERIF = "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc"

SENTENCES = [
    "昨日、友達と映画を見に行きました。",
    "この本は読みやすいと思います",
    "彼女は先生になりたがっている",
    "食べさせられたくなかったのに",
    "日本語を勉強しています",
    "美しかったので買ってしまった",
    "どうしてそんなことを言うの？",
    "明日の朝までに終わらせないと",
    "ねえ、聞いてる？ちゃんと答えてよ",
    "世界で一番大切な人を守りたい",
]


def background(width: int, height: int, kind: str, rng: random.Random) -> Image.Image:
    if kind == "dark":
        base = np.full((height, width, 3), 25, dtype=np.uint8)
    elif kind == "bright":
        base = np.full((height, width, 3), 225, dtype=np.uint8)
    else:  # busy scene: gradient plus noise blobs
        xs = np.linspace(40, 215, width, dtype=np.float32)
        base = np.tile(xs, (height, 1))[:, :, None].repeat(3, axis=2)
        base += np.random.default_rng(rng.randint(0, 10_000)).normal(0, 28, base.shape)
        base = np.clip(base, 0, 255).astype(np.uint8)
    image = Image.fromarray(base)
    if kind == "busy":
        draw = ImageDraw.Draw(image)
        for _ in range(18):
            x, y = rng.randint(0, width), rng.randint(0, height)
            r = rng.randint(20, 120)
            shade = rng.randint(0, 255)
            draw.ellipse((x - r, y - r, x + r, y + r), fill=(shade, shade, 255 - shade))
        image = image.filter(ImageFilter.GaussianBlur(2))
    return image


def draw_subtitle(image: Image.Image, text: str, size: int, style: str) -> None:
    font_path = FONT_SERIF if style == "serif" else (FONT_REGULAR if style == "thin" else FONT_BOLD)
    font = ImageFont.truetype(font_path, size)
    draw = ImageDraw.Draw(image)
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    x = (image.width - (right - left)) // 2
    y = image.height - (bottom - top) - int(size * 1.6)
    if style == "box":
        pad = size // 4
        overlay = Image.new("RGBA", image.size, (0, 0, 0, 0))
        ImageDraw.Draw(overlay).rectangle(
            (x - pad, y - pad, x + right - left + pad, y + bottom - top + pad * 2),
            fill=(0, 0, 0, 170),
        )
        image.paste(Image.alpha_composite(image.convert("RGBA"), overlay).convert("RGB"))
        draw = ImageDraw.Draw(image)
        draw.text((x, y), text, font=font, fill=(255, 255, 255))
    elif style == "yellow":
        draw.text((x, y), text, font=font, fill=(255, 235, 60), stroke_width=max(2, size // 14), stroke_fill=(0, 0, 0))
    else:
        draw.text((x, y), text, font=font, fill=(255, 255, 255), stroke_width=max(2, size // 12), stroke_fill=(0, 0, 0))


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(7)
    cases = []
    configs = [
        ("1080p", 1920, 1080, 56, "outline", "busy"),
        ("1080p", 1920, 1080, 56, "yellow", "busy"),
        ("1080p", 1920, 1080, 48, "box", "bright"),
        ("1080p", 1920, 1080, 52, "outline", "dark"),
        ("720p", 1280, 720, 38, "outline", "busy"),
        ("720p", 1280, 720, 34, "thin", "dark"),
        ("720p", 1280, 720, 36, "serif", "bright"),
        ("480p", 854, 480, 26, "outline", "busy"),
    ]
    for index, sentence in enumerate(SENTENCES):
        for label, width, height, size, style, bg in configs:
            image = background(width, height, bg, rng)
            draw_subtitle(image, sentence, size, style)
            name = f"{index:02d}_{label}_{style}_{bg}.png"
            image.save(out / name)
            cases.append({"file": name, "text": sentence, "label": label, "style": style, "background": bg, "font_size": size})
    (out / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {len(cases)} cases to {out}")


if __name__ == "__main__":
    main(sys.argv[1])
