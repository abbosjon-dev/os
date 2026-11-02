//! Oddiy grafik muhit: siçqonça kursori, bosiladigan panellar, klaviaturadan matn.
//! Grafika (virtio-gpu) va kiriş (virtio-input) bölsa işlaydi.

use crate::drivers::virtio_input as input;
use crate::gfx::{self, *};
use crate::{task, time};
use alloc::string::String;

/// Linux klaviatura kodidan belgi (US joylaşuvi, kiçik/katta).
fn keycode_char(code: u16, shift: bool) -> Option<char> {
    let c = match code {
        2..=11 => {
            let row = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
            let sh = ['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'];
            return Some(if shift { sh[(code - 2) as usize] } else { row[(code - 2) as usize] });
        }
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't', 21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g', 35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b', 49 => 'n', 50 => 'm',
        57 => ' ',
        12 => return Some(if shift { '_' } else { '-' }),
        13 => return Some(if shift { '+' } else { '=' }),
        51 => return Some(if shift { '<' } else { ',' }),
        52 => return Some(if shift { '>' } else { '.' }),
        53 => return Some(if shift { '?' } else { '/' }),
        _ => return None,
    };
    Some(if shift { c.to_ascii_uppercase() } else { c })
}

const KEY_ENTER: u16 = 28;
const KEY_BACKSPACE: u16 = 14;
const KEY_LSHIFT: u16 = 42;
const KEY_RSHIFT: u16 = 54;

struct Panel {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    label: &'static str,
    color: Color,
}

/// Grafik muhitni işga tuşiradi (alohida yadro oqimida).
pub extern "C" fn run(_: usize) {
    let (sw, sh) = gfx::size();
    let mut cx = sw / 2;
    let mut cy = sh / 2;
    let mut shift = false;
    let mut typed = String::from("Bu yerga yozing: ");
    let mut status = String::from("Panelni bosing yoki yozing");

    let panels = [
        Panel { x: 160, y: 520, w: 220, h: 90, label: "Terminal", color: ZAMIN_KOK },
        Panel { x: 402, y: 520, w: 220, h: 90, label: "Fayllar", color: YASHIL },
        Panel { x: 644, y: 520, w: 220, h: 90, label: "Tarmoq", color: SARIQ },
    ];

    loop {
        let mut changed = false;
        for e in input::poll() {
            match (e.typ, e.code) {
                (input::EV_ABS, input::ABS_X) => {
                    cx = (e.value as usize * sw) / 32768;
                    changed = true;
                }
                (input::EV_ABS, input::ABS_Y) => {
                    cy = (e.value as usize * sh) / 32768;
                    changed = true;
                }
                (input::EV_KEY, input::BTN_LEFT) if e.value == 1 => {
                    status = String::from("Boş joy bosildi");
                    for p in &panels {
                        if cx >= p.x && cx < p.x + p.w && cy >= p.y && cy < p.y + p.h {
                            status = alloc::format!("{} oçildi", p.label);
                        }
                    }
                    changed = true;
                }
                (input::EV_KEY, KEY_LSHIFT | KEY_RSHIFT) => shift = e.value != 0,
                (input::EV_KEY, KEY_ENTER) if e.value == 1 => {
                    typed.push('\n');
                    typed = String::from("Bu yerga yozing: ");
                    changed = true;
                }
                (input::EV_KEY, KEY_BACKSPACE) if e.value == 1 => {
                    typed.pop();
                    changed = true;
                }
                (input::EV_KEY, code) if e.value == 1 => {
                    if let Some(ch) = keycode_char(code, shift) {
                        if typed.chars().count() < 60 {
                            typed.push(ch);
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if changed {
            draw(&panels, &typed, &status, cx, cy);
            gfx::flush();
        }
        task::sleep_ms(16);
    }
}

fn draw(panels: &[Panel], typed: &str, status: &str, cx: usize, cy: usize) {
    let (w, _h) = gfx::size();
    gradient(0x14243a, ZAMIN_TUN);
    // Yuqori panel
    fill_rect(0, 0, w, 36, 0x0a1420);
    draw_text(12, 9, "Zamin OS", OQ, 2);
    let t = time::now();
    let vaqt = alloc::format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
    draw_text(w - text_width(&vaqt, 2) - 12, 9, &vaqt, KULRANG, 2);

    // Matn qutisi
    let (bx, by, bw, bh) = (160, 120, 704, 300);
    fill_rect(bx, by, bw, bh, 0x16273d);
    rect_outline(bx, by, bw, bh, ZAMIN_KOK);
    draw_text(bx + 20, by + 20, "Grafik muhit — kiriş sinovi", KULRANG, 2);
    draw_text(bx + 20, by + 70, typed, OQ, 3);
    draw_text(bx + 20, bh + by - 50, status, SARIQ, 2);

    // Panellar
    for p in panels {
        let over = cx >= p.x && cx < p.x + p.w && cy >= p.y && cy < p.y + p.h;
        fill_rect(p.x, p.y, p.w, p.h, if over { 0x24395a } else { 0x1b2d45 });
        rect_outline(p.x, p.y, p.w, p.h, p.color);
        fill_rect(p.x + 16, p.y + 20, 50, 50, p.color);
        draw_text(p.x + 84, p.y + 36, p.label, OQ, 2);
    }

    // Siçqonça kursori: qora çegarali oq uçburçak (strelka)
    for i in 0..18usize {
        let width = (18 - i) / 2 + 1;
        fill_rect(cx, cy + i, width + 2, 1, QORA); // çegara
        if i >= 1 {
            fill_rect(cx + 1, cy + i, width.saturating_sub(1), 1, OQ);
        }
    }
}
