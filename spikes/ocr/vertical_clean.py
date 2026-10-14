import sys
sys.path.insert(0, "/spike")
import cv2, numpy as np
from PIL import Image, ImageDraw, ImageFont
from meikiocr import MeikiOCR
from run_vertical import edit_distance

REGULAR = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
BOLD = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
TEXTS = ["昨日友達と映画を見に行きました", "彼女は先生になりたがっている", "日本語を勉強しています", "美しかったので買ってしまった"]
ocr = MeikiOCR()
for label, path, size, gap in (("regular 40", REGULAR, 40, 0), ("regular 64", REGULAR, 64, 0), ("bold 40", BOLD, 40, 0), ("regular 40 gap6", REGULAR, 40, 6)):
    total = errors = 0
    for text in TEXTS:
        w, h = size + 120, len(text) * (size + gap) + 100
        image = Image.new("RGB", (w, h), (255, 255, 255))
        draw = ImageDraw.Draw(image)
        font = ImageFont.truetype(path, size)
        y = 50
        for ch in text:
            draw.text((60, y), ch, font=font, fill=(0, 0, 0))
            y += size + gap
        lines = ocr.run_ocr(cv2.cvtColor(np.asarray(image), cv2.COLOR_RGB2BGR), det_threshold=0.5, rec_threshold=0.1)
        found = "".join(l["text"] for l in lines)
        errors += edit_distance(found, text)
        total += len(text)
    print(f"{label:<18} accuracy {100 * (1 - errors / total):.1f}%")
