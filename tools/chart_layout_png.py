#!/usr/bin/env python3
"""Draws a chart layout (JSON from `ChartLayout::to_json`) as a PNG, for the render tests' screenshots.

usage: chart_layout_png.py layout.json out.png [scale]
Needs Pillow. Fonts: assets/fonts/SourceSans3-*.ttf (next to this script's repository root).
"""
import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont

SS = 2  # supersampling on top of the output scale


def main():
    src, out = sys.argv[1], sys.argv[2]
    scale = float(sys.argv[3]) if len(sys.argv) > 3 else 2.0
    d = json.load(open(src))
    k = scale * SS
    x0, y0, x1, y1 = d["rect"]
    w, h = int((x1 - x0) * k), int(d["height"] * k)
    img = Image.new("RGB", (w, h), tuple(d["surface"][:3]))
    dr = ImageDraw.Draw(img, "RGBA")
    P = lambda p: ((p[0] - x0) * k, (p[1] - y0) * k)
    here = os.path.dirname(os.path.abspath(__file__))
    fonts = os.path.join(here, "..", "assets", "fonts")

    def font(pt, semibold):
        f = "SourceSans3-Semibold.ttf" if semibold else "SourceSans3-Regular.ttf"
        return ImageFont.truetype(os.path.join(fonts, f), max(1, int(round(pt * k))))

    for p in d["prims"]:
        kind = p["k"]
        if kind == "rect":
            l, t, r, b = p["r"]
            a, c = P((l, t)), P((r, b))
            rad = max(p["round"]) * k
            nw, ne, sw, se = [v > 0 for v in p["round"]]
            if rad > 0:
                dr.rounded_rectangle([a[0], a[1], c[0] - 1, c[1] - 1], radius=rad, fill=tuple(p["fill"]), corners=(nw, ne, se, sw))
            else:
                dr.rectangle([a[0], a[1], c[0] - 1, c[1] - 1], fill=tuple(p["fill"]))
        elif kind == "line":
            dr.line([P(p["a"]), P(p["b"])], fill=tuple(p["col"]), width=max(1, int(round(p["w"] * k))))
        elif kind == "tris":
            for t in p["tris"]:
                dr.polygon([P(q) for q in t], fill=tuple(p["fill"]))
        elif kind == "circle":
            c = P(p["c"])
            r = p["r"] * k
            if p.get("ring"):
                rr = r + p["ring"]["w"] * k
                dr.ellipse([c[0] - rr, c[1] - rr, c[0] + rr, c[1] + rr], fill=tuple(p["ring"]["col"]))
            dr.ellipse([c[0] - r, c[1] - r, c[0] + r, c[1] + r], fill=tuple(p["fill"]))
        elif kind == "outline":
            c = P(p["c"])
            r = p["r"] * k
            dr.ellipse([c[0] - r, c[1] - r, c[0] + r, c[1] + r], outline=tuple(p["col"]), width=max(1, int(round(p["w"] * k))))
    for t in d["texts"]:
        a = P((t["r"][0], t["r"][1]))
        ang = t.get("angle", 0.0)
        if abs(ang) > 1.2:
            # a turned text: drawn level on its own layer, turned about its centre, put in the middle of its rectangle
            f = font(t["pt"], t["semibold"])
            l, tp, r, b = t["r"]
            wl = int(dr.textlength(t["text"], font=f)) + 8
            hl = int(t["pt"] * k * 1.6) + 8
            layer = Image.new("RGBA", (wl, hl), (0, 0, 0, 0))
            ImageDraw.Draw(layer).text((4, 4), t["text"], font=f, fill=tuple(t["col"]) + ((255,) if len(t["col"]) == 3 else ()), anchor="la")
            import math
            layer = layer.rotate(-math.degrees(ang), expand=True, resample=Image.BICUBIC)
            cx, cy = P(((l + r) / 2, (tp + b) / 2))
            img.paste(layer, (int(cx - layer.width / 2), int(cy - layer.height / 2)), layer)
            continue
        dr.text(a, t["text"], font=font(t["pt"], t["semibold"]), fill=tuple(t["col"]), anchor="la")
    img = img.resize((int(w / SS), int(h / SS)), Image.LANCZOS)
    img.save(out)


if __name__ == "__main__":
    main()
