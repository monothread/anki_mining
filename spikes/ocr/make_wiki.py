"""A Wikipedia-like screenshot: title, long paragraphs, blue underlined links, small text."""
import sys

from PIL import Image, ImageDraw, ImageFont

REGULAR = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
BOLD = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"

PARAGRAPHS = [
    "日本語は主に日本国内や日本人同士の間で使われている言語であり、日本の事実上の公用語として扱われている。",
    "文字は漢字、ひらがな、カタカナ、アルファベットなどを組み合わせて書かれ、世界の言語の中でも特に複雑な表記体系を持つとされている。",
    "古くは中国から漢字が伝わり、その後に独自の仮名文字が生まれて、現在の日本語の書き言葉の基礎が形づくられていった。",
]
LINKS = ["漢字", "ひらがな", "カタカナ"]


def main(path: str) -> None:
    image = Image.new("RGB", (1920, 1080), (255, 255, 255))
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, 260, 1080), fill=(246, 246, 246))
    draw.text((300, 440), "日本語", font=ImageFont.truetype(BOLD, 38), fill=(0, 0, 0))
    draw.line((300, 495, 1800, 495), fill=(170, 170, 170), width=1)
    body = ImageFont.truetype(REGULAR, 16)
    y = 530
    for paragraph in PARAGRAPHS:
        x = 300
        draw.text((x, y), paragraph, font=body, fill=(32, 33, 34))
        for link in LINKS:
            index = paragraph.find(link)
            if index >= 0:
                prefix = draw.textlength(paragraph[:index], font=body)
                width = draw.textlength(link, font=body)
                draw.text((x + prefix, y), link, font=body, fill=(6, 69, 173))
                draw.line((x + prefix, y + 19, x + prefix + width, y + 19), fill=(6, 69, 173), width=1)
        y += 60
    image.save(path)
    print(f"wrote {path}")


if __name__ == "__main__":
    main(sys.argv[1])
