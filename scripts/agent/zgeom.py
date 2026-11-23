"""Zamin GUI geometriyasi (src/gui.rs formulalarining Python nusxasi).

Skrinshot skriptlari (shot.py, check_corners.py) har qanday ekran o'lchamida
(1024x768, 1280x800, 1920x1080, ...) dok ikonasi, svetofor tugmalari, manzil
satri va kattalashtirilgan brauzer sahifa sohasini topishi uchun.
gui.rs dagi set_metrics/dock_geom/new_window/zoom_rect/web_geo o'zgarsa —
bu fayl ham yangilanadi.
"""

N_LAUNCHERS = 14
BROWSER = 1  # LAUNCHERS dagi "Brauzer" indeksi
SETTINGS = 12  # LAUNCHERS dagi "Sozlamalar" indeksi


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
    g["park"] = (w - 5, h - 5)  # sichqonchani chetga (hover effektlari bo'lmasin)
    return g
