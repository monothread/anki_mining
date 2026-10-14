"""Draws the app icon: a star being mined with a pickaxe, chips flying off.

Usage: python make_icon.py <output.png>   (needs Pillow)
"""
import math
import random
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter

SIZE = 1024
SCALE = 2  # supersampling
S = SIZE * SCALE


def p(x: float, y: float) -> tuple[float, float]:
    return (x * SCALE, y * SCALE)


def vertical_gradient(top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    gradient = Image.new("RGB", (S, S))
    draw = ImageDraw.Draw(gradient)
    for y in range(S):
        t = y / (S - 1)
        draw.line((0, y, S, y), fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return gradient


def star_points(cx: float, cy: float, outer: float, inner: float, rotation: float = -90.0) -> list[tuple[float, float]]:
    points = []
    for index in range(10):
        radius = outer if index % 2 == 0 else inner
        angle = math.radians(rotation + index * 36)
        points.append(p(cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
    return points


def rounded_polygon_mask(points: list[tuple[float, float]], radius: float) -> Image.Image:
    """A polygon with rounded corners: filled polygon plus a round-joined outline."""
    mask = Image.new("L", (S, S), 0)
    draw = ImageDraw.Draw(mask)
    draw.polygon(points, fill=255)
    draw.line(points + [points[0]], fill=255, width=round(radius * 2), joint="curve")
    for x, y in points:
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=255)
    return mask


def tapered_stroke(centerline: list[tuple[float, float]], width: float, taper: float = 0.7) -> list[tuple[float, float]]:
    """A polygon around a polyline whose width shrinks to points at both ends."""
    left, right = [], []
    count = len(centerline)
    for index, (x, y) in enumerate(centerline):
        before = centerline[max(index - 1, 0)]
        after = centerline[min(index + 1, count - 1)]
        dx, dy = after[0] - before[0], after[1] - before[1]
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length, dx / length
        t = index / (count - 1)
        half = width / 2 * (math.sin(math.pi * t) ** taper)
        left.append((x + nx * half, y + ny * half))
        right.append((x - nx * half, y - ny * half))
    return left + right[::-1]


def bezier(p0, p1, p2, steps: int = 60) -> list[tuple[float, float]]:
    return [
        (
            (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t**2 * p2[0],
            (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t**2 * p2[1],
        )
        for t in (i / steps for i in range(steps + 1))
    ]


def pickaxe_layer(pivot: tuple[float, float], tilt: float) -> Image.Image:
    """A pickaxe drawn upright (head at the bottom, shaped like a U, shaft rising from the middle
    of its concave side, as on a real pickaxe) and then tilted around `pivot`, its lowest point."""
    px, py = pivot
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    def at(x: float, y: float) -> tuple[float, float]:
        return p(px + x, py + y)

    # Shaft.
    shaft = Image.new("L", (S, S), 0)
    sd = ImageDraw.Draw(shaft)
    sd.line([at(0, -20), at(0, -400)], fill=255, width=46 * SCALE)
    for y in (-20, -400):
        x0, y0 = at(0, y)
        sd.ellipse((x0 - 23 * SCALE, y0 - 23 * SCALE, x0 + 23 * SCALE, y0 + 23 * SCALE), fill=255)
    wood = vertical_gradient((226, 164, 92), (150, 92, 40)).convert("RGBA")
    layer.paste((70, 40, 18, 255), (0, 0), shaft.filter(ImageFilter.MaxFilter(11 * SCALE | 1)))
    layer.paste(wood, (0, 0), shaft)
    grain = Image.new("L", (S, S), 0)
    ImageDraw.Draw(grain).line([at(-9, -60), at(-9, -380)], fill=70, width=5 * SCALE)
    layer.paste((255, 235, 200, 255), (0, 0), grain)

    # Head: a U whose tips curve towards the shaft, pointed at both ends.
    centerline = [at(*xy) for xy in bezier((-190, -150), (0, 120), (190, -150))]
    head = Image.new("L", (S, S), 0)
    ImageDraw.Draw(head).polygon(tapered_stroke(centerline, 100 * SCALE, 0.75), fill=255)
    steel = vertical_gradient((236, 241, 247), (104, 116, 136)).convert("RGBA")
    layer.paste((24, 30, 44, 255), (0, 0), head.filter(ImageFilter.MaxFilter(9 * SCALE | 1)))
    layer.paste(steel, (0, 0), head)
    highlight = Image.new("L", (S, S), 0)
    ImageDraw.Draw(highlight).line(centerline[6:34], fill=120, width=10 * SCALE)
    layer.paste((255, 255, 255, 255), (0, 0), ImageChops.multiply(highlight, head))

    # Collar where the shaft goes through the head.
    collar = Image.new("L", (S, S), 0)
    ImageDraw.Draw(collar).rounded_rectangle((at(-30, -62), at(30, -2)), radius=14 * SCALE, fill=255)
    layer.paste((30, 36, 50, 255), (0, 0), collar.filter(ImageFilter.MaxFilter(7 * SCALE | 1)))
    layer.paste((96, 108, 128, 255), (0, 0), collar)

    return layer.rotate(tilt, center=p(px, py), resample=Image.BICUBIC)


def main(path: str) -> None:
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # Background tile.
    tile_mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(tile_mask).rounded_rectangle((p(40, 40), p(984, 984)), radius=210 * SCALE, fill=255)
    background = vertical_gradient((38, 52, 84), (16, 22, 40)).convert("RGBA")
    canvas.paste(background, (0, 0), tile_mask)

    # The star.
    star = rounded_polygon_mask(star_points(440, 580, 350, 168), 34 * SCALE)
    # A big jagged chunk is broken out of the upper right arm where the pickaxe hits.
    notch = Image.new("L", (S, S), 0)
    ImageDraw.Draw(notch).polygon(
        [
            p(*xy)
            for xy in [
                (610, 372), (668, 396), (655, 440), (712, 452), (700, 500),
                (748, 520), (830, 520), (830, 330), (650, 330),
            ]
        ],
        fill=255,
    )
    star = ImageChops.subtract(star, notch)

    fill = vertical_gradient((105, 205, 255), (24, 108, 220)).convert("RGBA")
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 110), (0, 0), star.filter(ImageFilter.GaussianBlur(14 * SCALE)))
    canvas.alpha_composite(ImageChops.offset(shadow, 0, 14 * SCALE))
    canvas.paste(fill, (0, 0), star)

    # Facets for depth.
    facets = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    fd = ImageDraw.Draw(facets)
    center = p(440, 580)
    for index in range(5):
        angle = math.radians(-90 + index * 72)
        tip = p(440 + 350 * math.cos(angle), 580 + 350 * math.sin(angle))
        valley = p(440 + 168 * math.cos(angle + math.radians(36)), 580 + 168 * math.sin(angle + math.radians(36)))
        fd.polygon([center, tip, valley], fill=(8, 40, 110, 70))
    canvas.paste(facets, (0, 0), ImageChops.multiply(facets.getchannel("A"), star))

    # Chips flying out of the impact point.
    rng = random.Random(11)
    chips = [(760, 400, 40, 30), (845, 345, 30, 80), (880, 440, 36, 120), (800, 520, 30, 10), (905, 520, 24, 60), (720, 330, 24, 150)]
    chip_fill = vertical_gradient((130, 215, 255), (40, 125, 230)).convert("RGBA")
    for cx, cy, size, rotation in chips:
        points = []
        for index in range(5):
            angle = math.radians(rotation + index * 72 + rng.uniform(-18, 18))
            radius = size * rng.uniform(0.7, 1.15)
            points.append(p(cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
        chip_mask = rounded_polygon_mask(points, 5 * SCALE)
        canvas.paste((10, 40, 100, 120), (0, 0), chip_mask.filter(ImageFilter.MaxFilter(5 * SCALE | 1)))
        canvas.paste(chip_fill, (0, 0), chip_mask)

    canvas.alpha_composite(pickaxe_layer(pivot=(668, 452), tilt=-36))

    canvas = canvas.resize((SIZE, SIZE), Image.LANCZOS)
    canvas.save(path)
    print(f"wrote {path}")


if __name__ == "__main__":
    main(sys.argv[1])
