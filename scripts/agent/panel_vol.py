#!/usr/bin/env python3
"""Sozlamalar → Övoz suratida slayder tugmaçasi kutilgan foizdami (zgeom.py svol_track).

Foydalanish: panel_vol.py surat.png FOIZ   — chiqish kodi 0: tugmaça markazi ±2 px içida.
Tugmaça — oq doira (radius u*2/3); markazi ustidagi qatorda oq vatar markazi topiladi.
"""
import os
import sys
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zgeom

im = Image.open(sys.argv[1]).convert("RGB")
want = int(sys.argv[2])
g = zgeom.metrics(*im.size)
tx, ty, tw, th = g["svol_track"]
kr = g["svol_kr"]
ex = tx + tw * want // 100
row = ty + th // 2 - kr // 2
white = lambda x: sum(im.getpixel((x, row))) > 700
if not white(ex):
    print("FAIL: tugmaça %d%% joyida (x=%d, y=%d) topilmadi" % (want, ex, row))
    sys.exit(1)
a = ex
while white(a - 1):
    a -= 1
b = ex
while white(b + 1):
    b += 1
c = (a + b) / 2
pct = (c - tx) * 100 / tw
print("tugmaça markazi x=%.1f (kutilgan %d) ≈ %.1f%%" % (c, ex, pct))
sys.exit(0 if abs(c - ex) <= 2 else 1)
