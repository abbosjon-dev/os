//! DXF (AutoCAD) dekoderi — ASCII, ENTITIES böligi.
//! LINE, CIRCLE, ARC, LWPOLYLINE, POLYLINE/VERTEX, POINT, SOLID.

use super::svg::parse_f32;
use super::vector::{self, flatten_arc, Pt, Scene, Shape};
use super::Image;
use alloc::string::String;
use alloc::vec::Vec;

struct Ent {
    pts: Vec<Pt>,
    closed: bool,
    color: u32,
}

pub fn decode(d: &[u8]) -> Option<Image> {
    let text = core::str::from_utf8(d).ok()?;
    // Guruh kodi / qiymat juftlari
    let mut lines = text.lines();
    let mut pairs: Vec<(i32, String)> = Vec::new();
    while let (Some(code), Some(val)) = (lines.next(), lines.next()) {
        if let Ok(c) = code.trim().parse::<i32>() {
            pairs.push((c, String::from(val.trim())));
        }
    }
    // ENTITIES böligini topamiz
    let mut i = 0;
    while i + 1 < pairs.len() {
        if pairs[i].0 == 0 && pairs[i].1 == "SECTION" && pairs[i + 1].0 == 2 && pairs[i + 1].1 == "ENTITIES" {
            break;
        }
        i += 1;
    }
    if i + 1 >= pairs.len() {
        return None;
    }
    i += 2;
    let mut ents: Vec<Ent> = Vec::new();
    while i < pairs.len() {
        if pairs[i].0 != 0 {
            i += 1;
            continue;
        }
        let typ = pairs[i].1.clone();
        if typ == "ENDSEC" || typ == "EOF" {
            break;
        }
        i += 1;
        // Entity atributlarini yigʻamiz (keyingi 0 gaça)
        let start = i;
        while i < pairs.len() && pairs[i].0 != 0 {
            i += 1;
        }
        let body = &pairs[start..i];
        parse_entity(&typ, body, &mut ents);
    }
    if ents.is_empty() {
        return None;
    }
    build_scene(&ents).map(|s| vector::render(&s, 1000))
}

fn getf(body: &[(i32, String)], code: i32) -> Option<f32> {
    body.iter().find(|(c, _)| *c == code).and_then(|(_, v)| parse_f32(v))
}

fn parse_entity(typ: &str, body: &[(i32, String)], out: &mut Vec<Ent>) {
    let color = aci(body.iter().find(|(c, _)| *c == 62).and_then(|(_, v)| v.parse().ok()).unwrap_or(7));
    match typ {
        "LINE" => {
            let (x0, y0) = (getf(body, 10).unwrap_or(0.0), getf(body, 20).unwrap_or(0.0));
            let (x1, y1) = (getf(body, 11).unwrap_or(0.0), getf(body, 21).unwrap_or(0.0));
            out.push(Ent { pts: alloc::vec![(x0, y0), (x1, y1)], closed: false, color });
        }
        "CIRCLE" => {
            let (cx, cy) = (getf(body, 10).unwrap_or(0.0), getf(body, 20).unwrap_or(0.0));
            let r = getf(body, 40).unwrap_or(0.0);
            if r > 0.0 {
                let mut pts = Vec::new();
                flatten_arc(&mut pts, cx, cy, r, r, 0.0, 6.2831853, 64);
                out.push(Ent { pts, closed: true, color });
            }
        }
        "ARC" => {
            let (cx, cy) = (getf(body, 10).unwrap_or(0.0), getf(body, 20).unwrap_or(0.0));
            let r = getf(body, 40).unwrap_or(0.0);
            let a0 = getf(body, 50).unwrap_or(0.0) * 3.14159265 / 180.0;
            let mut a1 = getf(body, 51).unwrap_or(360.0) * 3.14159265 / 180.0;
            if a1 < a0 {
                a1 += 6.2831853;
            }
            if r > 0.0 {
                let mut pts = Vec::new();
                flatten_arc(&mut pts, cx, cy, r, r, a0, a1, 48);
                out.push(Ent { pts, closed: false, color });
            }
        }
        "LWPOLYLINE" => {
            let closed = body.iter().find(|(c, _)| *c == 70).map(|(_, v)| v.parse::<i32>().unwrap_or(0) & 1 != 0).unwrap_or(false);
            let mut pts = Vec::new();
            let mut cur_x = None;
            for (c, v) in body {
                if *c == 10 {
                    cur_x = parse_f32(v);
                } else if *c == 20 {
                    if let (Some(x), Some(y)) = (cur_x, parse_f32(v)) {
                        pts.push((x, y));
                    }
                }
            }
            if pts.len() >= 2 {
                out.push(Ent { pts, closed, color });
            }
        }
        "SOLID" | "3DFACE" => {
            let mut pts = Vec::new();
            for (ci, cj) in [(10, 20), (11, 21), (13, 23), (12, 22)] {
                if let (Some(x), Some(y)) = (getf(body, ci), getf(body, cj)) {
                    pts.push((x, y));
                }
            }
            if pts.len() >= 3 {
                out.push(Ent { pts, closed: true, color });
            }
        }
        "POINT" => {
            let (x, y) = (getf(body, 10).unwrap_or(0.0), getf(body, 20).unwrap_or(0.0));
            out.push(Ent { pts: alloc::vec![(x - 0.5, y), (x + 0.5, y)], closed: false, color });
        }
        _ => {}
    }
}

fn build_scene(ents: &[Ent]) -> Option<Scene> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for e in ents {
        for &(x, y) in &e.pts {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
    }
    if minx > maxx {
        return None;
    }
    let (w, h) = ((maxx - minx).max(1.0), (maxy - miny).max(1.0));
    let pad = (w.max(h)) * 0.03;
    let mut scene = Scene::new(w + pad * 2.0, h + pad * 2.0);
    scene.bg = 0xffffff;
    for e in ents {
        // y-up → y-down (CAD koordinatasi teskari)
        let mut sp: Vec<Pt> = e.pts.iter().map(|&(x, y)| (x - minx + pad, maxy - y + pad)).collect();
        if e.closed && sp.len() > 2 {
            let f = sp[0];
            sp.push(f);
        }
        scene.shapes.push(Shape { subpaths: alloc::vec![sp], fill: None, stroke: Some(e.color), stroke_w: w.max(h) / 400.0, nonzero: false });
    }
    Some(scene)
}

/// AutoCAD rang indeksi (ACI) → RGB (asosiylari).
fn aci(i: i32) -> u32 {
    match i {
        1 => 0xff0000,
        2 => 0xffff00,
        3 => 0x00ff00,
        4 => 0x00ffff,
        5 => 0x0000ff,
        6 => 0xff00ff,
        7 | 0 => 0x000000, // oq fonda qora
        8 => 0x808080,
        9 => 0xc0c0c0,
        _ => 0x000000,
    }
}
