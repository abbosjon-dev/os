#!/usr/bin/env python3
"""Yumaloq burchak tekshiruvi (zamin://matn skrinshoti uchun).

Brauzer sahifa sohasidagi bloklarni topadi (fondan yorug' qutilar va sahifadagi
ma'lum rangli qutilar/chegaralar) va har blok burchagi atrofidagi 6x6 hududda
fondan (blok tashqarisi, soya bo'lsa soya bilan) va blok rangidan (chegara/fon,
gradient bo'lsa burchak yaqinidagi eng to'q rang) to'qroq piksel yo'qligini tekshiradi.

Foydalanish: check_corners.py surat.png [x0 y0 x1 y1]  (sahifa sohasi)
Chiqish kodi 0 — toza; 1 — nuqson topildi.
"""
import sys
import numpy as np
from PIL import Image

KNOWN = [0xeef2f8, 0xffffff, 0xdfe9fb, 0xfde7c8, 0x1e3a8a, 0x3a7bd5, 0xb06a00]
TOL = 6  # yorug'lik (R+G+B) bo'yicha ruxsat etilgan farq


def comps(mask):
    """4-bog'langan komponentlar bbox lari (x0, y0, x1, y1, soni) — numpy bilan."""
    h, w = mask.shape
    lab = np.zeros((h, w), dtype=np.int32)
    out = []
    nxt = 1
    ys, xs = np.nonzero(mask)
    for sy, sx in zip(ys, xs):
        if lab[sy, sx]:
            continue
        stack = [(sy, sx)]
        lab[sy, sx] = nxt
        x0 = x1 = sx
        y0 = y1 = sy
        cnt = 0
        while stack:
            y, x = stack.pop()
            cnt += 1
            x0, x1, y0, y1 = min(x0, x), max(x1, x), min(y0, y), max(y1, y)
            for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
                if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and not lab[ny, nx]:
                    lab[ny, nx] = nxt
                    stack.append((ny, nx))
        nxt += 1
        out.append((x0, y0, x1, y1, cnt))
    return out


def main():
    img = np.asarray(Image.open(sys.argv[1]).convert("RGB")).astype(np.int32)
    if len(sys.argv) >= 6:
        px0, py0, px1, py1 = map(int, sys.argv[2:6])
    else:
        px0, py0, px1, py1 = 8, 142, 1260, 686  # kattalashtirilgan brauzer oynasi
    page = img[py0:py1, px0:px1]
    lum = page.sum(axis=2)
    rgb = (page[:, :, 0] << 16) | (page[:, :, 1] << 8) | page[:, :, 2]
    vals, cnts = np.unique(rgb, return_counts=True)
    bg = int(vals[cnts.argmax()])
    bgl = (bg >> 16 & 255) + (bg >> 8 & 255) + (bg & 255)
    masks = [lum > bgl + 40] + [rgb == c for c in KNOWN]
    boxes = set()
    for m in masks:
        for (x0, y0, x1, y1, cnt) in comps(m):
            w, h = x1 - x0 + 1, y1 - y0 + 1
            if w >= 30 and h >= 18 and cnt >= 0.25 * w * h:
                boxes.add((x0, y0, x1, y1))
    H, W = lum.shape
    bad = 0
    checked = 0
    for (x0, y0, x1, y1) in sorted(boxes, key=lambda b: (b[1], b[0])):
        for cx, cy, sx, sy in ((x0, y0, 1, 1), (x1, y0, -1, 1), (x0, y1, 1, -1), (x1, y1, -1, -1)):
            # sahifa chetiga (aylantirilgan/kesilgan) tegib turgan burchak — tekshirilmaydi
            if cy <= 1 or cy >= H - 2 or cx <= 1 or cx >= W - 2:
                continue
            # burchak yaqinidagi blok ranglari: qirralar bo'ylab 7..11 px uzoqlikda, 0..6 px ichkarida
            ref = []
            for a in range(7, 12):
                for b in range(0, 7):
                    for (yy, xx) in ((cy + sy * a, cx + sx * b), (cy + sy * b, cx + sx * a)):
                        if 0 <= yy < H and 0 <= xx < W:
                            ref.append(lum[yy, xx])
            # tashqi fon (soya bo'lsa — soya bilan): qirralar tashqarisi
            out = [bgl]
            for a in range(4, 12):
                for d in (1, 2, 3):
                    for (yy, xx) in ((cy + sy * a, cx - sx * d), (cy - sy * d, cx + sx * a)):
                        if 0 <= yy < H and 0 <= xx < W:
                            out.append(lum[yy, xx])
            allowed = min(min(ref), min(out)) - TOL
            reg = lum[min(cy, cy + sy * 5):max(cy, cy + sy * 5) + 1, min(cx, cx + sx * 5):max(cx, cx + sx * 5) + 1]
            checked += 1
            if reg.min() < allowed:
                bad += 1
                yy, xx = np.unravel_index(reg.argmin(), reg.shape)
                print(f"NUQSON: blok ({x0+px0},{y0+py0})-({x1+px0},{y1+py0}) burchak ({cx+px0},{cy+py0}): "
                      f"lum {reg.min()} < ruxsat {allowed}")
    print(f"bloklar: {len(boxes)}, burchaklar: {checked}, nuqsonli: {bad}")
    sys.exit(1 if bad or checked == 0 else 0)


if __name__ == "__main__":
    main()
