#!/usr/bin/env python3
"""Rasterises one egui frame (JSON written by the render tests: tessellated meshes and the font atlas) as a PNG, for the
screenshots of the app's own controls (panels, drop-downs) that the chart layout tool cannot draw.

usage: egui_mesh_png.py frame.json out.png [x0 y0 x1 y1]   (the optional crop is in points)
Needs numpy and Pillow.
"""
import json
import sys

import numpy as np
from PIL import Image


def main():
    d = json.load(open(sys.argv[1]))
    ppp = d["ppp"]
    w, h = d["size"]
    W, H = int(round(w * ppp)), int(round(h * ppp))
    bg = d["background"]
    dst = np.zeros((H, W, 4), dtype=np.float32)
    dst[..., 0], dst[..., 1], dst[..., 2], dst[..., 3] = bg[0] / 255.0, bg[1] / 255.0, bg[2] / 255.0, 1.0
    aw, ah = d["atlas"]["size"]
    atlas = np.array(d["atlas"]["rgba"], dtype=np.float32).reshape(ah, aw, 4) / 255.0
    for m in d["meshes"]:
        if m["texture"] != 0:
            continue
        cx0, cy0, cx1, cy1 = [v * ppp for v in m["clip"]]
        vx = np.array(m["pos"], dtype=np.float32).reshape(-1, 2) * ppp
        uv = np.array(m["uv"], dtype=np.float32).reshape(-1, 2) * np.array([aw, ah], dtype=np.float32)
        col = np.array(m["col"], dtype=np.float32).reshape(-1, 4) / 255.0
        idx = np.array(m["idx"], dtype=np.int64).reshape(-1, 3)
        for a, b, c in idx:
            p0, p1, p2 = vx[a], vx[b], vx[c]
            x0 = max(int(np.floor(min(p0[0], p1[0], p2[0]))), int(np.floor(cx0)), 0)
            x1 = min(int(np.ceil(max(p0[0], p1[0], p2[0]))), int(np.ceil(cx1)), W)
            y0 = max(int(np.floor(min(p0[1], p1[1], p2[1]))), int(np.floor(cy0)), 0)
            y1 = min(int(np.ceil(max(p0[1], p1[1], p2[1]))), int(np.ceil(cy1)), H)
            if x1 <= x0 or y1 <= y0:
                continue
            den = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1])
            if abs(den) < 1e-9:
                continue
            xs, ys = np.meshgrid(np.arange(x0, x1, dtype=np.float32) + 0.5, np.arange(y0, y1, dtype=np.float32) + 0.5)
            l0 = ((p1[1] - p2[1]) * (xs - p2[0]) + (p2[0] - p1[0]) * (ys - p2[1])) / den
            l1 = ((p2[1] - p0[1]) * (xs - p2[0]) + (p0[0] - p2[0]) * (ys - p2[1])) / den
            l2 = 1.0 - l0 - l1
            inside = (l0 >= -1e-4) & (l1 >= -1e-4) & (l2 >= -1e-4)
            if not inside.any():
                continue
            u = l0 * uv[a][0] + l1 * uv[b][0] + l2 * uv[c][0]
            v = l0 * uv[a][1] + l1 * uv[b][1] + l2 * uv[c][1]
            ui = np.clip(np.floor(u - 0.5 + 0.5).astype(np.int64), 0, aw - 1)
            vi = np.clip(np.floor(v - 0.5 + 0.5).astype(np.int64), 0, ah - 1)
            tex = atlas[vi, ui]
            vc = l0[..., None] * col[a] + l1[..., None] * col[b] + l2[..., None] * col[c]
            src = vc * tex
            region = dst[y0:y1, x0:x1]
            a_src = src[..., 3:4]
            blended = src + region * (1.0 - a_src)
            region[inside] = blended[inside]
    if len(sys.argv) >= 7:
        x0, y0, x1, y1 = [int(round(float(v) * ppp)) for v in sys.argv[3:7]]
        dst = dst[max(y0, 0):min(y1, H), max(x0, 0):min(x1, W)]
    out = (np.clip(dst[..., :3], 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)
    Image.fromarray(out, "RGB").save(sys.argv[2])


main()
