"""Applies video-like degradations to the generated cases and keeps the expected text."""
import io
import json
import random
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter


def jpeg(image: Image.Image, quality: int) -> Image.Image:
    buffer = io.BytesIO()
    image.save(buffer, "JPEG", quality=quality)
    buffer.seek(0)
    return Image.open(buffer).convert("RGB")


def noise(image: Image.Image, sigma: float, seed: int) -> Image.Image:
    array = np.asarray(image, dtype=np.float32)
    array += np.random.default_rng(seed).normal(0, sigma, array.shape)
    return Image.fromarray(np.clip(array, 0, 255).astype(np.uint8))


def downscale_then_up(image: Image.Image, factor: float) -> Image.Image:
    small = image.resize((int(image.width * factor), int(image.height * factor)), Image.BILINEAR)
    return small.resize(image.size, Image.BILINEAR)


DEGRADATIONS = {
    "jpeg25": lambda image, seed: jpeg(image, 25),
    "blur1.2": lambda image, seed: image.filter(ImageFilter.GaussianBlur(1.2)),
    "noise18": lambda image, seed: noise(image, 18, seed),
    "half_res": lambda image, seed: downscale_then_up(image, 0.5),
    "combo": lambda image, seed: jpeg(noise(downscale_then_up(image, 0.7).filter(ImageFilter.GaussianBlur(0.8)), 10, seed), 35),
}


def main(source: str, target: str) -> None:
    src, dst = Path(source), Path(target)
    dst.mkdir(parents=True, exist_ok=True)
    cases = json.loads((src / "cases.json").read_text(encoding="utf-8"))
    rng = random.Random(3)
    selected = [case for case in cases if case["label"] in ("1080p", "720p")]
    out_cases = []
    for case in selected:
        image = Image.open(src / case["file"]).convert("RGB")
        crop_top = int(image.height * 0.68)
        for name, degrade in DEGRADATIONS.items():
            result = degrade(image, rng.randint(0, 10_000))
            region = result.crop((0, crop_top, result.width, result.height))  # subtitle band only
            file = f"{Path(case['file']).stem}__{name}.png"
            region.save(dst / file)
            out_cases.append({**case, "file": file, "label": name})
    (dst / "cases.json").write_text(json.dumps(out_cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {len(out_cases)} degraded subtitle-band cases")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
