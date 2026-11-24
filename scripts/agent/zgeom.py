"""Zamin GUI geometriyasi (src/gui.rs formulalarining Python nusxasi).

Skrinshot skriptlari (shot.py, check_corners.py) har qanday ekran o'lchamida
(1024x768, 1280x800, 1920x1080, ...) dok ikonasi, svetofor tugmalari, manzil
satri va kattalashtirilgan brauzer sahifa sohasini topishi uchun.
gui.rs dagi set_metrics/dock_geom/new_window/zoom_rect/web_geo o'zgarsa —
bu fayl ham yangilanadi.
"""

import os
import re

N_LAUNCHERS = 14
BROWSER = 1  # LAUNCHERS dagi "Brauzer" indeksi
SETTINGS = 12  # LAUNCHERS dagi "Sozlamalar" indeksi


_ADV = None


def _font():
    """src/gfx_font.rs dan UI shrifti belgi kengliklari (adv) — gfx::text_width uçun."""
    global _ADV
    if _ADV is None:
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "src", "gfx_font.rs"), encoding="utf-8").read()
        codes = [int(v) for v in re.search(r"static CODES: &\[u32\] = &\[([^\]]*)\]", src).group(1).split(",") if v.strip()]
        idx = [int(v) for v in re.search(r"static IDX: &\[u16\] = &\[([^\]]*)\]", src).group(1).split(",") if v.strip()]
        ui = src[src.index("static UI_GLYPHS"):]
        ui = ui[:ui.index("];")]
        advs = [int(v) for v in re.findall(r"adv:(\d+)", ui)]
        base = int(re.search(r"pub static UI: Face = Face\{ base:(\d+)", src).group(1))
        _ADV = ({c: advs[i] for c, i in zip(codes, idx)}, base, advs[idx[codes.index(ord("?"))]])
    return _ADV


def text_width(s, px):
    """gfx::text_width (UI shrifti) nusxasi."""
    m, base, q = _font()
    return sum(m.get(ord(ch), q) * px // base for ch in s)


def body(h):
    return max(14, min(32, h // 42))


def metrics(w, h):
    u = body(h)
    small = max(u * 8 // 10, 12)
    title_h = u * 2
    dock_h = u * 7 // 2
    menubar_h = u * 3 // 2
    # dock_geom
    pad = dock_h // 6
    icon = dock_h - pad * 2
    gap = icon // 5
    dw = N_LAUNCHERS * icon + (N_LAUNCHERS + 1) * gap
    dx = max(w - dw, 0) // 2
    dy = max(h - (dock_h + pad), 0)
    g = {"u": u, "W": w, "H": h}

    def icon_c(i):
        return (dx + gap + i * (icon + gap) + icon // 2, dy + pad + icon // 2)

    g["browser"] = icon_c(BROWSER)
    # new_window(n=0): brauzer oynasi (u*40 x u*30), svetofor "kattalashtirish" nuqtasi
    bw, bh = u * 40, u * 30
    x = min(w // 8, max(w - bw, 1))
    y = min(menubar_h + u * 2, max(max(h - (dock_h + bh + u), 0), menubar_h))
    r = max(u * 2 // 5, 5)
    g["max"] = (x + title_h // 2 + r + 2 * (r * 5 // 2), y + title_h // 2)
    # zoom_rect: kattalashtirilgan oyna
    m = u // 3
    zx, zy, zw, zh = m, menubar_h + m, w - 2 * m, max(dy - (menubar_h + 3 * m), 0)
    # web_geo
    top = zy + title_h + u * 3 // 2
    btn = u * 3 // 2
    by = top + u // 3
    b0 = zx + u // 2
    step = btn + u // 4
    ax = b0 + step * 3 + u // 4
    g["home"] = (b0 + step * 2 + btn // 2, by + btn // 2)
    g["addr"] = (ax + u * 4, by + btn // 2)
    ptop = by + btn + u // 3 + 1
    foot = small + u // 2
    px0, pw = zx + 1, zw - 2
    ph = zy + zh - (ptop + foot)
    g["page"] = (px0, ptop, pw, ph)
    # check_corners sohasi: sahifa ichi, öng tarafdagi aylantirish chizig'isiz
    g["area"] = (px0 + 1, ptop + 1, px0 + pw - 13, ptop + ph - 1)
    g["center"] = (px0 + pw // 2, ptop + ph // 2)
    # Sozlamalar (dok: 12-ikona; new_window(n=0): u*34 x u*24) → Tarmoq bölimi (gui.rs
    # settings_geo / net_mode_btn / net_field / net_apply_btn)
    g["settings"] = icon_c(SETTINGS)
    sw_, sh_ = u * 34, u * 24
    sx = min(w // 8, max(w - sw_, 1))
    sy = min(menubar_h + u * 2, max(max(h - (dock_h + sh_ + u), 0), menubar_h))
    sbw, ih = u * 9, u * 7 // 4
    cx, y0 = sx + sbw + u, sy + title_h + u * 2 // 3
    g["set_tab3"] = (sx + sbw // 2, sy + title_h + u // 2 + 3 * ih + ih // 2)
    rw = sx + sw_ - (cx + u)
    bw = min(rw, u * 18) // 2
    my, mh = y0 + u * 2, u * 3 // 2
    g["net_dhcp"] = (cx + bw // 2, my + mh // 2)
    g["net_manual"] = (cx + bw + bw // 2, my + mh // 2)
    fw = (rw - u) // 2
    rowh = small * 4 // 5 + u // 4 + u * 3 // 2 + u // 2
    for k in range(4):
        fy = my + mh + u // 2 + (k // 2) * rowh + small * 4 // 5 + u // 4
        g["net_f%d" % k] = (cx + (k % 2) * (fw + u) + fw // 2, fy + u * 3 // 4)
    fy2 = my + mh + u // 2 + rowh + small * 4 // 5 + u // 4
    g["net_apply"] = (cx + u, fy2 + u * 3 // 2 + u * 2 // 3 + u * 3 // 4)
    # Sozlamalar → Övoz slayderi (gui.rs vol_track): tugmaça markazi foizga qarab
    vx = sx + sbw + u
    vrw = sx + sw_ - (vx + u)
    vty = sy + title_h + u * 2 // 3 + u * 2 + small + u
    vtx, vtw = vx + u * 2 // 3, vrw - u * 5
    g["svol_track"] = (vtx, vty, vtw, max(u // 3, 4))
    g["svol_kr"] = u * 2 // 3
    g["set_tab2"] = (sx + sbw // 2, sy + title_h + u // 2 + 2 * ih + ih // 2)
    # Menyu panelidagi holat belgilari (gui.rs status_rx / status_x): 0 batareya, 1 karnay, 2 Wi-Fi
    sm = small
    wd = max(text_width(d, sm) for d in ("Duş", "Seş", "Çor", "Pay", "Jum", "Şan", "Yak"))
    mn = max(text_width(m_, sm) for m_ in ("yanvar", "fevral", "mart", "aprel", "may", "iyun", "iyul",
                                         "avgust", "sentabr", "oktabr", "noyabr", "dekabr"))
    srx = max(w - (wd + mn + text_width(", 28- 00:00", sm) + u), 0)
    for k, nm in enumerate(("bat", "spk", "wifi", "bell")):
        g[nm] = (max(srx - (u + (k + 1) * (sm * 3 // 2)), 0), menubar_h // 2)
    # Zamin paneli (gui.rs quick_geo, tasdiqsiz holat): övoz slayderi va panel sohasi
    pw = min(u * 19, w - u)
    qx, qy = w - (pw + u // 2), menubar_h + 3
    ry = qy + u // 2 + sm + u * 2 // 3 + sm * 9 // 10 + u // 3
    bh = u * 3 // 2
    kr = u * 2 // 3
    tx = qx + u + bh + u * 2 // 3 + kr
    tw = max(qx + pw - u - (text_width("100%", sm) + u // 2 + kr + tx), 0)
    g["qmute"] = (qx + u + bh // 2, ry + bh // 2)
    for p_ in range(0, 101, 10):
        g["qvol%d" % p_] = (tx + tw * p_ // 100, ry + bh // 2)
    net_y = ry + bh + u * 2 // 3 + u // 2
    ly = net_y + sm + u // 2 + (sm * 9 // 10 + u // 3) * 3 + u // 4
    g["qnet"] = (qx + pw // 2, ly + bh // 2)
    theme_y = ly + bh + u * 2 // 3 + u // 2
    sy2 = theme_y + sm + u // 2
    d = u * 5 // 4
    iw = pw - u * 2
    gap = max(iw - d * 6, 0) // 5
    for i_ in range(6):  # mavzu rangi namunalari (gui.rs ACCENTS)
        g["qacc%d" % i_] = (qx + u + i_ * (d + gap) + d // 2, sy2 + d // 2)
    power_y = sy2 + d + u * 2 // 3 + u // 2
    qh = power_y + u * 3 + u // 2 + sm * 4 // 5 + u // 2 - qy
    g["quick"] = (qx, qy, pw, qh)
    g["park"] = (w - 5, h - 5)  # sichqonchani chetga (hover effektlari bo'lmasin)
    return g
