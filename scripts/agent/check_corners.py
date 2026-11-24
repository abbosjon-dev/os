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

Foydalanish: check_corners.py surat.png [x0 y0 x1 y1] [-v]   (sahifa sohasi;
  berilmasa — surat ölçamiga qarab zgeom.py dan: 1024x768, 1280x800, 1920x1080, ...)
  -v — topilgan bloklar va tekshirilgan burchaklar ro'yxati ("BURCHAK x y sx sy").
check_corners.py --gop KERNEL CHIQISH_PAPKA [-v]
  UEFI GOP zaxira ekrani yo'li (virtio-gpu yo'q, QEMU -device ramfb): zamin://matn (boshi va
  yarim aylantirilgan), zamin://bezak, zamin://inline va zamin://overflow — 1280x800 (ramfb fw_cfg orqali qayta
  sozlangan: qator qadami 1344 piksel, RGBX) va 1024x768 (EDK2 ning o'z ramfb rejimi, BGRX,
  -cpu neoverse-n1) ekranlarida suratga olinadi va tekshiriladi. UEFI firmware bo'lmasa — SKIP (0).
check_corners.py --quick surat.png [-v]
  Zamin paneli (tezkor sozlamalar, gui.rs quick_geo): panel tö'g'ri to'rtburçagi zgeom.py dan
  ("quick"), radius u*3/4. Har burçak yoyi va qirralar böylab AA piksellar içkaridagi panel
  foni va taşqaridagi (soya bilan) fon rangidan to'qroq bölmasligi tekşiriladi.
Chiqish kodi 0 — toza; 1 — nuqson topildi (yoki birorta burchak tekshirilmadi).
"""
import os
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


def check(img, area, verbose=False, strict=False):
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
            if strict:
                bad += strict_corner(page, lum, bgl, (x0, y0, x1, y1), (cx, cy, sx, sy), min(ref), (px0, py0))
            # katta / alohida radiusli burçak (50% doira, 60 px dan katta): butun yoy böylab
            nb, nw = arc_corner(page, lum, (x0, y0, x1, y1), (cx, cy, sx, sy), min(min(ref), min(out)), (px0, py0), verbose)
            bad += nb
            ARC_N[0] += nw
    sn = f", qat'iy: {STRICT_N[0]}" if strict else ""
    print(f"fon #{bg:06x}, bloklar: {len(boxes)}, burchaklar: {checked}{sn}, yoy piksellari: {ARC_N[0]}, nuqsonli: {bad}")
    return bad, checked


STRICT_N = [0]  # qat'iy tekşirilgan burçaklar soni
ARC_N = [0]     # yoy böylab tekşirilgan piksellar soni
ARC_MAX = 120   # yoy zonasi eng katta ölçami (px)


def arc_corner(page, lum, box, corner, ref_lum, off, verbose):
    """Yumaloq burçak yoyi böylab "tişça" tekşiruvi (har qanday radius: alohida burçaklar,
    50% doira, 60 px dan katta). Burçak radiusi bbox qirralari böylab taşqi rangdagi
    piksellar soni bilan baholanadi (rx — yuqori/pastki qator, ry — çap/öng ustun). Keyin
    zona (rx × ry) içidagi har qator va ustun bbox çetidan içkariga yuriladi: taşqi fon
    (soya) piksellari, AA çeti va şakl içidagi dastlabki 3 piksel tekşiriladi — hech biri
    blokning içki rangi (qirra örtasidagi çegara/fon), taşqi fon va burçak yaqinidagi
    ranglarning eng to'qidan TOL dan ko'proq to'q bölmasligi şart. Matn (şakl içida,
    çekiniş ortida) yurişga tuşmaydi. Qaytaradi: (nuqsonlar, tekşirilgan piksellar)."""
    x0, y0, x1, y1 = box
    cx, cy, sx, sy = corner
    H, W = lum.shape
    if x1 - x0 < 30 or y1 - y0 < 18:
        return 0, 0
    # taşqi namuna: burçakdan diagonal taşqarida (sahifa çetida bölsa — tekşirilmaydi)
    ox, oy = cx - sx, cy - sy
    if not (0 <= ox < W and 0 <= oy < H):
        return 0, 0
    outc = page[oy, ox].astype(int)

    def outside(x, y, ref):
        return int(np.abs(page[y, x].astype(int) - ref).max()) <= 10

    def run(x, y, dx, dy, lim):
        k = 0
        ref = page[y - dy, x - dx].astype(int) if 0 <= y - dy < H and 0 <= x - dx < W else outc
        while k < lim and 0 <= x + dx * k < W and 0 <= y + dy * k < H and outside(x + dx * k, y + dy * k, ref):
            k += 1
        return k
    half_w, half_h = (x1 - x0 + 1) // 2, (y1 - y0 + 1) // 2
    rx = run(cx, cy, sx, 0, min(ARC_MAX, half_w))
    ry = run(cx, cy, 0, sy, min(ARC_MAX, half_h))
    if rx < 6 or ry < 6:
        return 0, 0   # kiçik radius — 6x6 va qat'iy tekşiruvlar qamraydi
    # içki rang: qirralar örtasida, bbox çetidan 1..4 px içkarida (çegara yoki fon)
    mids = [lum[y0 + k, (x0 + x1) // 2] for k in range(1, 5)] + [lum[y1 - k, (x0 + x1) // 2] for k in range(1, 5)] + \
           [lum[(y0 + y1) // 2, x0 + k] for k in range(1, 5)] + [lum[(y0 + y1) // 2, x1 - k] for k in range(1, 5)]
    outs = [int(outc.sum())]
    bad = 0
    n = 0
    worst = None
    zx, zy = min(rx + 2, half_w), min(ry + 2, half_h)
    lines = [(cx, cy + sy * j, sx, 0, zx) for j in range(zy)] + [(cx + sx * j, cy, 0, sy, zy) for j in range(zx)]
    for (lx, ly, dx, dy, lim) in lines:
        if not (0 <= lx < W and 0 <= ly < H):
            continue
        ref = page[ly - dy, lx - dx].astype(int) if 0 <= ly - dy < H and 0 <= lx - dx < W else outc
        outs.append(int(ref.sum()))
        inside = 0
        seen = []     # (x, y, lum) — çet (AA) va şakl içidagi dastlabki piksellar
        first = None  # şakl içiga kirgan birinçi qadam
        for k in range(lim):
            x, y = lx + dx * k, ly + dy * k
            if not (0 <= x < W and 0 <= y < H):
                break
            v = int(lum[y, x])
            if outside(x, y, ref):
                outs.append(v)
                inside = 0
                continue
            first = k if first is None else first
            seen.append((x, y, v))
            inside += 1
            if inside >= 3:
                break
        if not seen:
            continue
        # şaklning shu joydagi içki rangi (gradient bölsa — mahalliy): 4..7 px içkarida
        fill = []
        for k in range(first + 4, first + 8):
            x, y = lx + dx * k, ly + dy * k
            if 0 <= x < W and 0 <= y < H:
                fill.append(int(lum[y, x]))
        allowed = min([ref_lum, min(mids), min(outs)] + fill) - TOL
        for (x, y, v) in seen:
            n += 1
            if v < allowed and (worst is None or v - allowed < worst[0]):
                worst = (v - allowed, x, y, v, allowed)
    if worst is not None:
        bad = 1
        print(f"NUQSON (yoy): blok ({x0+off[0]},{y0+off[1]})-({x1+off[0]},{y1+off[1]}) burçak ({cx+off[0]},{cy+off[1]}) "
              f"r≈{rx}x{ry}: piksel ({worst[1]+off[0]},{worst[2]+off[1]}) lum {worst[3]} < ruxsat {worst[4]}")
    if verbose:
        # yoyning diagonal nuqtasi (o'z-o'zini sinaş uçun): burçakdan diagonal böylab birinçi
        # taşqi bölmagan piksel
        for k in range(min(rx, ry) + 2):
            x, y = cx + sx * k * rx // max(rx, ry), cy + sy * k * ry // max(rx, ry)
            if 0 <= x < W and 0 <= y < H and not outside(x, y, outc):
                print(f"YOY {x+off[0]} {y+off[1]} {rx} {ry}")
                break
    return bad, n
DARK_TOL = 8   # burçakning taşqi pikseli va taşqi fon orasidagi ruxsat (kanal böyiça)


def strict_corner(page, lum, bgl, box, corner, block_lum, off):
    """Qat'iy (qorong'i holat) tekşiruvi bitta burçak uçun:
      * burçakdagi 3x3 piksel sahifa (taşqi) foni va blok rangi orasida — ikkalasidan ham
        to'qroq piksel yöq (qora/0 bilan aralaştiriş nuqsoni);
      * yumaloq burçakda eng çetki piksel taşqi fondan DARK_TOL dan köp farq qilmaydi
        (yarim aralaşgan "tişça" yöq); töğri burçakda u qirra rangiga teng.
    Bir neçta niqob bloklarni topadi: çegarali qutining içki foni ham alohida blok bölib
    çiqadi, şuning uçun burçak avval haqiqiy taşqi qirragaça (<= 5 px) aniqlaştiriladi.
    Taşqi fon — burçakdan diagonal taşqaridagi piksel (soya bölsa, soya bilan)."""
    x0, y0, x1, y1 = box
    cx, cy, sx, sy = corner
    H, W = lum.shape
    d = min(12, (x1 - x0) // 2, (y1 - y0) // 2)

    def px(x, y):
        return page[y, x].astype(int) if 0 <= x < W and 0 <= y < H else None

    def near(a, b, tol=DARK_TOL):
        return a is not None and b is not None and int(np.abs(a - b).max()) <= tol

    # haqiqiy qirra: qirra örtasidagi qator/ustun böylab taşqariga, taşqi namunaga teng bölguncha
    def walk(fx, fy, ax, ay):
        # (fx, fy) — bbox qirrasi pikseli, (ax, ay) — taşqariga qadam
        out = px(fx + ax * 6, fy + ay * 6)
        for k in range(0, 6):
            if near(px(fx + ax * (k + 1), fy + ay * (k + 1)), out) and not near(px(fx + ax * k, fy + ay * k), out):
                return k
        return None
    kx = walk(cx, cy + sy * d, -sx, 0)
    ky = walk(cx + sx * d, cy, 0, -sy)
    if kx is None or ky is None:
        return 0
    cx, cy = cx - sx * kx, cy - sy * ky
    # quti emas (masalan, och matn harflari): ikkala qirra burçakdan d..d+10 oraliğida töğri va bir
    # rangli bölişi şart (gradient sekin özgaradi — 24 birlik ruxsat)
    for (ax, ay) in ((sx, 0), (0, sy)):
        e0 = px(cx + ax * d, cy + ay * d)
        if any(not near(px(cx + ax * (d + k), cy + ay * (d + k)), e0, 24) for k in range(1, 11)):
            return 0
    ox, oy = cx - sx, cy - sy
    outp = px(ox, oy)
    if outp is None:
        return 0
    STRICT_N[0] += 1
    # blok rangi: içki fon va qirra (çegara bölsa — çegara rangi) — qaysi biri to'qroq
    edge_l = [int(e.sum()) for e in (px(cx + sx * d, cy), px(cx, cy + sy * d)) if e is not None]
    lo = min([int(lum[oy, ox]), bgl, int(block_lum)] + edge_l) - TOL
    reg = lum[min(cy, cy + sy * 2):max(cy, cy + sy * 2) + 1, min(cx, cx + sx * 2):max(cx, cx + sx * 2) + 1]
    n = 0
    tag = f"blok ({x0+off[0]},{y0+off[1]})-({x1+off[0]},{y1+off[1]}) burçak ({cx+off[0]},{cy+off[1]})"
    if reg.min() < lo:
        n += 1
        print(f"NUQSON (qat'iy): {tag} 3x3: lum {reg.min()} < {lo} (fon va blokdan to'q)")
    # töğri burçak: çetki piksel qirra rangiga teng (qirra boyidagi eng taşqi qator/ustun)
    cp = px(cx, cy)
    square = near(cp, px(cx + sx * d, cy)) or near(cp, px(cx, cy + sy * d))
    # kiçik radius (<= 3 px): çetki piksel qonuniy ravişda qisman qamrovli — faqat 3x3 tekşiruvi
    small = near(px(cx + sx * 2, cy), px(cx + sx * d, cy), 24) or near(px(cx, cy + sy * 2), px(cx, cy + sy * d), 24)
    diff = int(np.abs(cp - outp).max())
    if not square and not small and diff > DARK_TOL:
        n += 1
        print(f"NUQSON (qat'iy): {tag}: çetki piksel {tuple(int(v) for v in cp)} taşqi fon "
              f"{tuple(int(v) for v in outp)} dan {diff} farq qiladi")
    return n


# GOP rejimi: (nom, shot.py --gop spetsifikatsiyasi, CPU)
GOP_CFGS = [("gop-1280x800", "1280x800@1344:rgbx", "max"), ("gop-1024x768", "1024x768", "neoverse-n1")]
GOP_FW = ("/usr/share/qemu-efi-aarch64/QEMU_EFI.fd", "/usr/share/AAVMF/AAVMF_CODE.fd")


def gop_main(args, verbose):
    import re
    import subprocess
    kernel, out = args[0], args[1]
    if not any(os.path.exists(f) for f in GOP_FW):
        print("SKIP burchaklar (GOP): UEFI firmware yöq")
        return 0
    os.makedirs(out, exist_ok=True)
    here = os.path.dirname(os.path.abspath(__file__))
    m = "m,@park;w,1"
    clicks = (f"@browser;w,2;@max;w,1;u,zamin://matn;{m};snap,matn;m,@center;wd,8;{m};snap,matn2;"
              f"u,zamin://bezak;{m};snap,bezak;u,zamin://inline;{m};snap,inline;u,zamin://overflow;{m}")
    bad = total = 0
    for name, spec, cpu in GOP_CFGS:
        png = os.path.join(out, name + ".png")
        for f in os.listdir(out):
            if f.startswith(name):
                os.remove(os.path.join(out, f))
        env = dict(os.environ, CLICKS=clicks)
        with open(os.path.join(out, f"shot-{name}.log"), "wb") as lf:
            subprocess.run([sys.executable, os.path.join(here, "shot.py"), kernel, png, "fresh", f"--gop={spec}", f"--cpu={cpu}"],
                           env=env, stdout=lf, stderr=subprocess.STDOUT, timeout=600)
        log = open(png + ".log", "rb").read().decode("utf8", "replace") if os.path.exists(png + ".log") else ""
        w, h = map(int, spec.split("@")[0].split(":")[0].split("x"))
        if f"[gpu ] UEFI GOP: {w}x{h}" not in log or "virtio-gpu" in log:
            print(f"FAIL burchaklar (GOP): {name} — ekran GOP framebufferida {w}x{h} ölçamda işga tuşmadi")
            return 1
        if ":" in spec and not re.search(rf"ramfb: qayta sozlandi {w}x{h}, format RGBX, qator \d+", log):
            print(f"FAIL burchaklar (GOP): {name} — ramfb qayta sozlanmadi (fw_cfg)")
            return 1
        for f in (png.replace(".png", "_matn.png"), png.replace(".png", "_matn2.png"), png.replace(".png", "_bezak.png"), png.replace(".png", "_inline.png"), png):
            if not os.path.exists(f):
                print(f"FAIL burchaklar (GOP): surat olinmadi ({f})")
                return 1
            img = np.asarray(Image.open(f).convert("RGB")).astype(np.int32)
            if (img.shape[1], img.shape[0]) != (w, h):
                print(f"FAIL burchaklar (GOP): {os.path.basename(f)} ölçami {img.shape[1]}x{img.shape[0]} (kutilgan {w}x{h})")
                return 1
            sys.path.insert(0, here)
            import zgeom
            print(f"{os.path.basename(f)}: ", end="")
            b, c = check(img, zgeom.metrics(w, h)["area"], verbose)
            bad += b
            total += c
            if c == 0:
                print(f"FAIL burchaklar (GOP): {os.path.basename(f)} — birorta burchak tekşirilmadi")
                return 1
    if bad:
        print(f"FAIL burchaklar (GOP): {bad} ta nuqsonli burchak")
        return 1
    print(f"OK burchaklar (GOP): ramfb 1280x800 (qadam 1344, RGBX) va 1024x768 (BGRX, neoverse-n1) — "
          f"matn/bezak/inline/overflow, {total} burchak, to'q piksel 0")
    return 0


def quick_main(path, verbose):
    """Zamin paneli qirralari va burçaklarida to'q (tişça) piksel yöqligini tekşiradi."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import zgeom
    im = np.asarray(Image.open(path).convert("RGB")).astype(int)
    H, W = im.shape[:2]
    lum = im.sum(axis=2)
    g = zgeom.metrics(W, H)
    x, y, w, h = g["quick"]
    r = g["u"] * 3 // 4
    x1, y1 = x + w - 1, y + h - 1
    if x1 >= W or y1 >= H:
        print("NUQSON: panel ekrandan taşqarida", (x, y, w, h))
        return 1
    panel = int(np.median(lum[y + r:y + r * 2, x + r:x + r * 2]))
    if panel > 200:
        print("NUQSON: panel topilmadi (foni yorug': %d)" % panel)
        return 1
    bad = []

    def lv(px, py):
        return lum[min(max(py, 0), H - 1), min(max(px, 0), W - 1)]

    def test(px, py, ox, oy, ix, iy):
        # (px,py) — qirra pikseli; (ox,oy) — taşqaridagi, (ix,iy) — içkaridagi namuna
        lo = min(lv(ox, oy), lv(ix, iy), panel) - 3 * TOL
        if lum[py, px] < lo:
            bad.append((px, py, int(lum[py, px]), int(lo)))

    n = 0
    # burçaklar: yoy markazi (ccx, ccy), yönalişi (sx, sy)
    for (ccx, ccy, sx, sy) in ((x + r, y + r, -1, -1), (x1 - r, y + r, 1, -1), (x + r, y1 - r, -1, 1), (x1 - r, y1 - r, 1, 1)):
        for dy in range(0, r + 3):
            for dx in range(0, r + 3):
                px, py = ccx + sx * dx, ccy + sy * dy
                d = (dx * dx + dy * dy) ** 0.5
                if not (r - 2.5 <= d <= r + 1.0) or d == 0:
                    continue
                ux, uy = dx / d, dy / d
                ox, oy = int(round(ccx + sx * ux * (r + 3))), int(round(ccy + sy * uy * (r + 3)))
                ix, iy = int(round(ccx + sx * ux * (r - 4))), int(round(ccy + sy * uy * (r - 4)))
                test(px, py, ox, oy, ix, iy)
                n += 1
        if verbose:
            print("BURCHAK", ccx + sx * r, ccy + sy * r, -sx, -sy)
    # tög'ri qirralar (burçaklar orasida)
    for px in range(x + r, x1 - r + 1, 2):
        test(px, y, px, y - 3, px, y + 4)
        test(px, y1, px, y1 + 3, px, y1 - 4)
        n += 2
    for py in range(y + r, y1 - r + 1, 2):
        test(x, py, x - 3, py, x + 4, py)
        test(x1, py, x1 + 3, py, x1 - 4, py)
        n += 2
    for b in bad[:10]:
        print("NUQSON: to'q piksel (%d,%d) yorug'lik %d < %d" % b)
    print("panel %s, fon %d, tekşirilgan piksellar: %d, nuqsonli: %d" % ((x, y, w, h), panel, n, len(bad)))
    return 1 if bad else 0


def main():
    if "--quick" in sys.argv[1:]:
        args = [a for a in sys.argv[1:] if a not in ("-v", "--quick")]
        sys.exit(quick_main(args[0], "-v" in sys.argv[1:]))
    if "--gop" in sys.argv[1:]:
        args = [a for a in sys.argv[1:] if a not in ("-v", "--gop")]
        sys.exit(gop_main(args, "-v" in sys.argv[1:]))
    args = [a for a in sys.argv[1:] if a not in ("-v", "--strict")]
    verbose = "-v" in sys.argv[1:]
    strict = "--strict" in sys.argv[1:]
    img = np.asarray(Image.open(args[0]).convert("RGB")).astype(np.int32)
    if len(args) >= 5:
        area = tuple(map(int, args[1:5]))
    else:
        # kattalashtirilgan brauzer oynasining sahifa sohasi — surat ölçamidan (zgeom.py)
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        import zgeom
        area = zgeom.metrics(img.shape[1], img.shape[0])["area"]
    bad, checked = check(img, area, verbose, strict)
    sys.exit(1 if bad or checked == 0 else 0)


if __name__ == "__main__":
    main()
