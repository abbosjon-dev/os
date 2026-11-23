//! USB HID (boot protokoli) hisobotlarini virtio-input hodisalariga aylantiriş.
//!
//! Klaviatura: 8 baytli boot hisoboti — [modifikatorlar, 0, tugma×6]. Oldingi va joriy
//! hisobot solişiriladi: yangi paydo bölgan tugma — bosiş (value 1), yöqolgani —
//! böşatiş (value 0), ikkalasida ham bor — uşlab turiliş (hodisa yöq). Kodlar —
//! Linux KEY_* (virtio-input bilan bir xil), şuning uçun GUI ikkala manbani bir xil öqiydi.
//! Planşet (QEMU usb-tablet): [tugmalar, X lo, X hi, Y lo, Y hi, g'ildirak] — X/Y 0..32767.
//! Siçqonça (boot, nisbiy): [tugmalar, dx, dy] — absolyut kursorga yiğiladi.

use super::virtio_input::{ABS_X, ABS_Y, BTN_LEFT, EV_ABS, EV_KEY, Event};
use alloc::vec::Vec;

const EV_REL: u16 = 2;
const REL_WHEEL: u16 = 8;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;

/// Absolyut koordinata çegarasi (virtio-tablet bilan bir xil: 0..32767).
pub const ABS_MAX: u32 = 32767;

/// Modifikator bitlari (0..7) → Linux kodlari: LCtrl, LShift, LAlt, LMeta, RCtrl, RShift, RAlt, RMeta.
const MODS: [u16; 8] = [29, 42, 56, 125, 97, 54, 100, 126];

/// HID klaviatura usage (0x00..0x73) → Linux KEY_* (0 — yöq).
#[rustfmt::skip]
const USAGE: [u8; 0x68] = [
    0, 0, 0, 0, 30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, // 00-0f: a..l
    50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, 2, 3, // 10-1f: m..z, 1, 2
    4, 5, 6, 7, 8, 9, 10, 11, 28, 1, 14, 15, 57, 12, 13, 26,     // 20-2f: 3..0, Enter, Esc, BS, Tab, Space, - = [
    27, 43, 43, 39, 40, 41, 51, 52, 53, 58, 59, 60, 61, 62, 63, 64, // 30-3f: ] \ # ; ' ` , . / Caps F1..F6
    65, 66, 67, 68, 87, 88, 99, 70, 119, 110, 102, 104, 111, 107, 109, 106, // 40-4f: F7..F12 PrtSc ScrLk Pause Ins Home PgUp Del End PgDn →
    105, 108, 103, 69, 98, 55, 74, 78, 96, 79, 80, 81, 75, 76, 77, 71, // 50-5f: ← ↓ ↑ NumLk KP/ KP* KP- KP+ KPEnter KP1..KP7
    72, 73, 82, 83, 86, 127, 116, 117,                             // 60-67: KP8 KP9 KP0 KP. 102nd Compose Power KP=
];

/// HID usage → Linux tugma kodi.
pub fn usage_to_key(u: u8) -> u16 {
    match u {
        0x68..=0x73 => 183 + (u - 0x68) as u16, // F13..F24
        _ => USAGE.get(u as usize).copied().unwrap_or(0) as u16,
    }
}

/// Ikki boot klaviatura hisoboti orasidagi farq: (Linux kod, bosildimi) ro'yxati.
/// Avval böşatişlar, keyin bosişlar (modifikatorlar har guruhda birinçi).
/// "Rollover xatosi" (usage 0x01 — 6 dan ortiq tugma) bölsa — holat özgarmaydi.
pub fn hid_report_diff(prev: &[u8; 8], cur: &[u8; 8]) -> Vec<(u16, bool)> {
    let mut out = Vec::new();
    if cur[2..].iter().any(|&u| (1..=3).contains(&u)) {
        return out;
    }
    let has = |r: &[u8; 8], u: u8| r[2..].contains(&u);
    for (b, &code) in MODS.iter().enumerate() {
        if prev[0] & (1 << b) != 0 && cur[0] & (1 << b) == 0 {
            out.push((code, false));
        }
    }
    for &u in &prev[2..] {
        if u > 3 && !has(cur, u) {
            let k = usage_to_key(u);
            if k != 0 {
                out.push((k, false));
            }
        }
    }
    for (b, &code) in MODS.iter().enumerate() {
        if prev[0] & (1 << b) == 0 && cur[0] & (1 << b) != 0 {
            out.push((code, true));
        }
    }
    for (i, &u) in cur[2..].iter().enumerate() {
        // takroriy usage bir hisobotda ikki marta kelsa — bitta bosiş
        if u > 3 && !has(prev, u) && !cur[2..2 + i].contains(&u) {
            let k = usage_to_key(u);
            if k != 0 {
                out.push((k, true));
            }
        }
    }
    out
}

/// Qurilma mantiqiy koordinatasini (0..`max`) virtio-tablet şkalasiga (0..32767) ötkazadi.
pub fn scale_abs(v: u32, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    (v.min(max) as u64 * ABS_MAX as u64 / max as u64) as u32
}

fn key(code: u16, down: bool) -> Event {
    Event { typ: EV_KEY, code, value: down as u32 }
}

fn buttons(prev: u8, cur: u8, out: &mut Vec<Event>) {
    for (b, code) in [(0, BTN_LEFT), (1, BTN_RIGHT), (2, BTN_MIDDLE)] {
        let (p, c) = (prev & (1 << b) != 0, cur & (1 << b) != 0);
        if p != c {
            out.push(key(code, c));
        }
    }
}

/// Klaviatura hisoboti → hodisalar.
pub fn keyboard_events(prev: &[u8; 8], cur: &[u8; 8]) -> Vec<Event> {
    hid_report_diff(prev, cur).into_iter().map(|(c, d)| key(c, d)).collect()
}

/// Planşet hisoboti (QEMU usb-tablet) → hodisalar. Avval koordinata, keyin tugmalar
/// (bosiş yangi nuqtada qayd etilsin).
pub fn tablet_events(prev_buttons: u8, r: &[u8]) -> Vec<Event> {
    let mut out = Vec::new();
    if r.len() < 5 {
        return out;
    }
    let x = u16::from_le_bytes([r[1], r[2]]) as u32 & 0x7fff;
    let y = u16::from_le_bytes([r[3], r[4]]) as u32 & 0x7fff;
    out.push(Event { typ: EV_ABS, code: ABS_X, value: scale_abs(x, 0x7fff) });
    out.push(Event { typ: EV_ABS, code: ABS_Y, value: scale_abs(y, 0x7fff) });
    if r.len() > 5 && r[5] != 0 {
        out.push(Event { typ: EV_REL, code: REL_WHEEL, value: r[5] as i8 as i32 as u32 });
    }
    buttons(prev_buttons, r[0], &mut out);
    out
}

/// Boot siçqonça hisoboti (nisbiy) → absolyut hodisalar; `pos` — joriy (0..32767) kursor.
pub fn mouse_events(prev_buttons: u8, r: &[u8], pos: &mut (u32, u32)) -> Vec<Event> {
    let mut out = Vec::new();
    if r.len() < 3 {
        return out;
    }
    // ~32 qadam ekran kengligining 1/100 qismi: nisbiy birlikni 0..32767 şkalaga kattalaştiramiz
    let step = |p: u32, d: i8| (p as i32 + d as i32 * 12).clamp(0, ABS_MAX as i32) as u32;
    if r[1] != 0 || r[2] != 0 {
        pos.0 = step(pos.0, r[1] as i8);
        pos.1 = step(pos.1, r[2] as i8);
        out.push(Event { typ: EV_ABS, code: ABS_X, value: pos.0 });
        out.push(Event { typ: EV_ABS, code: ABS_Y, value: pos.1 });
    }
    if r.len() > 3 && r[3] != 0 {
        out.push(Event { typ: EV_REL, code: REL_WHEEL, value: r[3] as i8 as i32 as u32 });
    }
    buttons(prev_buttons, r[0], &mut out);
    out
}

/// O'z-o'zini sinaş: modifikatorlar, 6 tugmali rollover, böşatiş/uşlab turiş, planşet şkalasi.
pub fn self_test() -> Result<usize, &'static str> {
    let mut n = 0;
    let mut check = |ok: bool, what: &'static str| {
        n += 1;
        if ok { Ok(()) } else { Err(what) }
    };
    let z = [0u8; 8];
    // 'a' bosildi → KEY_A (30) bosiş
    let a = [0, 0, 0x04, 0, 0, 0, 0, 0];
    check(hid_report_diff(&z, &a) == [(30, true)], "a bosiş")?;
    // uşlab turiş: hodisa yöq (takror bosiş emas)
    check(hid_report_diff(&a, &a).is_empty(), "uşlab turiş")?;
    // böşatiş
    check(hid_report_diff(&a, &z) == [(30, false)], "a böşatiş")?;
    // Shift (LShift=bit1) + '1' → 42 bosiş, keyin 2 bosiş
    let s1 = [0x02, 0, 0x1e, 0, 0, 0, 0, 0];
    check(hid_report_diff(&z, &s1) == [(42, true), (2, true)], "Shift+1")?;
    // Shift qoldi, '1' böşatildi
    let s = [0x02, 0, 0, 0, 0, 0, 0, 0];
    check(hid_report_diff(&s1, &s) == [(2, false)], "Shift uşlab, 1 böşatiş")?;
    // RCtrl + RAlt (AltGr) + LMeta
    let m = [0x10 | 0x40 | 0x08, 0, 0, 0, 0, 0, 0, 0];
    check(hid_report_diff(&z, &m) == [(125, true), (97, true), (100, true)], "RCtrl/AltGr/Meta")?;
    check(hid_report_diff(&m, &z) == [(125, false), (97, false), (100, false)], "modifikator böşatiş")?;
    // 6 tugmali rollover: a s d f j k bir vaqtda
    let six = [0, 0, 0x04, 0x16, 0x07, 0x09, 0x0d, 0x0e];
    check(hid_report_diff(&z, &six) == [(30, true), (31, true), (32, true), (33, true), (36, true), (37, true)], "6KRO")?;
    // 7-tugma: rollover xatosi (0x01 ×6) — holat özgarmaydi, hodisa yöq
    let err = [0, 0, 1, 1, 1, 1, 1, 1];
    check(hid_report_diff(&six, &err).is_empty(), "rollover xatosi")?;
    // tartib almaşsa ham (s, a joyi özgardi) — hodisa yöq; bittasi (d) böşatildi, l bosildi
    let six2 = [0, 0, 0x16, 0x04, 0x0f, 0x09, 0x0d, 0x0e];
    check(hid_report_diff(&six, &six2) == [(32, false), (38, true)], "6KRO almaşiş")?;
    // maxsus tugmalar: Enter, Backspace, strelkalar, F12, F13
    let sp = [0, 0, 0x28, 0x2a, 0x52, 0x45, 0x68, 0];
    check(hid_report_diff(&z, &sp) == [(28, true), (14, true), (103, true), (88, true), (183, true)], "maxsus tugmalar")?;
    // keyboard_events: value 1/0
    let ev = keyboard_events(&z, &a);
    check(ev.len() == 1 && ev[0].typ == EV_KEY && ev[0].code == 30 && ev[0].value == 1, "hodisa")?;
    // planşet: 0..32767 şkala, ekranga (GUI: v * w / 32768) aylantiriş çegarada
    check(scale_abs(0, 0x7fff) == 0 && scale_abs(0x7fff, 0x7fff) == ABS_MAX, "şkala çegaralari")?;
    check(scale_abs(0x4000, 0x7fff) == 0x4000 && scale_abs(99999, 0x7fff) == ABS_MAX, "şkala örta/oşiq")?;
    check(scale_abs(1023, 1023) == ABS_MAX && scale_abs(512, 1023) == 16399, "boşqa mantiqiy max")?;
    let to_px = |v: u32, w: u32| v * w / 32768;
    check(to_px(ABS_MAX, 1920) == 1919 && to_px(ABS_MAX / 2, 1080) == 539, "ekran piksellari")?;
    // planşet hisoboti: X=0x7fff, Y=0x4000, chap tugma bosildi
    let t = tablet_events(0, &[1, 0xff, 0x7f, 0x00, 0x40, 0]);
    check(t.len() == 3 && t[0].value == ABS_MAX && t[1].value == 0x4000 && t[2].code == BTN_LEFT && t[2].value == 1, "planşet bosiş")?;
    let t = tablet_events(1, &[0, 0, 0, 0, 0, 0]);
    check(t.len() == 3 && t[0].value == 0 && t[2].code == BTN_LEFT && t[2].value == 0, "planşet böşatiş")?;
    // nisbiy siçqonça: çegaradan çiqmaydi
    let mut pos = (10, ABS_MAX - 5);
    let r = mouse_events(0, &[2, 0x80, 0x7f], &mut pos);
    check(pos == (0, ABS_MAX) && r.len() == 3 && r[2].code == BTN_RIGHT, "siçqonça")?;
    Ok(n)
}
