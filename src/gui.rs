//! Oynalar menejeri: siljitiladigan oynalar, grafik terminal, vazifalar paneli.
//! Grafika (virtio-gpu) va kiriş (virtio-input) bölganda işlaydi.

use crate::drivers::virtio_input as input;
use crate::gfx::{self, Font, *};
use crate::task::{Handle, SpawnOptions};
use crate::{accounts, fs, pipe, task, time};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

const NONE_PIPE: usize = usize::MAX;

// --- Moslaşuvçi ölçamlar (ekran kattaligiga qarab) ---
static M_BODY: AtomicUsize = AtomicUsize::new(18);

/// Ekran ölçamidan UI ölçamlarini hisoblaydi (istalgan ölçamga moslaşadi).
fn set_metrics(sh: usize) {
    let body = (sh / 42).clamp(14, 32);
    M_BODY.store(body, Ordering::Relaxed);
}
/// Asosiy matn ölçami (piksel).
fn body() -> usize {
    M_BODY.load(Ordering::Relaxed)
}
fn small() -> usize {
    (body() * 8 / 10).max(12)
}
/// Sarlavha paneli balandligi.
fn title_h() -> usize {
    body() * 2
}
/// Dok (vazifalar paneli) balandligi.
fn dock_h() -> usize {
    body() * 7 / 2
}
/// Oyna burçak radiusi.
fn radius() -> usize {
    (body() * 5 / 7).max(8)
}
/// Yuqori menyu paneli balandligi.
fn menubar_h() -> usize {
    body() * 3 / 2
}

// Mac uslubidagi ranglar (tün mavzu)
const WIN_BODY: Color = 0x1b2130;
const WIN_TITLE: Color = 0x262d3e;
const DESK_TOP: Color = 0x16335e;
const DESK_BOT: Color = 0x0a1322;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Terminal,
    Files,
    Editor,
    Clock,
    Image,
    Audio,
    About,
}

impl Kind {
    /// Oyna klaviaturadan matn qabul qiladimi?
    fn takes_keys(self) -> bool {
        matches!(self, Kind::Terminal | Kind::Editor)
    }
}

const NOTES_PATH: &str = "~/qaydlar.txt";

/// Fayl turi (Fayllar oynasida ikona va oçişni tanlaş uçun).
#[derive(Clone, Copy, PartialEq)]
enum FKind {
    Dir,
    Image,
    Text,
    Program,
    Audio,
    Video,
    Archive,
    Other,
}

impl FKind {
    /// Fayl nomidan (kengaytma) turini aniqlaydi.
    fn of(name: &str, is_dir: bool) -> FKind {
        if is_dir {
            return FKind::Dir;
        }
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        match ext.as_str() {
            // Rasm formatlari (koʻpi tanilади; dekodlanadiganlari image modulida)
            "jpg" | "jpeg" | "jpe" | "jfif" | "jif" | "png" | "apng" | "webp" | "gif" | "svg" | "avif" | "heif"
            | "heic" | "bmp" | "dib" | "tif" | "tiff" | "ico" | "cur" | "jxl" | "ai" | "eps" | "cdr" | "wmf" | "emf"
            | "dxf" | "cgm" | "psd" | "psb" | "xcf" | "kra" | "ora" | "afphoto" | "procreate" | "exr" | "hdr" | "tga"
            | "dng" | "cr2" | "cr3" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "raf" | "orf" | "rw2" | "pef" | "crw"
            | "3fr" | "iiq" | "dds" | "ktx" | "ktx2" | "pvr" | "astc" | "basis" | "qoi" | "pcx" | "ppm" | "pgm"
            | "pbm" | "pnm" | "xbm" | "xpm" | "icns" | "jp2" | "j2k" | "jpf" | "jpx" | "jng" | "mng" | "bpg" | "flif"
            | "fits" | "fit" | "dicom" | "dcm" | "ani" => FKind::Image,
            "txt" | "md" | "rs" | "toml" | "c" | "h" | "cfg" | "conf" | "log" | "json" | "csv" | "xml" | "html" | "sh" => FKind::Text,
            "wav" | "mp3" | "ogg" | "flac" | "m4a" | "aac" => FKind::Audio,
            "mp4" | "avi" | "mkv" | "mov" | "webm" => FKind::Video,
            "zip" | "tar" | "gz" | "xz" | "zst" | "7z" => FKind::Archive,
            "" => FKind::Program, // kengaytmasiz — odatda dastur
            _ => FKind::Other,
        }
    }
    /// Ranggi (ikona uçun).
    fn color(self) -> Color {
        match self {
            FKind::Dir => 0x56a8f5,
            FKind::Image => 0xd36b9a,
            FKind::Text => 0x9fb0c3,
            FKind::Program => 0x30d158,
            FKind::Audio => 0xffd426,
            FKind::Video => 0xff7a9c,
            FKind::Archive => 0xc0924a,
            FKind::Other => 0x7a8aa0,
        }
    }
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
    path: String,       // Fayllar: joriy papka; Matn/Rasm: fayl yoʻli
    rows: Vec<(String, FKind)>, // Fayllar oynasi qatorlari (nom, tur)
    img: Option<crate::image::Image>, // Rasm oynasi: dekodlangan rasm (yoki joriy kadr)
    frames: Vec<(crate::image::Image, u32)>, // harakatli rasm kadrlari (rasm, ms)
    frame: usize,       // joriy kadr indeksi
    frame_ms: u64,      // oxirgi kadr almaşgan vaqt (ms)
    clip: Option<crate::audio::Clip>, // Audio oynasi: övoz
    env: Vec<(i16, i16)>, // toʻlqin öramasi (min,max)
    playing: bool,      // ijro ketyaptimi
    play_pos: u32,      // ijro örni (ms)
    play_t: u64,        // oxirgi yangilaş vaqti (ms)
    // Haqiqiy EL0 terminal jarayoni bilan quvur orqali bogʻ
    pid: task::Pid,     // 0 — jarayon yöq
    pin: usize,         // kiriş quvuri (GUI yozadi → bola stdin); NONE_PIPE — yöq
    pout: usize,        // çiqiş quvuri (bola stdout → GUI öqiydi); NONE_PIPE — yöq
    cur: Vec<u8>,       // hali tugamagan çiqiş qatori (bayt oqimi)
    esc: Option<String>, // qayta işlanayotgan ESC ketma-ketligi
}

impl Window {
    fn title_bar(&self, cx: usize, cy: usize) -> bool {
        cx >= self.x && cx < self.x + self.w && cy >= self.y && cy < self.y + title_h()
    }
    /// Çap tarafdagi yopiş nuqtasi markazi va radiusi.
    fn close_dot(&self) -> (usize, usize, usize) {
        let r = (body() * 2 / 5).max(5);
        (self.x + title_h() / 2 + r, self.y + title_h() / 2, r)
    }
    fn close_btn(&self, cx: usize, cy: usize) -> bool {
        let (dx, dy, r) = self.close_dot();
        let r = r + 3; // bosiş uçun biroz kattaroq
        cx + r >= dx && cx <= dx + r && cy + r >= dy && cy <= dy + r
    }
    fn contains(&self, cx: usize, cy: usize) -> bool {
        cx >= self.x && cx < self.x + self.w && cy >= self.y && cy < self.y + self.h
    }
    /// Matn muharriridagi "Saqla" tugmasi (sarlavha ostidagi asboblar qatorida).
    fn save_btn(&self) -> (usize, usize, usize, usize) {
        let bw = body() * 5;
        let bh = body() * 3 / 2;
        (self.x + self.w - bw - body() / 2, self.y + title_h() + body() / 3, bw, bh)
    }
    fn in_save_btn(&self, cx: usize, cy: usize) -> bool {
        let (bx, by, bw, bh) = self.save_btn();
        cx >= bx && cx < bx + bw && cy >= by && cy < by + bh
    }
    /// Mazmun boşlanadigan y (sarlavhadan keyin).
    fn content_top(&self) -> usize {
        self.y + title_h() + body() / 3
    }
    /// Övoz oynasidagi ijro tugmasi (doira), markaz va radius.
    fn play_btn(&self) -> (usize, usize, usize) {
        let r = body();
        (self.x + body() + r, self.y + self.h - body() - r, r)
    }
    fn in_play_btn(&self, cx: usize, cy: usize) -> bool {
        let (px, py, r) = self.play_btn();
        let (dx, dy) = (cx as isize - px as isize, cy as isize - py as isize);
        (dx * dx + dy * dy) as usize <= (r + 4) * (r + 4)
    }
}

struct Launcher {
    label: &'static str,
    kind: Kind,
    color: Color,
}

const LAUNCHERS: [Launcher; 6] = [
    Launcher { label: "Terminal", kind: Kind::Terminal, color: ZAMIN_KOK },
    Launcher { label: "Fayllar", kind: Kind::Files, color: YASHIL },
    Launcher { label: "Matn", kind: Kind::Editor, color: 0x4aa3df },
    Launcher { label: "Soat", kind: Kind::Clock, color: SARIQ },
    Launcher { label: "Rasm", kind: Kind::Image, color: 0xd36b9a },
    Launcher { label: "Haqida", kind: Kind::About, color: KULRANG },
];

/// Dok (markazlashgan panel) geometriyasi: (dx, dy, dw, dh, icon, gap).
fn dock_geom(sw: usize, sh: usize) -> (usize, usize, usize, usize, usize, usize) {
    let n = LAUNCHERS.len();
    let dh = dock_h();
    let pad = dh / 6;
    let icon = dh - pad * 2;
    let gap = icon / 5;
    let dw = n * icon + (n + 1) * gap;
    let dx = sw.saturating_sub(dw) / 2;
    let dy = sh.saturating_sub(dh + pad);
    (dx, dy, dw, dh, icon, gap)
}

/// i-chi dok ikonasining töğriburçagi: (x, y, ölçam).
fn dock_icon_rect(sw: usize, sh: usize, i: usize) -> (usize, usize, usize) {
    let (dx, dy, _dw, dh, icon, gap) = dock_geom(sw, sh);
    let pad = dh / 6;
    (dx + gap + i * (icon + gap), dy + pad, icon)
}

fn base(x: usize, y: usize, w: usize, h: usize, kind: Kind, title: String, lines: Vec<String>) -> Window {
    Window {
        x, y, w, h, kind, title, lines,
        input: String::new(),
        path: String::new(),
        rows: Vec::new(),
        img: None,
        frames: Vec::new(),
        frame: 0,
        frame_ms: 0,
        clip: None,
        env: Vec::new(),
        playing: false,
        play_pos: 0,
        play_t: 0,
        pid: 0, pin: NONE_PIPE, pout: NONE_PIPE,
        cur: Vec::new(), esc: None,
    }
}

fn new_window(kind: Kind, n: usize) -> Window {
    let (sw, sh) = gfx::size();
    let u = body(); // ölçam birligi
    // Oynani markazga yaqin, biroz surilgan joyga qöyamiz
    let place = |w: usize, h: usize| {
        // Kaskad: har yangi oyna biroz pastga-öngga suriladi
        let step = u * 2;
        let x = sw / 8 + (n * step) % (sw / 3);
        let y = menubar_h() + u * 2 + (n * step) % (sh / 4);
        let xmax = sw.saturating_sub(w).max(1);
        let ymax = sh.saturating_sub(dock_h() + h + u).max(menubar_h());
        (x.min(xmax), y.min(ymax))
    };
    match kind {
        Kind::Terminal => {
            let (w, h) = (u * 32, u * 21);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Zamin terminali"), Vec::new());
            spawn_terminal(&mut win);
            win
        }
        Kind::Files => {
            let (w, h) = (u * 26, u * 20);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Fayllar"), Vec::new());
            load_dir(&mut win, "~");
            win
        }
        Kind::Editor => {
            let mut lines: Vec<String> = fs::read_all(NOTES_PATH)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).lines().map(String::from).collect())
                .unwrap_or_default();
            let input = lines.pop().unwrap_or_default();
            let (w, h) = (u * 29, u * 20);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Matn — qaydlar.txt"), lines);
            win.input = input;
            win.path = String::from(NOTES_PATH);
            win
        }
        Kind::Clock => {
            let (w, h) = (u * 20, u * 11);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Soat"), Vec::new())
        }
        Kind::Image => {
            let (w, h) = (u * 27, u * 20);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Rasm — Zamin"), Vec::new())
        }
        Kind::About => {
            let (w, h) = (u * 25, u * 15);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Zamin OS haqida"), alloc::vec![
                String::from("Zamin OS"),
                alloc::format!("Versiya {}", env!("CARGO_PKG_VERSION")),
                String::new(),
                String::from("Rust'da noldan yozilgan ARM64 OS"),
                String::from("Dasturçi: Abbosjon Dev"),
                String::from("zamin.app · info@zamin.app"),
            ])
        }
        Kind::Audio => {
            // Audio faqat fayldan oçiladi (dokdan emas)
            let (w, h) = (u * 30, u * 16);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Övoz"), Vec::new())
        }
    }
}

/// Fayllar oynasiga papka tarkibini yuklaydi (papkalar oldin, keyin alifbo).
fn load_dir(win: &mut Window, path: &str) {
    win.path = String::from(path);
    win.rows.clear();
    if path != "/" && path != "~" {
        win.rows.push((String::from(".."), FKind::Dir));
    }
    let mut entries = fs::list(path).unwrap_or_default();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    for e in entries {
        win.rows.push((e.name.clone(), FKind::of(&e.name, e.is_dir)));
    }
    win.title = alloc::format!("Fayllar — {}", path);
}

/// Yöl birlaştiradi (yoki ".." — ota papka).
fn path_join(base: &str, name: &str) -> String {
    if name == ".." {
        match base.rsplit_once('/') {
            Some((p, _)) if !p.is_empty() => String::from(p),
            _ => String::from(if base.starts_with('~') { "~" } else { "/" }),
        }
    } else if base.ends_with('/') {
        alloc::format!("{}{}", base, name)
    } else {
        alloc::format!("{}/{}", base, name)
    }
}

/// Fayllar oynasida qator bosilganda: papka — kiradi; fayl — mos ilovada oçadi.
fn files_activate(windows: &mut Vec<Window>, wi: usize, row: usize, opened: &mut usize) {
    let (path, kind) = {
        let w = &windows[wi];
        let Some((name, kind)) = w.rows.get(row) else { return };
        (path_join(&w.path, name), *kind)
    };
    if kind == FKind::Dir {
        load_dir(&mut windows[wi], &path);
        return;
    }
    open_file(windows, &path, *opened);
    *opened += 1;
}

/// Faylni öqib, turiga qarab mos oynada oçadi.
fn open_file(windows: &mut Vec<Window>, path: &str, n: usize) {
    let (sw, sh) = gfx::size();
    let u = body();
    let data = fs::read_all(path).unwrap_or_default();
    let name = path.rsplit('/').next().unwrap_or(path);
    let x = sw / 6 + (n * u * 2) % (sw / 3);
    let y = menubar_h() + u * 2 + (n * u) % (sh / 4);
    // Övozmi?
    if crate::audio::is_audio_ext(name) || crate::audio::format_name(&data).is_some() {
        let (w, h) = (u * 30, u * 16);
        let mut win = base(x.min(sw - w), y, w, h, Kind::Audio, alloc::format!("Övoz — {}", name), Vec::new());
        win.path = String::from(path);
        if let Some(clip) = crate::audio::decode(name, &data) {
            win.env = clip.envelope(u * 24);
            win.clip = Some(clip);
        } else {
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
            win.lines = alloc::vec![alloc::format!("{} formati hali qöllanmaydi", ext), String::from("Qöllanadi: WAV, AIFF")];
        }
        windows.push(win);
        return;
    }
    // Rasmmi? (tur yoki sarlavha baytidan)
    let is_img = FKind::of(name, false) == FKind::Image || crate::image::format_name(&data).is_some();
    if is_img {
        let (w, h) = (u * 28, u * 22);
        let mut win = base(x.min(sw - w), y, w, h, Kind::Image, alloc::format!("Rasm — {}", name), Vec::new());
        win.path = String::from(path);
        // Harakatli rasm (GIF/APNG)?
        if let Some(frames) = crate::image::decode_frames(name, &data).filter(|f| f.len() > 1) {
            win.title = alloc::format!("Rasm — {} ({} kadr)", name, frames.len());
            win.frames = frames;
            win.frame_ms = crate::timer::uptime_ms();
        } else {
            win.img = crate::image::decode_file(name, &data);
        }
        if win.img.is_none() && win.frames.is_empty() {
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
            win.lines = alloc::vec![
                alloc::format!("{} formati hali qöllanmaydi", ext),
                String::new(),
                String::from("Qöllanadi: PNG, JPEG, GIF, BMP, TGA,"),
                String::from("PCX, ICO, QOI, PNM (PBM/PGM/PPM)"),
            ];
        }
        windows.push(win);
        return;
    }
    // ELF dastur?
    if data.starts_with(b"\x7fELF") {
        let (w, h) = (u * 24, u * 10);
        let mut win = base(x.min(sw - w), y, w, h, Kind::About, alloc::format!("Dastur — {}", name), Vec::new());
        win.lines = alloc::vec![
            alloc::format!("{}", name),
            alloc::format!("ELF dasturi · {} bayt", data.len()),
            String::new(),
            String::from("Terminalda işga tuşiring: işlat " ) + name,
        ];
        windows.push(win);
        return;
    }
    // Matn sifatida oçamiz (muharrirda tahrirlanadi)
    let mut lines: Vec<String> = String::from_utf8_lossy(&data).lines().map(String::from).collect();
    let input = lines.pop().unwrap_or_default();
    let (w, h) = (u * 29, u * 20);
    let mut win = base(x.min(sw - w), y, w, h, Kind::Editor, alloc::format!("Matn — {}", name), lines);
    win.path = String::from(path);
    win.input = input;
    windows.push(win);
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
    let lh = gfx::line_height(small(), Font::Mono).max(1);
    win.h.saturating_sub(title_h() + body()) / lh
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
    let (_sw, sh) = gfx::size();
    set_metrics(sh); // ekran ölçamiga moslaşamiz
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
    gradient(DESK_TOP, DESK_BOT);
    gfx::draw_text_centered(w / 2, h / 2 - body() * 3, "Zamin OS", OQ, body() * 3);
    gfx::draw_text_centered(w / 2, h / 2 + body(), "Birinçi sozlaş serial konsulda ketyapti…", KULRANG, body());
    gfx::flush();
}

/// Kiriş ekrani geometriyasi: karta va içki elementlar.
struct LoginGeom {
    cx: usize,
    cy: usize,
    cw: usize,
    ch: usize,
    avatar: usize, // markaz y
    rows_y: usize,
    row_h: usize,
    field_x: usize,
    field_y: usize,
    field_w: usize,
    field_h: usize,
    btn_x: usize,
    btn_y: usize,
    btn_w: usize,
    btn_h: usize,
}

fn login_geom(multi: bool) -> LoginGeom {
    let (sw, sh) = gfx::size();
    let u = body();
    let cw = (sw / 3).clamp(u * 20, u * 32);
    let ch = if multi { u * 20 } else { u * 17 };
    let cx = (sw - cw) / 2;
    let cy = (sh - ch) / 2;
    let field_w = cw - u * 5;
    let field_h = u * 2;
    let btn_w = cw * 2 / 5;
    let btn_h = u * 2;
    LoginGeom {
        cx, cy, cw, ch,
        avatar: cy + u * 4,
        rows_y: cy + u * 9,
        row_h: u * 2,
        field_x: cx + (cw - field_w) / 2,
        field_y: cy + ch - u * 6,
        field_w, field_h,
        btn_x: cx + (cw - btn_w) / 2,
        btn_y: cy + ch - u * 5 / 2,
        btn_w, btn_h,
    }
}

/// Grafik kiriş ekrani: foydalanuvçini tanlab, parol kiritadi.
fn login_screen() {
    let (sw, sh) = gfx::size();
    let accts = accounts::all();
    let multi = accts.len() > 1;
    let mut sel = 0usize;
    let mut pass = String::new();
    let mut err = false;
    let mut shift = false;
    let mut cx = sw / 2;
    let mut cy = sh / 2;

    loop {
        let g = login_geom(multi);
        let mut submit = false;
        for e in input::poll() {
            match (e.typ, e.code) {
                (input::EV_ABS, input::ABS_X) => cx = (e.value as usize * sw) / 32768,
                (input::EV_ABS, input::ABS_Y) => cy = (e.value as usize * sh) / 32768,
                (input::EV_KEY, input::BTN_LEFT) if e.value == 1 => {
                    if cx >= g.btn_x && cx < g.btn_x + g.btn_w && cy >= g.btn_y && cy < g.btn_y + g.btn_h {
                        submit = true;
                    }
                    if multi {
                        for i in 0..accts.len() {
                            let ry = g.rows_y + i * g.row_h;
                            if cy >= ry && cy < ry + g.row_h && cx >= g.cx + body() && cx < g.cx + g.cw - body() {
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
                return;
            } else {
                err = true;
                accounts::forget(core::mem::take(&mut pass));
                draw_login(&accts, sel, 0, err, cx, cy);
                gfx::flush();
                task::sleep_ms(500);
            }
        }

        draw_login(&accts, sel, pass.len(), err, cx, cy);
        gfx::flush();
        task::sleep_ms(16);
    }
}

fn draw_login(accts: &[accounts::Account], sel: usize, dots: usize, err: bool, cx: usize, cy: usize) {
    let (sw, _sh) = gfx::size();
    let u = body();
    let multi = accts.len() > 1;
    let g = login_geom(multi);
    gradient(DESK_TOP, DESK_BOT);
    gfx::draw_text_centered(sw / 2, g.cy - u * 3, "Zamin OS", OQ, u * 2);

    // Karta (soya + yumaloq shaffof panel)
    gfx::shadow(g.cx, g.cy, g.cw, g.ch, radius() * 2, 12);
    fill_round_rect_a(g.cx, g.cy, g.cw, g.ch, radius() * 2, 0x1a2538, 240);
    gfx::round_outline(g.cx, g.cy, g.cw, g.ch, radius() * 2, 1, 0x31405c);

    // Avatar (doira + bosh harf)
    let acct = &accts[sel];
    let ar = u * 2;
    gfx::fill_circle(sw / 2, g.avatar, ar, ZAMIN_KOK);
    let initial = acct.full_name.chars().next().unwrap_or('Z').to_uppercase().collect::<String>();
    gfx::draw_text_centered(sw / 2, g.avatar - ar * 2 / 3, &initial, OQ, ar * 4 / 3);
    gfx::draw_text_centered(sw / 2, g.avatar + ar + u / 2, &acct.full_name, OQ, u);
    gfx::draw_text_centered(sw / 2, g.avatar + ar + u * 3 / 2 + u / 4, &alloc::format!("@{}", acct.username), KULRANG, small());

    if multi {
        for (i, a) in accts.iter().enumerate() {
            let ry = g.rows_y + i * g.row_h;
            let active = i == sel;
            fill_round_rect(g.cx + u, ry, g.cw - u * 2, g.row_h - u / 4, g.row_h / 4, if active { ZAMIN_KOK } else { 0x263650 });
            gfx::draw_text(g.cx + u * 2, ry + (g.row_h.saturating_sub(small())) / 2, &a.username, OQ, small());
        }
    } else {
        // Parol maydoni
        let edge = if err { QIZIL } else { 0x3c5479 };
        fill_round_rect(g.field_x, g.field_y, g.field_w, g.field_h, g.field_h / 2, 0x0e1a2c);
        gfx::round_outline(g.field_x, g.field_y, g.field_w, g.field_h, g.field_h / 2, 2, edge);
        let dotr = (u / 4).max(3);
        let step = dotr * 3;
        let maxn = (g.field_w - u) / step;
        for i in 0..dots.min(maxn) {
            gfx::fill_circle(g.field_x + u + i * step + dotr, g.field_y + g.field_h / 2, dotr, OQ);
        }
        if dots == 0 {
            gfx::draw_text(g.field_x + u, g.field_y + (g.field_h.saturating_sub(small())) / 2, "parol…", KULRANG, small());
        }
    }

    if err {
        gfx::draw_text_centered(sw / 2, g.btn_y - u * 3 / 2, "Parol xato — qayta urining", QIZIL, small());
    }

    // "Kiriş" tugmasi
    fill_round_rect(g.btn_x, g.btn_y, g.btn_w, g.btn_h, g.btn_h / 2, ZAMIN_KOK);
    gfx::draw_text_centered(g.btn_x + g.btn_w / 2, g.btn_y + (g.btn_h.saturating_sub(small())) / 2, "Kiriş →", OQ, small());

    cursor(cx, cy);
}

/// Silliq siçqon kursori — mac uslubidagi koʻrsatkich (oq, qora chegara).
fn cursor(cx: usize, cy: usize) {
    let f = (body() as f32 * 1.3 / 16.0).max(1.0); // masştab (16-birlik shakl)
    // Standart ko'rsatkich köpburçagi (uç nuqtasi — (0,0))
    const ARROW: [(f32, f32); 7] = [
        (0.0, 0.0),
        (0.0, 12.8),
        (3.3, 9.8),
        (5.3, 14.6),
        (7.1, 13.8),
        (5.1, 9.2),
        (8.6, 9.2),
    ];
    let (ox, oy) = (cx as f32, cy as f32);
    let outer: Vec<(f32, f32)> = ARROW.iter().map(|&(x, y)| (ox + x * f, oy + y * f)).collect();
    // Markaz (ichki oq uçun kiçraytiramiz)
    let cxm = outer.iter().map(|p| p.0).sum::<f32>() / 7.0;
    let cym = outer.iter().map(|p| p.1).sum::<f32>() / 7.0;
    let inner: Vec<(f32, f32)> =
        outer.iter().map(|&(x, y)| (cxm + (x - cxm) * 0.78, cym + (y - cym) * 0.78)).collect();
    gfx::fill_poly(&outer, QORA); // chegara
    gfx::fill_poly(&inner, OQ); // ichi
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
    let mut last_sec = time::now().second;

    draw(&windows, cx, cy);
    gfx::flush();

    loop {
        let mut changed = false;
        // Soat oynasi oçiq bölsa — har soniyada yangilaymiz
        let sec = time::now().second;
        if sec != last_sec {
            last_sec = sec;
            if windows.iter().any(|w| w.kind == Kind::Clock) {
                changed = true;
            }
        }
        // Harakatli rasm (GIF) kadrlarini vaqtiga qarab almaştiramiz
        let now_ms = crate::timer::uptime_ms();
        for win in windows.iter_mut() {
            if win.frames.len() > 1 {
                let delay = win.frames[win.frame].1.max(20) as u64;
                if now_ms.wrapping_sub(win.frame_ms) >= delay {
                    win.frame = (win.frame + 1) % win.frames.len();
                    win.frame_ms = now_ms;
                    changed = true;
                }
            }
            // Övoz ijrosi bosh (playhead) vaqtga qarab oldinga suriladi
            if win.playing {
                if let Some(dur) = win.clip.as_ref().map(|c| c.duration_ms()) {
                    let dt = now_ms.wrapping_sub(win.play_t) as u32;
                    win.play_t = now_ms;
                    win.play_pos = win.play_pos.saturating_add(dt);
                    if win.play_pos >= dur {
                        win.play_pos = dur;
                        win.playing = false;
                        audio_sync(win);
                    }
                    changed = true;
                }
            }
        }
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
                    if let Some(w) = windows.last_mut().filter(|w| w.kind.takes_keys()) {
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
                    windows[idx].y = ny.clamp(menubar_h(), sh.saturating_sub(dock_h() + 20));
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
    if w.kind == Kind::Editor {
        match code {
            28 => w.lines.push(core::mem::take(&mut w.input)), // Enter: yangi qator
            14 => {
                // Orqaga oçiriş: qator boşida bölsa avvalgi qatorga qoşilamiz
                if w.input.is_empty() {
                    if let Some(prev) = w.lines.pop() {
                        w.input = prev;
                    }
                } else {
                    w.input.pop();
                }
            }
            _ => {
                if let Some(ch) = keycode_char(code, shift) {
                    if w.input.len() < 200 {
                        w.input.push(ch);
                    }
                }
            }
        }
        return;
    }
    // Terminal
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

/// Matn muharriri mazmunini `~/qaydlar.txt` ga saqlaydi. Natija — xabar.
fn save_notes(w: &Window) -> String {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    let mut text = w.lines.join("\n");
    if !w.lines.is_empty() {
        text.push('\n');
    }
    text.push_str(&w.input);
    text.push('\n');
    let path = if w.path.is_empty() { NOTES_PATH } else { w.path.as_str() };
    match fs::open(path, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE) {
        Ok(id) => match fs::write(id, Some(0), text.as_bytes()) {
            Ok(n) => alloc::format!("saqlandi: {} bayt → {}", n, path),
            Err(e) => alloc::format!("xato: {:?}", e),
        },
        Err(e) => alloc::format!("oçib bölmadi: {:?}", e),
    }
}

fn handle_click(windows: &mut Vec<Window>, cx: usize, cy: usize, sh: usize, drag: &mut Option<(usize, usize)>, opened: &mut usize) {
    // Dok (pastdagi markazlashgan panel)?
    let (sw, _) = gfx::size();
    let (_, dy, _, _, icon, _) = dock_geom(sw, sh);
    if cy >= dy {
        for (i, l) in LAUNCHERS.iter().enumerate() {
            let (ix, iy, isz) = dock_icon_rect(sw, sh, i);
            let _ = icon;
            if cx >= ix && cx < ix + isz && cy >= iy && cy < iy + isz {
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
        if windows[i].kind == Kind::Editor && windows[i].in_save_btn(cx, cy) {
            let msg = save_notes(&windows[i]);
            windows[i].cur = msg.into_bytes(); // holat xabari
            return;
        }
        if windows[i].kind == Kind::Audio && windows[i].clip.is_some() && windows[i].in_play_btn(cx, cy) {
            let w = &mut windows[i];
            w.playing = !w.playing;
            w.play_t = crate::timer::uptime_ms();
            if w.play_pos >= w.clip.as_ref().map(|c| c.duration_ms()).unwrap_or(0) {
                w.play_pos = 0; // oxirida — boşidan
            }
            audio_sync(w);
            return;
        }
        if windows[i].title_bar(cx, cy) {
            let off = (cx - windows[i].x) | ((cy - windows[i].y) << 16);
            let w = windows.remove(i);
            windows.push(w); // old planga
            *drag = Some((windows.len() - 1, off));
            return;
        }
        // Fayllar oynasida qator bosiş: papka — kiradi; fayl — oçadi
        if windows[i].kind == Kind::Files {
            if let Some(row) = file_row_at(&windows[i], cx, cy) {
                // Oynani old planga olib, keyin faollaştiramiz
                let w = windows.remove(i);
                windows.push(w);
                let wi = windows.len() - 1;
                files_activate(windows, wi, row, opened);
                return;
            }
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
    gfx::reset_clip();
    // Fon: yumşoq koʻp bosqiçli gradient (indigo → koʻk → tün)
    let mid = h * 9 / 20;
    gradient_rect(0, 0, w, mid, 0x24406e, 0x172a4a);
    gradient_rect(0, mid, w, h - mid, 0x172a4a, 0x0a1320);

    draw_menubar(w);

    for (i, win) in windows.iter().enumerate() {
        let active = i == windows.len() - 1;
        draw_window(win, active);
    }
    gfx::reset_clip();

    draw_dock(w, h, cx, cy, windows);
    cursor(cx, cy);
}

/// Yuqori menyu paneli (shaffof).
fn draw_menubar(w: usize) {
    let mb = menubar_h();
    fill_round_rect_a(0, 0, w, mb, 0, 0x0b1424, 170);
    fill_rect(0, mb - 1, w, 1, 0x1f2c42);
    let ty = (mb.saturating_sub(small())) / 2;
    gfx::draw_text(body(), ty, "Zamin OS", 0xf2f6fc, small());
    let user = accounts::current().map(|a| a.username).unwrap_or_default();
    let t = time::now();
    let right = alloc::format!("{}     {:02}:{:02}", user, t.hour, t.minute);
    gfx::draw_text(w.saturating_sub(gfx::text_width(&right, small()) + body()), ty, &right, 0xd4deee, small());
}

/// Bitta oynani so'nggi mac uslubida chizadi (yumşoq soya, shaffof tana,
/// birlaşgan sarlavha, svetofor nuqtalari). Mazmun oyna içiga qirqiladi.
fn draw_window(win: &Window, active: bool) {
    gfx::reset_clip();
    let r = radius();
    let th = title_h();
    // Yumşoq soya
    gfx::shadow(win.x, win.y, win.w, win.h, r, if active { 14 } else { 7 });
    // Tana (toza, shaffofmas)
    fill_round_rect(win.x, win.y, win.w, win.h, r, if active { WIN_BODY } else { 0x191e29 });
    // Birlaşgan sarlavha uçun biroz oçroq fon (yuqori burçaklar yumaloq)
    let tcol = if active { WIN_TITLE } else { 0x1d2430 };
    fill_round_rect(win.x, win.y, win.w, th, r, tcol);
    fill_rect(win.x, win.y + th - r, win.w, r, tcol);
    fill_rect(win.x, win.y + th, win.w, 1, 0x0f131c);
    // Yuqori ichki yorugʻ chiziq (shisha qirrasi)
    fill_round_rect_a(win.x + r, win.y, win.w - 2 * r, 1, 0, 0xffffff, 30);
    // Hairline chegara
    gfx::round_outline(win.x, win.y, win.w, win.h, r, 1, if active { 0x3a4458 } else { 0x2a3040 });
    // Svetofor nuqtalari (çap)
    let (dx, dy, dr) = win.close_dot();
    let step = dr * 5 / 2;
    let dim = |c: Color| if active { c } else { 0x4a5366 };
    gfx::fill_circle(dx, dy, dr, dim(0xff5f57));
    gfx::fill_circle(dx + step, dy, dr, dim(0xfebc2e));
    gfx::fill_circle(dx + step * 2, dy, dr, dim(0x28c840));
    // Sarlavha matni (markazda, nuqtalardan qoçib qirqiladi)
    let tl = win.x + th * 2;
    let tw = win.w.saturating_sub(th * 3);
    gfx::set_clip(tl, win.y, tw, th);
    let ty = win.y + (th.saturating_sub(small())) / 2;
    gfx::draw_text_centered(win.x + win.w / 2, ty, &win.title, if active { 0xeef3fa } else { 0x8696ac }, small());
    gfx::reset_clip();

    // Mazmun — oyna içiga qirqiladi
    gfx::set_clip(win.x + 1, win.y + th, win.w.saturating_sub(2), win.h.saturating_sub(th + 1));
    match win.kind {
        Kind::Clock => draw_clock(win),
        Kind::Image if win.img.is_some() || !win.frames.is_empty() => draw_image(win),
        Kind::Image if win.path.is_empty() => draw_scene(win), // dok "Rasm" — namuna manzara
        Kind::Image => draw_text_window(win),                  // oçib bölmagan rasm — xabar
        Kind::Audio if win.clip.is_some() => draw_audio(win),
        Kind::Audio => draw_text_window(win),
        Kind::Editor => draw_editor(win),
        Kind::Files => draw_files(win),
        _ => draw_text_window(win),
    }
    gfx::reset_clip();
}

/// Fayl qatori balandligi.
fn file_rh() -> usize {
    body() * 9 / 5
}

/// Bosilgan nuqtadagi fayl qatori indeksi (Fayllar oynasi).
fn file_row_at(win: &Window, cx: usize, cy: usize) -> Option<usize> {
    let top = win.content_top();
    if cx < win.x || cx >= win.x + win.w || cy < top || cy >= win.y + win.h - body() / 2 {
        return None;
    }
    let i = (cy - top) / file_rh();
    if i < win.rows.len() { Some(i) } else { None }
}

/// Fayllar oynasi: ikona + nom qatorlari.
fn draw_files(win: &Window) {
    let rh = file_rh();
    let pad = body();
    let isz = body() * 5 / 4;
    let mut y = win.content_top();
    for (name, kind) in &win.rows {
        if y + rh > win.y + win.h - body() / 2 {
            break;
        }
        // Ikona
        file_icon(*kind, win.x + pad, y + (rh - isz) / 2, isz);
        // Nom
        let ty = y + (rh.saturating_sub(small())) / 2;
        let label = if *kind == FKind::Dir && name != ".." {
            alloc::format!("{}/", name)
        } else {
            name.clone()
        };
        gfx::draw_text(win.x + pad * 2 + isz, ty, &label, 0xe7edf6, small());
        y += rh;
    }
}

/// Fayl turi ikonasi (kiçik).
fn file_icon(kind: FKind, x: usize, y: usize, s: usize) {
    let c = kind.color();
    if kind == FKind::Dir {
        // Papka
        gfx::fill_round_rect(x, y + s / 5, s * 2 / 5, s / 5, s / 12, c);
        gfx::fill_round_rect(x, y + s / 3, s, s * 3 / 5, s / 8, c);
        return;
    }
    // Hujjat varaqi + burchak + tur belgisi
    gfx::fill_round_rect(x + s / 6, y, s * 2 / 3, s, s / 10, 0x2c3345);
    gfx::round_outline(x + s / 6, y, s * 2 / 3, s, s / 10, 1, c);
    let letter = match kind {
        FKind::Image => "▦",
        FKind::Text => "¶",
        FKind::Program => ">",
        FKind::Audio => "♪",
        FKind::Video => "▶",
        FKind::Archive => "▣",
        _ => "·",
    };
    let _ = letter; // belgilar shriftda bölmasligi mumkin — rangli chiziq bilan belgilaymiz
    gfx::fill_round_rect(x + s / 4, y + s / 6, s / 3, s / 8, s / 16, c);
    gfx::fill_rect(x + s / 4, y + s / 2, s * 5 / 12, (s / 10).max(1), c);
    gfx::fill_rect(x + s / 4, y + s * 2 / 3, s / 3, (s / 10).max(1), c);
}

/// Rasm oynasi: dekodlangan rasmni (yoki harakatli kadrni) nisbat saqlab körsatadi.
fn draw_image(win: &Window) {
    let img = if !win.frames.is_empty() {
        &win.frames[win.frame.min(win.frames.len() - 1)].0
    } else if let Some(img) = &win.img {
        img
    } else {
        return;
    };
    let pad = body() / 2;
    let x = win.x + pad;
    let y = win.content_top();
    let w = win.w.saturating_sub(pad * 2);
    let h = (win.y + win.h).saturating_sub(y + pad + small() + pad);
    // Şaxmat fon (shaffoflik hissi)
    gfx::fill_rect(x, y, w, h, 0x11151d);
    let (ix, iy, iw, ih) = gfx::blit_fit(x, y, w, h, img.w, img.h, &img.px);
    gfx::round_outline(ix.saturating_sub(1), iy.saturating_sub(1), iw + 2, ih + 2, 1, 1, 0x3a4458);
    // Maʼlumot satri (ölçam)
    let info = alloc::format!("{}×{} · {} piksel", img.w, img.h, img.w * img.h);
    gfx::draw_text(x, win.y + win.h - pad - small(), &info, 0x8aa0bc, small());
}

/// Terminal/Fayllar/Haqida mazmunini chizadi.
fn draw_text_window(win: &Window) {
    let mono = win.kind == Kind::Terminal;
    let px = small();
    let lh = gfx::line_height(px, if mono { Font::Mono } else { Font::Ui }).max(px + 3);
    let fg = if mono { 0x46d98a } else { 0xe7edf6 };
    let pad = body();
    let mut ty = win.content_top();
    let draw_line = |x: usize, y: usize, s: &str, c: Color| {
        if mono { gfx::draw_text_mono(x, y, s, c, px) } else { gfx::draw_text(x, y, s, c, px) };
    };
    for line in &win.lines {
        if ty + lh > win.y + win.h - pad / 2 {
            break;
        }
        draw_line(win.x + pad, ty, line, fg);
        ty += lh;
    }
    if win.kind == Kind::Terminal && ty + lh <= win.y + win.h - pad / 2 {
        let cur = String::from_utf8_lossy(&win.cur);
        draw_line(win.x + pad, ty, &alloc::format!("{}{}", cur, win.input), fg);
        // Kursor çizigʻi
        let cwdt = gfx::text_width_mono(&alloc::format!("{}{}", cur, win.input), px);
        fill_rect(win.x + pad + cwdt + 1, ty, (px / 10).max(1), px, 0x46d98a);
    }
}

/// Soat oynasi: katta vaqt va sana.
fn draw_clock(win: &Window) {
    let t = time::now();
    let cx = win.x + win.w / 2;
    let avail = (win.y + win.h).saturating_sub(win.content_top());
    let big = (avail / 3).clamp(28, 96);
    let vaqt = alloc::format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
    let cy0 = win.content_top() + avail / 2;
    gfx::draw_text_centered(cx, cy0 - big, &vaqt, 0xf4f8ff, big);
    let sana = alloc::format!("{:04}-{:02}-{:02}", t.year, t.month, t.day);
    gfx::draw_text_centered(cx, cy0 + big / 3, &sana, 0x7fb0ea, body());
}

/// Matn muharriri: asboblar qatori (Saqla), matn, kursor, holat.
fn draw_editor(win: &Window) {
    let px = small();
    let lh = gfx::line_height(px, Font::Mono).max(px + 3);
    // Asboblar qatori
    let (bx, by, bw, bh) = win.save_btn();
    gfx::fill_round_rect(bx, by, bw, bh, bh / 2, 0x30c85a);
    let bty = by + (bh.saturating_sub(small())) / 2;
    gfx::draw_text_centered(bx + bw / 2, bty, "Saqla", 0x06230f, small());
    let status = String::from_utf8_lossy(&win.cur);
    if !status.is_empty() {
        gfx::draw_text(win.x + body(), by + (bh.saturating_sub(small())) / 2, &status, 0x8aa0bc, small());
    }
    // Matn
    let pad = body();
    let mut ty = by + bh + pad / 2;
    for line in &win.lines {
        if ty + lh > win.y + win.h - pad / 2 {
            break;
        }
        gfx::draw_text_mono(win.x + pad, ty, line, 0xe7edf6, px);
        ty += lh;
    }
    if ty + lh <= win.y + win.h - pad / 2 {
        let w = gfx::text_width_mono(&win.input, px);
        gfx::draw_text_mono(win.x + pad, ty, &win.input, 0xe7edf6, px);
        fill_rect(win.x + pad + w + 1, ty, (px / 10).max(1), px, 0x7fb0ea);
    }
}

/// Rasm köruvçi: namunaviy "Zamin" manzarasi (osmon, quyoş, togʻlar).
fn draw_scene(win: &Window) {
    let (x, y) = (win.x + 1, win.y + title_h() + 1);
    let (w, h) = (win.w - 2, (win.y + win.h).saturating_sub(y + 1));
    // Osmon gradienti (quyoş botişi)
    gfx::gradient_rect(x, y, w, h * 5 / 6, 0x2b4a8c, 0xf0b36a);
    // Quyoş
    gfx::fill_circle(x + w * 3 / 4, y + h / 3, (h / 7).max(10), 0xfff1c0);
    gfx::fill_circle(x + w * 3 / 4, y + h / 3, (h / 9).max(7), SARIQ);
    // Togʻlar
    hill(x, y, w, h, w / 3, h * 2 / 3, 0x2a4740);
    hill(x, y, w, h, w * 2 / 3, h / 2, 0x1f5141);
    // Yer
    fill_rect(x, y + h * 5 / 6, w, h - h * 5 / 6, 0x143026);
}

/// Ijro holatini övoz qurilmasi bilan moslaştiradi (qurilma bölsa chaladi).
fn audio_sync(win: &Window) {
    if let Some(clip) = &win.clip {
        if win.playing {
            crate::audio::sound::play(&clip.samples, clip.rate, clip.channels, win.play_pos);
        } else {
            crate::audio::sound::stop();
        }
    }
}

/// Övoz oynasi: toʻlqin shakli, maʼlumot, ijro tugmasi, progress.
fn draw_audio(win: &Window) {
    let Some(clip) = &win.clip else { return };
    let pad = body();
    let x = win.x + pad;
    let y = win.content_top();
    let w = win.w.saturating_sub(pad * 2);
    // Maʼlumot
    let dur = clip.duration_ms();
    let info = alloc::format!(
        "{} Hz · {} kanal · {}:{:02}",
        clip.rate,
        clip.channels,
        dur / 60000,
        (dur / 1000) % 60
    );
    gfx::draw_text(x, y, &info, 0x9fb0c3, small());
    // Toʻlqin
    let wy = y + body() * 2;
    let wh = win.h.saturating_sub((wy - win.y) + body() * 4);
    let mid = wy + wh / 2;
    gfx::fill_rect(x, mid, w, 1, 0x2a3446);
    let n = win.env.len().max(1);
    for (i, &(mn, mx)) in win.env.iter().enumerate() {
        let bx = x + i * w / n;
        let top = mid as i32 - (mx as i32 * (wh as i32 / 2) / 32768);
        let bot = mid as i32 - (mn as i32 * (wh as i32 / 2) / 32768);
        let (t, b) = (top.min(bot).max(wy as i32), top.max(bot).min((wy + wh) as i32));
        if b > t {
            gfx::fill_rect(bx, t as usize, (w / n).max(1), (b - t) as usize, 0x46d98a);
        }
    }
    // Ijro bosh (playhead)
    if dur > 0 {
        let hx = x + (win.play_pos.min(dur) as usize * w / dur as usize);
        gfx::fill_rect(hx, wy, 2, wh, 0xffd426);
    }
    // Ijro tugmasi
    let (px, py, r) = win.play_btn();
    gfx::fill_circle(px, py, r, if win.playing { 0xffd426 } else { ZAMIN_KOK });
    if win.playing {
        // pauza — ikki tayoqça
        fill_rect(px - r / 3, py - r / 2, r / 4, r, 0x06230f);
        fill_rect(px + r / 8, py - r / 2, r / 4, r, 0x06230f);
    } else {
        // uçburçak (play)
        for i in 0..r {
            let hh = (r - i) * 2 / 3;
            fill_rect(px - r / 3 + i / 2, py.saturating_sub(hh / 2), 1, hh.max(1), OQ);
        }
    }
    // Vaqt
    let pos = win.play_pos.min(dur);
    let tt = alloc::format!("{}:{:02} / {}:{:02}", pos / 60000, (pos / 1000) % 60, dur / 60000, (dur / 1000) % 60);
    gfx::draw_text(px + r + pad, py.saturating_sub(small() / 2), &tt, 0xe7edf6, small());
}

/// Dok: markazlashgan shaffof panel + ilova ikonalari (siçqon tagida kattaroq).
fn draw_dock(sw: usize, sh: usize, cx: usize, cy: usize, windows: &[Window]) {
    let (dx, dy, dw, dh, _icon, _gap) = dock_geom(sw, sh);
    let dr = dh * 3 / 8;
    gfx::shadow(dx, dy, dw, dh, dr, 8);
    fill_round_rect_a(dx, dy, dw, dh, dr, 0x1b2436, 210);
    fill_round_rect_a(dx + dr, dy, dw - 2 * dr, 1, 0, 0xffffff, 30); // yuqori yorugʻlik
    gfx::round_outline(dx, dy, dw, dh, dr, 1, 0x39435a);
    for (i, l) in LAUNCHERS.iter().enumerate() {
        let (ix, iy, isz) = dock_icon_rect(sw, sh, i);
        let hover = cx >= ix && cx < ix + isz && cy >= dy && cy < dy + dh;
        let open = windows.iter().any(|w| w.kind == l.kind);
        if hover {
            // Fon yoritgichi + nomi (tooltip)
            fill_round_rect_a(ix - isz / 8, iy - isz / 8, isz + isz / 4, isz + isz / 4, isz / 4, 0xffffff, 24);
            let lw = gfx::text_width(l.label, small());
            let tx = (ix + isz / 2).saturating_sub(lw / 2 + body() / 2);
            let tipw = lw + body();
            let tipy = dy.saturating_sub(small() + body());
            fill_round_rect_a(tx, tipy, tipw, small() + body() / 2, (small() + body() / 2) / 3, 0x0b1424, 230);
            gfx::draw_text_centered(ix + isz / 2, tipy + body() / 4, l.label, OQ, small());
        }
        draw_icon(l.kind, l.color, ix, iy, isz);
        if open {
            // Ishlayotgan ilova belgisi (nuqta)
            let d = (isz / 12).max(2);
            gfx::fill_circle(ix + isz / 2, dy + dh - d - 1, d, 0xcdd7e6);
        }
    }
}

/// Bitta ilova ikonasini chizadi (squircle plitka + belgisi).
fn draw_icon(kind: Kind, color: Color, x: usize, y: usize, s: usize) {
    let r = s * 3 / 10; // squircle
    // Plitka: rang + yuqori yorugʻlik + chegara
    fill_round_rect(x, y, s, s, r, color);
    gfx::fill_round_rect_a(x, y, s, s / 2, r, 0xffffff, 32);
    gfx::round_outline(x, y, s, s, r, 1, 0x000000);
    let c = x + s / 2;
    let m = s / 2;
    match kind {
        Kind::Terminal => {
            gfx::draw_text_mono(x + s / 5, y + s / 3, ">_", OQ, m * 4 / 5);
        }
        Kind::Files => {
            let fw = s * 11 / 20;
            let fh = s * 2 / 5;
            let fx = x + (s - fw) / 2;
            let fy = y + (s - fh) / 2 + s / 20;
            gfx::fill_round_rect(fx, fy - fh / 4, fw * 2 / 5, fh / 3, fh / 8, 0xffe9a8);
            gfx::fill_round_rect(fx, fy, fw, fh, fh / 5, 0xffcf5e);
        }
        Kind::Editor => {
            let dw = s * 2 / 5;
            let dh = s * 3 / 5;
            let dxp = x + (s - dw) / 2;
            let dyp = y + (s - dh) / 2;
            gfx::fill_round_rect(dxp, dyp, dw, dh, dw / 6, OQ);
            for k in 0..3 {
                fill_rect(dxp + dw / 5, dyp + dh / 4 + k * dh / 5, dw * 3 / 5, (s / 22).max(1), 0x8392a8);
            }
        }
        Kind::Clock => {
            let rr = s * 2 / 5;
            gfx::fill_circle(c, y + m, rr, OQ);
            gfx::fill_circle(c, y + m, rr * 17 / 20, 0x1b2740);
            fill_rect(c, y + m - rr / 2, (s / 20).max(1), rr / 2, OQ);
            fill_rect(c, y + m, rr * 3 / 5, (s / 20).max(1), OQ);
            gfx::fill_circle(c, y + m, (s / 20).max(2), 0xff6b5c);
        }
        Kind::Image => {
            let iw = s * 3 / 5;
            let ih = s * 7 / 15;
            let ix = x + (s - iw) / 2;
            let iy = y + (s - ih) / 2;
            gfx::gradient_rect(ix, iy, iw, ih, 0x8fd0ff, 0xffd59e);
            gfx::fill_circle(ix + iw * 3 / 4, iy + ih / 3, (ih / 5).max(2), 0xfff6d8);
            hill(ix, iy, iw, ih, iw / 2, ih * 3 / 5, 0x2e8b58);
        }
        Kind::About => {
            gfx::draw_text_centered(c, y + s / 6, "i", OQ, s * 2 / 3);
        }
        Kind::Audio => {
            // nota belgisi
            gfx::fill_circle(c - s / 6, y + s * 2 / 3, s / 10, OQ);
            fill_rect(c - s / 6 + s / 10, y + s / 4, (s / 20).max(1), s * 5 / 12, OQ);
        }
    }
}

/// Oddiy togʻ (uçburçak): (peak_x, peak_y) çöqqisi, poydevori pastda.
fn hill(bx: usize, by: usize, _bw: usize, bh: usize, peak_x: usize, peak_h: usize, color: Color) {
    let base_y = by + bh * 5 / 6;
    let peak_y = base_y.saturating_sub(peak_h);
    let span = peak_h; // 45° yonbagʻir
    for row in 0..=(base_y - peak_y) {
        let yy = peak_y + row;
        if yy >= by + bh {
            break;
        }
        let half = row.min(span);
        let left = (bx + peak_x).saturating_sub(half);
        fill_rect(left, yy, half * 2 + 1, 1, color);
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
