#!/usr/bin/env python3
"""Yumaloq burchak tekshiruvi (brauzer sahifasi skrinshoti uchun).

Brauzer sahifa sohasidagi bloklarni topadi va har blok burchagi atrofidagi 6x6
hududda fondan (blok tashqarisi, soya bo'lsa soya bilan) va blok rangidan (chegara/
fon, gradient bo'lsa burchak yaqinidagi eng to'q rang) to'qroq piksel yo'qligini
tekshiradi ("qora tishcha" nuqsoni).

Bloklar qanday topiladi (rang ro'yxati sahifa bo'yicha avtomatik aniqlanadi):
  * sahifa foni — sahifa sohasining chap chetidagi (body padding) eng ko'p rang;
  * fondan ancha yorug' piksellar (to'q sahifadagi och qutilar);
  * KNOWN ranglar va sahifada ko'p uchraydigan (>= FREQ piksel) har bir aniq rang —
    bir rangli qutilar, opacity bilan aralashgan qutilar, chegaralar;
  * to'yingan (max-min >= CHROMA) piksellar — gradientli va rangli qutilar.

Foydalanish: check_corners.py surat.png [x0 y0 x1 y1] [-v]   (sahifa sohasi)
  -v — topilgan bloklar va tekshirilgan burchaklar ro'yxati ("BURCHAK x y sx sy").
Chiqish kodi 0 — toza; 1 — nuqson topildi (yoki birorta burchak tekshirilmadi).
"""
import sys
import numpy as np
from PIL import Image

KNOWN = [0xeef2f8, 0xffffff, 0xdfe9fb, 0xfde7c8, 0x1e3a8a, 0x3a7bd5, 0xb06a00]
TOL = 6        # yorug'lik (R+G+B) bo'yicha ruxsat etilgan farq
FREQ = 2000    # "ko'p uchraydigan rang" chegarasi (piksel)
CHROMA = 60    # gradient/rangli quti piksellari uchun to'yinganlik chegarasi


def comps(mask):
    """4-bog'langan komponentlar bbox lari (x0, y0, x1, y1, soni) — qator bo'laklari
    (run) va union-find bilan (katta maydonlarda ham tez)."""
    h, w = mask.shape
    m = np.zeros((h, w + 2), dtype=np.int8)
    m[:, 1:-1] = mask
    d = np.diff(m, axis=1)
    runs = []          # (y, xs, xe) — xe istisno
    row_runs = []
    for y in range(h):
        st = np.nonzero(d[y] == 1)[0]
        en = np.nonzero(d[y] == -1)[0]
        base = len(runs)
        runs.extend((y, int(a), int(b)) for a, b in zip(st, en))
        row_runs.append((base, len(runs)))
    par = list(range(len(runs)))

    def find(a):
        while par[a] != a:
            par[a] = par[par[a]]
            a = par[a]
        return a

    for y in range(1, h):
        pb, pe = row_runs[y - 1]
        cb, ce = row_runs[y]
        i, j = pb, cb
        while i < pe and j < ce:
            _, a0, a1 = runs[i]
            _, b0, b1 = runs[j]
            if a0 < b1 and b0 < a1:
                ra, rb = find(i), find(j)
                if ra != rb:
                    par[ra] = rb
            if a1 < b1:
                i += 1
            else:
                j += 1
    box = {}
    for k, (y, a, b) in enumerate(runs):
        r = find(k)
        if r in box:
            x0, y0, x1, y1, c = box[r]
            box[r] = (min(x0, a), min(y0, y), max(x1, b - 1), max(y1, y), c + b - a)
        else:
            box[r] = (a, y, b - 1, y, b - a)
    return list(box.values())


def find_boxes(page):
    lum = page.sum(axis=2)
    rgb = (page[:, :, 0] << 16) | (page[:, :, 1] << 8) | page[:, :, 2]
    # fon — chap chetdagi (sahifa ichki chekinishi) eng ko'p rang
    edge = rgb[:, 2:12]
    v, c = np.unique(edge, return_counts=True)
    bg = int(v[c.argmax()])
    bgl = (bg >> 16 & 255) + (bg >> 8 & 255) + (bg & 255)
    masks = [(lum > bgl + 40, 0.25)] + [(rgb == k, 0.25) for k in KNOWN if k != bg]
    vals, cnts = np.unique(rgb, return_counts=True)
    for k, n in zip(vals, cnts):
        if n >= FREQ and int(k) != bg and int(k) not in KNOWN:
            masks.append((rgb == k, 0.5))
    chroma = page.max(axis=2) - page.min(axis=2)
    masks.append((chroma >= CHROMA, 0.5))
    boxes = set()
    for m, fill in masks:
        if not m.any():
            continue
        for (x0, y0, x1, y1, cnt) in comps(m):
            w, h = x1 - x0 + 1, y1 - y0 + 1
            if w >= 30 and h >= 18 and cnt >= fill * w * h:
                boxes.add((x0, y0, x1, y1))
    return lum, bgl, bg, boxes


def check(img, area, verbose=False):
    px0, py0, px1, py1 = area
    page = img[py0:py1, px0:px1]
    lum, bgl, bg, boxes = find_boxes(page)
    H, W = lum.shape
    bad = 0
    checked = 0
    for (x0, y0, x1, y1) in sorted(boxes, key=lambda b: (b[1], b[0])):
        if verbose:
            print(f"BLOK {x0+px0} {y0+py0} {x1+px0} {y1+py0}")
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
            if verbose:
                print(f"BURCHAK {cx+px0} {cy+py0} {sx} {sy} ruxsat {allowed}")
            if reg.min() < allowed:
                bad += 1
                print(f"NUQSON: blok ({x0+px0},{y0+py0})-({x1+px0},{y1+py0}) burchak ({cx+px0},{cy+py0}): "
                      f"lum {reg.min()} < ruxsat {allowed}")
    print(f"fon #{bg:06x}, bloklar: {len(boxes)}, burchaklar: {checked}, nuqsonli: {bad}")
    return bad, checked


def main():
    args = [a for a in sys.argv[1:] if a != "-v"]
    verbose = "-v" in sys.argv[1:]
    img = np.asarray(Image.open(args[0]).convert("RGB")).astype(np.int32)
    if len(args) >= 5:
        area = tuple(map(int, args[1:5]))
    else:
        area = (8, 142, 1260, 686)  # kattalashtirilgan brauzer oynasi
    bad, checked = check(img, area, verbose)
    sys.exit(1 if bad or checked == 0 else 0)


if __name__ == "__main__":
    main()
