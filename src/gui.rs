//! Oynalar menejeri: siljitiladigan oynalar, grafik terminal, vazifalar paneli.
//! Grafika (virtio-gpu) va kiriş (virtio-input) bölganda işlaydi.

use crate::drivers::virtio_input as input;
use crate::gfx::{self, *};
use crate::task::{Handle, SpawnOptions};
use crate::{accounts, fs, pipe, task, time};
use alloc::string::String;
use alloc::vec::Vec;

const TITLE_H: usize = 28;
const TASKBAR_H: usize = 40;
const NONE_PIPE: usize = usize::MAX;

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
    lines: Vec<String>, // tugagan çiqiş satrlari
    input: String,      // terminal kiriş qatori (hali yuborilmagan)
    // Haqiqiy EL0 terminal jarayoni bilan quvur orqali bogʻ
    pid: task::Pid,     // 0 — jarayon yöq
    pin: usize,         // kiriş quvuri (GUI yozadi → bola stdin); NONE_PIPE — yöq
    pout: usize,        // çiqiş quvuri (bola stdout → GUI öqiydi); NONE_PIPE — yöq
    cur: Vec<u8>,       // hali tugamagan çiqiş qatori (bayt oqimi)
    esc: Option<String>, // qayta işlanayotgan ESC ketma-ketligi
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

fn base(x: usize, y: usize, w: usize, h: usize, kind: Kind, title: String, lines: Vec<String>) -> Window {
    Window {
        x, y, w, h, kind, title, lines,
        input: String::new(),
        pid: 0, pin: NONE_PIPE, pout: NONE_PIPE,
        cur: Vec::new(), esc: None,
    }
}

fn new_window(kind: Kind, n: usize) -> Window {
    let (x, y) = (120 + n * 40 % 300, 80 + n * 36 % 200);
    match kind {
        Kind::Terminal => {
            let mut win = base(x, y, 580, 380, kind, String::from("Zamin terminali"), Vec::new());
            spawn_terminal(&mut win);
            win
        }
        Kind::Files => {
            let mut lines = alloc::vec![String::from("~ (uy papkasi):")];
            for e in fs::list("/uy").unwrap_or_default() {
                lines.push(alloc::format!("  {}/", e.name));
            }
            base(x, y, 420, 320, kind, String::from("Fayllar"), lines)
        }
        Kind::About => base(x, y, 460, 260, kind, String::from("Zamin OS haqida"), alloc::vec![
            String::from("Zamin OS"),
            alloc::format!("Versiya {}", env!("CARGO_PKG_VERSION")),
            String::new(),
            String::from("Rust'da noldan yozilgan ARM64 OS"),
            String::from("Dasturçi: Abbosjon Dev"),
            String::from("zamin.app · info@zamin.app"),
        ]),
    }
}

/// Oynaga haqiqiy `/dasturlar/terminal` (EL0) jarayonini quvurlar bilan ulaydi.
/// Kiriş quvuri: GUI yozuvçi, bola öquvçi (stdin). Çiqiş quvuri: bola yozuvçi
/// (stdout/stderr), GUI öquvçi.
fn spawn_terminal(win: &mut Window) {
    let Ok(image) = fs::read_all("/dasturlar/terminal") else {
        win.lines.push(String::from("terminal dasturi topilmadi (/dasturlar/terminal)"));
        return;
    };
    let pin = pipe::create_raw();
    let pout = pipe::create_raw();
    pipe::add_ref(pin, true); // GUI — yozuvçi
    pipe::add_ref(pout, false); // GUI — öquvçi
    let opts = SpawnOptions {
        detached: true,
        parent: 0,
        stdio: [Handle::PipeRead(pin), Handle::PipeWrite(pout), Handle::PipeWrite(pout)],
        env: task::default_env(),
        trusted: false, // grafik terminal: tizim buyruqlari (parol, tiklaş) ruxsatsiz
        cwd: task::default_cwd(),
        uid: accounts::current_uid(),
        jail: None,
        extra: None,
    };
    match task::spawn_user("terminal", &image, "", opts) {
        Ok(pid) => {
            win.pid = pid;
            win.pin = pin;
            win.pout = pout;
        }
        Err(_) => {
            pipe::release(pin, true);
            pipe::release(pout, false);
            win.lines.push(String::from("terminalni işga tuşirib bölmadi"));
        }
    }
}

/// Oyna quvurlarini va jarayonini yopadi.
fn close_terminal(win: &mut Window) {
    if win.pid != 0 {
        task::kill(win.pid);
        win.pid = 0;
    }
    if win.pin != NONE_PIPE {
        pipe::release(win.pin, true);
        win.pin = NONE_PIPE;
    }
    if win.pout != NONE_PIPE {
        pipe::release(win.pout, false);
        win.pout = NONE_PIPE;
    }
}

/// Bola jarayon çiqişini (bayt oqimi) oyna satrlariga qoşadi.
/// Oddiy terminal emulyatori: `\n`, `\r`, orqaga oçiriş, ANSI ESC (tozalaş) ni tuşunadi.
fn feed(win: &mut Window, bytes: &[u8]) {
    for &b in bytes {
        if let Some(esc) = win.esc.as_mut() {
            esc.push(b as char);
            if esc.len() >= 2 && (0x40..=0x7e).contains(&b) {
                let seq = core::mem::take(&mut win.esc).unwrap();
                if seq.ends_with('J') {
                    // ekranni tozalaş (\x1b[2J)
                    win.lines.clear();
                    win.cur.clear();
                }
                // boşqa ketma-ketliklar (kursor, \x1b[H) — eʼtiborsiz
            } else if esc.len() > 24 {
                win.esc = None;
            }
            continue;
        }
        match b {
            0x1b => win.esc = Some(String::new()),
            b'\n' => push_line(win),
            b'\r' => win.cur.clear(),
            0x08 => {
                win.cur.pop();
            }
            0..=0x1f => {}
            _ => win.cur.push(b),
        }
    }
    let max = term_rows(win);
    while win.lines.len() > max {
        win.lines.remove(0);
    }
}

fn push_line(win: &mut Window) {
    let line = String::from_utf8_lossy(&core::mem::take(&mut win.cur)).into_owned();
    win.lines.push(line);
}

fn term_rows(win: &Window) -> usize {
    (win.h.saturating_sub(TITLE_H + 24)) / 20
}

/// Terminal oynalaridan çiqişni öqib, satrlarga qoşadi. Bola tugasa — qaytadan işga tuşiradi.
/// Biror narsa özgargan bölsa `true`.
fn pump(windows: &mut [Window]) -> bool {
    let mut changed = false;
    for win in windows.iter_mut() {
        if win.kind != Kind::Terminal || win.pout == NONE_PIPE {
            continue;
        }
        loop {
            match pipe::try_read(win.pout, 4096) {
                Some(data) if !data.is_empty() => {
                    feed(win, &data);
                    changed = true;
                }
                Some(_) => break, // hozirça maʼlumot yöq
                None => {
                    // bola tugadi (stdout yopildi) — yangi terminal oçamiz
                    close_terminal(win);
                    push_line(win);
                    win.lines.push(String::from("[terminal tugadi — qaytadan oçildi]"));
                    spawn_terminal(win);
                    changed = true;
                    break;
                }
            }
        }
    }
    changed
}

pub extern "C" fn run(_: usize) {
    // Birinçi sozlaş serial konsulda ketayotgan bölsa — tugaşini kutamiz
    while accounts::all().is_empty() {
        wait_screen();
        task::sleep_ms(200);
    }
    // Hali kirilmagan bölsa — grafik parol ekrani
    if accounts::current().is_none() {
        login_screen();
    }
    desktop();
}

/// Birinçi sozlaş kutilayotganda körinadigan ekran.
fn wait_screen() {
    let (w, h) = gfx::size();
    gradient(0x101c30, ZAMIN_TUN);
    draw_text_centered(w / 2, h / 2 - 40, "Zamin OS", OQ, 4);
    draw_text_centered(w / 2, h / 2 + 20, "Birinçi sozlaş serial konsulda ketyapti…", KULRANG, 2);
    gfx::flush();
}

/// Grafik kiriş ekrani: foydalanuvçini tanlab, parol kiritadi. Parol toʻgʻri
/// bölgaç qaytadi (iş stoliga ötiladi).
fn login_screen() {
    let (sw, sh) = gfx::size();
    let accts = accounts::all();
    let mut sel = 0usize;
    let mut pass = String::new();
    let mut err = false;
    let mut shift = false;
    let mut cx = sw / 2;
    let mut cy = sh / 2;

    // Karta ölçamlari
    let cw = 460usize;
    let ch = 300usize;
    let card_x = (sw - cw) / 2;
    let card_y = (sh - ch) / 2;
    let btn_w = 160usize;
    let btn_h = 40usize;
    let btn_x = card_x + (cw - btn_w) / 2;
    let btn_y = card_y + ch - 60;
    // Hisob tanlaş qatorlari (bir neçta bölsa)
    let row_h = 28usize;
    let rows_y = card_y + 150;

    loop {
        let mut submit = false;
        for e in input::poll() {
            match (e.typ, e.code) {
                (input::EV_ABS, input::ABS_X) => cx = (e.value as usize * sw) / 32768,
                (input::EV_ABS, input::ABS_Y) => cy = (e.value as usize * sh) / 32768,
                (input::EV_KEY, input::BTN_LEFT) if e.value == 1 => {
                    // "Kiriş" tugmasi?
                    if cx >= btn_x && cx < btn_x + btn_w && cy >= btn_y && cy < btn_y + btn_h {
                        submit = true;
                    }
                    // Hisob tanlaş (bir neçta bölsa)
                    if accts.len() > 1 {
                        for i in 0..accts.len() {
                            let ry = rows_y + i * row_h;
                            if cy >= ry && cy < ry + row_h && cx >= card_x + 40 && cx < card_x + cw - 40 {
                                sel = i;
                                err = false;
                            }
                        }
                    }
                }
                (input::EV_KEY, 42) | (input::EV_KEY, 54) => shift = e.value != 0,
                (input::EV_KEY, 28) if e.value == 1 => submit = true,
                (input::EV_KEY, 14) if e.value == 1 => {
                    pass.pop();
                    err = false;
                }
                (input::EV_KEY, code) if e.value == 1 => {
                    if let Some(c) = keycode_char(code, shift) {
                        if pass.len() < 64 {
                            pass.push(c);
                            err = false;
                        }
                    }
                }
                _ => {}
            }
        }

        if submit && !pass.is_empty() {
            let acct = &accts[sel];
            if let Some(key) = accounts::unlock(acct, &pass) {
                accounts::forget(core::mem::take(&mut pass));
                crate::setup::apply_login(acct, key);
                return; // muvaffaqiyat — iş stoliga
            } else {
                err = true;
                accounts::forget(core::mem::take(&mut pass));
                draw_login(&accts, sel, 0, err, cx, cy, card_x, card_y, cw, ch, btn_x, btn_y, btn_w, btn_h, rows_y, row_h);
                gfx::flush();
                task::sleep_ms(500); // xato: biroz kutiş
            }
        }

        draw_login(&accts, sel, pass.len(), err, cx, cy, card_x, card_y, cw, ch, btn_x, btn_y, btn_w, btn_h, rows_y, row_h);
        gfx::flush();
        task::sleep_ms(16);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_login(
    accts: &[accounts::Account], sel: usize, dots: usize, err: bool, cx: usize, cy: usize,
    card_x: usize, card_y: usize, cw: usize, ch: usize,
    btn_x: usize, btn_y: usize, btn_w: usize, btn_h: usize, rows_y: usize, row_h: usize,
) {
    let (sw, _sh) = gfx::size();
    gradient(0x0e1a2e, ZAMIN_TUN);
    draw_text_centered(sw / 2, card_y - 70, "Zamin OS", OQ, 4);

    // Karta
    fill_rect(card_x + 5, card_y + 6, cw, ch, 0x060d16);
    fill_rect(card_x, card_y, cw, ch, 0x18283e);
    rect_outline(card_x, card_y, cw, ch, ZAMIN_KOK);

    // Qulf belgisi (oddiy)
    let lx = card_x + cw / 2 - 12;
    let ly = card_y + 26;
    rect_outline(lx + 4, ly, 16, 10, SARIQ); // tutqiç
    fill_rect(lx, ly + 10, 24, 18, SARIQ); // tana
    fill_rect(lx + 10, ly + 16, 4, 7, 0x18283e); // teşik

    let acct = &accts[sel];
    draw_text_centered(sw / 2, card_y + 62, &acct.full_name, OQ, 2);
    draw_text_centered(sw / 2, card_y + 86, &alloc::format!("@{}", acct.username), KULRANG, 2);

    if accts.len() > 1 {
        for (i, a) in accts.iter().enumerate() {
            let ry = rows_y + i * row_h;
            let active = i == sel;
            fill_rect(card_x + 40, ry, cw - 80, row_h - 4, if active { ZAMIN_KOK } else { 0x22344e });
            draw_text(card_x + 52, ry + 4, &a.username, OQ, 2);
        }
    } else {
        // Parol maydoni
        let fx = card_x + 60;
        let fy = card_y + 120;
        let fw = cw - 120;
        fill_rect(fx, fy, fw, 34, 0x0c1726);
        rect_outline(fx, fy, fw, 34, if err { QIZIL } else { ZAMIN_KOK });
        for i in 0..dots.min((fw - 20) / 16) {
            fill_rect(fx + 12 + i * 16, fy + 13, 8, 8, OQ);
        }
        if dots == 0 {
            draw_text(fx + 12, fy + 9, "parol…", KULRANG, 2);
        }
    }

    if err {
        draw_text_centered(sw / 2, btn_y - 26, "Parol xato — qayta urining", QIZIL, 2);
    }

    // "Kiriş" tugmasi
    fill_rect(btn_x, btn_y, btn_w, btn_h, ZAMIN_KOK);
    draw_text_centered(btn_x + btn_w / 2, btn_y + 10, "Kiriş →", OQ, 2);

    cursor(cx, cy);
}

fn cursor(cx: usize, cy: usize) {
    for i in 0..18usize {
        let width = (18 - i) / 2 + 1;
        fill_rect(cx, cy + i, width + 2, 1, QORA);
        if i >= 1 {
            fill_rect(cx + 1, cy + i, width.saturating_sub(1), 1, OQ);
        }
    }
}

fn desktop() {
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
        // Terminal jarayonlari çiqişini öqiymiz
        if pump(&mut windows) {
            changed = true;
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
            // Enter: qatorni bola stdin iga yuboramiz; ekranda özimiz aks ettiramiz (bola qaytarmaydi)
            let line = core::mem::take(&mut w.input);
            w.cur.extend_from_slice(line.as_bytes());
            push_line(w);
            if w.pin != NONE_PIPE {
                let mut data = line.into_bytes();
                data.push(b'\n');
                let _ = pipe::try_write(w.pin, &data);
            }
            let max = term_rows(w);
            while w.lines.len() > max {
                w.lines.remove(0);
            }
        }
        14 => {
            w.input.pop();
        }
        _ => {
            if let Some(ch) = keycode_char(code, shift) {
                if w.input.len() < 120 {
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
            let mut w = windows.remove(i);
            close_terminal(&mut w);
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
        let fg = if win.kind == Kind::Terminal { YASHIL } else { OQ };
        let mut ty = win.y + TITLE_H + 8;
        for line in &win.lines {
            if ty + 20 > win.y + win.h {
                break;
            }
            draw_text(win.x + 10, ty, line, fg, 2);
            ty += 20;
        }
        // Terminal: tugamagan qator (odatda taklif) + hozir terilayotgan kiriş
        if win.kind == Kind::Terminal && ty + 20 <= win.y + win.h {
            let cur = String::from_utf8_lossy(&win.cur);
            draw_text(win.x + 10, ty, &alloc::format!("{}{}_", cur, win.input), fg, 2);
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
