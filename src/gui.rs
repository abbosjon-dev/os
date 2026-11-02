//! Oynalar menejeri: siljitiladigan oynalar, grafik terminal, vazifalar paneli.
//! Grafika (virtio-gpu) va kiriş (virtio-input) bölganda işlaydi.

use crate::drivers::virtio_input as input;
use crate::gfx::{self, *};
use crate::{fs, task, time};
use alloc::string::String;
use alloc::vec::Vec;

const TITLE_H: usize = 28;
const TASKBAR_H: usize = 40;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Terminal,
    Files,
    About,
}

struct Window {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    kind: Kind,
    title: String,
    lines: Vec<String>, // çiqiş satrlari
    input: String,      // terminal kiriş qatori
}

impl Window {
    fn title_bar(&self, cx: usize, cy: usize) -> bool {
        cx >= self.x && cx < self.x + self.w && cy >= self.y && cy < self.y + TITLE_H
    }
    fn close_btn(&self, cx: usize, cy: usize) -> bool {
        cx >= self.x + self.w - TITLE_H && cx < self.x + self.w && cy >= self.y && cy < self.y + TITLE_H
    }
    fn contains(&self, cx: usize, cy: usize) -> bool {
        cx >= self.x && cx < self.x + self.w && cy >= self.y && cy < self.y + self.h
    }
}

struct Launcher {
    label: &'static str,
    kind: Kind,
    color: Color,
}

const LAUNCHERS: [Launcher; 3] = [
    Launcher { label: "Terminal", kind: Kind::Terminal, color: ZAMIN_KOK },
    Launcher { label: "Fayllar", kind: Kind::Files, color: YASHIL },
    Launcher { label: "Haqida", kind: Kind::About, color: SARIQ },
];

fn new_window(kind: Kind, n: usize) -> Window {
    let (x, y) = (120 + n * 40 % 300, 80 + n * 36 % 200);
    match kind {
        Kind::Terminal => Window {
            x, y, w: 560, h: 360, kind,
            title: String::from("Zamin terminali"),
            lines: alloc::vec![String::from("Zamin OS grafik terminali. Buyruqlar: yordam")],
            input: String::new(),
        },
        Kind::Files => {
            let mut lines = alloc::vec![String::from("~ (uy papkasi):")];
            for e in fs::list("/uy").unwrap_or_default() {
                lines.push(alloc::format!("  {}/", e.name));
            }
            Window { x, y, w: 420, h: 320, kind, title: String::from("Fayllar"), lines, input: String::new() }
        }
        Kind::About => Window {
            x, y, w: 460, h: 260, kind,
            title: String::from("Zamin OS haqida"),
            lines: alloc::vec![
                String::from("Zamin OS"),
                alloc::format!("Versiya {}", env!("CARGO_PKG_VERSION")),
                String::new(),
                String::from("Rust'da noldan yozilgan ARM64 OS"),
                String::from("Dasturçi: Abbosjon Dev"),
                String::from("zamin.app · info@zamin.app"),
            ],
            input: String::new(),
        },
    }
}

/// Terminal buyruğini bajaradi.
fn run_command(w: &mut Window, cmd: &str) {
    let (name, args) = cmd.split_once(' ').unwrap_or((cmd, ""));
    match name {
        "" => {}
        "yordam" => w.lines.push(String::from("buyruqlar: yordam, vaqt, fayllar, aks, tozala")),
        "vaqt" => {
            let t = time::now();
            w.lines.push(alloc::format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second));
        }
        "fayllar" => {
            let path = if args.is_empty() { "/uy" } else { args };
            match fs::list(path) {
                Ok(list) => {
                    for e in list {
                        w.lines.push(alloc::format!("  {}{}", e.name, if e.is_dir { "/" } else { "" }));
                    }
                }
                Err(_) => w.lines.push(String::from("  papka topilmadi")),
            }
        }
        "aks" => w.lines.push(String::from(args)),
        "tozala" => w.lines.clear(),
        _ => w.lines.push(alloc::format!("nomaʼlum buyruq: {}", name)),
    }
}

pub extern "C" fn run(_: usize) {
    let (sw, sh) = gfx::size();
    let mut windows: Vec<Window> = Vec::new();
    let mut cx = sw / 2;
    let mut cy = sh / 2;
    let mut pressed = false;
    let mut drag: Option<(usize, usize)> = None; // (oyna indeksi, offset)
    let mut shift = false;
    let mut opened = 0;

    draw(&windows, cx, cy);
    gfx::flush();

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
                (input::EV_KEY, input::BTN_LEFT) => {
                    if e.value == 1 {
                        pressed = true;
                        handle_click(&mut windows, cx, cy, sh, &mut drag, &mut opened);
                    } else {
                        pressed = false;
                        drag = None;
                    }
                    changed = true;
                }
                (input::EV_KEY, 42) | (input::EV_KEY, 54) => shift = e.value != 0,
                (input::EV_KEY, code) if e.value == 1 => {
                    if let Some(w) = windows.last_mut().filter(|w| w.kind == Kind::Terminal) {
                        type_key(w, code, shift);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        // Siljitiş
        if pressed {
            if let Some((idx, off)) = drag {
                let nx = cx.saturating_sub(off & 0xffff);
                let ny = cy.saturating_sub(off >> 16);
                if idx < windows.len() {
                    windows[idx].x = nx.min(sw.saturating_sub(40));
                    windows[idx].y = ny.min(sh.saturating_sub(TASKBAR_H + 20));
                    changed = true;
                }
            }
        }
        if changed {
            draw(&windows, cx, cy);
            gfx::flush();
        }
        task::sleep_ms(16);
    }
}

fn type_key(w: &mut Window, code: u16, shift: bool) {
    match code {
        28 => {
            // Enter
            let cmd = core::mem::take(&mut w.input);
            w.lines.push(alloc::format!("zamin> {}", cmd));
            run_command(w, cmd.trim());
            let max = (w.h - TITLE_H - 40) / 20;
            while w.lines.len() > max {
                w.lines.remove(0);
            }
        }
        14 => {
            w.input.pop();
        }
        _ => {
            if let Some(ch) = keycode_char(code, shift) {
                if w.input.len() < 60 {
                    w.input.push(ch);
                }
            }
        }
    }
}

fn handle_click(windows: &mut Vec<Window>, cx: usize, cy: usize, sh: usize, drag: &mut Option<(usize, usize)>, opened: &mut usize) {
    // Vazifalar paneli (pastda)?
    if cy >= sh - TASKBAR_H {
        let bw = 150;
        for (i, l) in LAUNCHERS.iter().enumerate() {
            let bx = 10 + i * (bw + 10);
            if cx >= bx && cx < bx + bw {
                windows.push(new_window(l.kind, *opened));
                *opened += 1;
                return;
            }
        }
        return;
    }
    // Yuqoridan (oxirgi = eng tepada) oynalarni tekşiramiz
    for i in (0..windows.len()).rev() {
        if windows[i].close_btn(cx, cy) {
            windows.remove(i);
            return;
        }
        if windows[i].title_bar(cx, cy) {
            let off = (cx - windows[i].x) | ((cy - windows[i].y) << 16);
            let w = windows.remove(i);
            windows.push(w); // old planga
            *drag = Some((windows.len() - 1, off));
            return;
        }
        if windows[i].contains(cx, cy) {
            let w = windows.remove(i);
            windows.push(w); // old planga — fokus
            return;
        }
    }
}

fn draw(windows: &[Window], cx: usize, cy: usize) {
    let (w, h) = gfx::size();
    gradient(0x14243a, ZAMIN_TUN);
    draw_text(16, 16, "Zamin OS", 0x2a4a74, 3); // fon logotipi

    for (i, win) in windows.iter().enumerate() {
        let active = i == windows.len() - 1;
        // Soya
        fill_rect(win.x + 4, win.y + 4, win.w, win.h, 0x060d16);
        // Tana
        fill_rect(win.x, win.y, win.w, win.h, 0x16273d);
        rect_outline(win.x, win.y, win.w, win.h, if active { ZAMIN_KOK } else { 0x33465f });
        // Sarlavha
        fill_rect(win.x, win.y, win.w, TITLE_H, if active { ZAMIN_KOK } else { 0x2a3a52 });
        draw_text(win.x + 10, win.y + 6, &win.title, OQ, 2);
        // Yopiş tugmasi
        fill_rect(win.x + win.w - TITLE_H, win.y, TITLE_H, TITLE_H, QIZIL);
        draw_text(win.x + win.w - TITLE_H + 8, win.y + 6, "x", OQ, 2);
        // Mazmun
        let mut ty = win.y + TITLE_H + 8;
        for line in &win.lines {
            if ty + 20 > win.y + win.h {
                break;
            }
            draw_text(win.x + 10, ty, line, if win.kind == Kind::Terminal { YASHIL } else { OQ }, 2);
            ty += 20;
        }
        if win.kind == Kind::Terminal && ty + 20 <= win.y + win.h {
            draw_text(win.x + 10, ty, &alloc::format!("zamin> {}_", win.input), OQ, 2);
        }
    }

    // Vazifalar paneli
    fill_rect(0, h - TASKBAR_H, w, TASKBAR_H, 0x0a1420);
    let bw = 150;
    for (i, l) in LAUNCHERS.iter().enumerate() {
        let bx = 10 + i * (bw + 10);
        fill_rect(bx, h - TASKBAR_H + 6, bw, TASKBAR_H - 12, 0x1b2d45);
        rect_outline(bx, h - TASKBAR_H + 6, bw, TASKBAR_H - 12, l.color);
        draw_text(bx + 12, h - TASKBAR_H + 12, l.label, OQ, 2);
    }
    let t = time::now();
    let vaqt = alloc::format!("{:02}:{:02}", t.hour, t.minute);
    draw_text(w - text_width(&vaqt, 2) - 12, h - TASKBAR_H + 12, &vaqt, KULRANG, 2);

    // Kursor
    for i in 0..18usize {
        let width = (18 - i) / 2 + 1;
        fill_rect(cx, cy + i, width + 2, 1, QORA);
        if i >= 1 {
            fill_rect(cx + 1, cy + i, width.saturating_sub(1), 1, OQ);
        }
    }
}

fn keycode_char(code: u16, shift: bool) -> Option<char> {
    let c = match code {
        2..=11 => {
            let row = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
            return Some(row[(code - 2) as usize]);
        }
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't', 21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g', 35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b', 49 => 'n', 50 => 'm',
        57 => ' ',
        52 => '.', 51 => ',', 53 => '/', 12 => '-',
        _ => return None,
    };
    Some(if shift { c.to_ascii_uppercase() } else { c })
}
