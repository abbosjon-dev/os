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
    Browser,
    Code,
    Video,
    Music,
    Settings,
    Trash,
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
    side: Vec<String>,  // Fayllar: yon panel papkalari (uy papkasidagi)
    thumbs: Vec<Option<crate::image::Image>>, // Fayllar: rasm eskizlari (rows bilan bir xil tartibda)
    id: usize,          // yagona raqam (animatsiya uçun)
    hidden: bool,       // dokka yashirilgan (sariq tugma)
    saved: Option<(usize, usize, usize, usize)>, // kattalashtirişdan oldingi (x, y, w, h)
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
    /// Svetofor tugmasi: 0 — yopiş, 1 — yashiriş, 2 — kattalashtiriş.
    fn dot_at(&self, cx: usize, cy: usize) -> Option<usize> {
        let (dx, dy, r) = self.close_dot();
        let step = r * 5 / 2;
        (0..3).find(|&k| {
            let (ddx, ddy) = (cx as isize - (dx + k * step) as isize, cy as isize - dy as isize);
            ((ddx * ddx + ddy * ddy) as usize) <= (r + 3) * (r + 3)
        })
    }
    /// Siçqon svetofor tugmalari ustidami (belgilarni körsatiş uçun)?
    fn over_dots(&self, cx: usize, cy: usize) -> bool {
        let (dx, dy, r) = self.close_dot();
        let step = r * 5 / 2;
        cx + r + 4 >= dx && cx <= dx + step * 2 + r + 4 && cy + r + 4 >= dy && cy <= dy + r + 4
    }
    /// Öng-pastki burçak — ölçamni özgartiriş tutqiçi.
    fn on_grip(&self, cx: usize, cy: usize) -> bool {
        let g = body();
        cx + g >= self.x + self.w && cx < self.x + self.w + 3 && cy + g >= self.y + self.h && cy < self.y + self.h + 3
    }
    #[allow(dead_code)]
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
    /// Pleer geometriyasi: (muqova ölçami, öng qism x, öng qism kengligi, mazmun boşi).
    fn audio_geo(&self) -> (usize, usize, usize, usize) {
        let u = body();
        let ct = self.y + title_h() + u;
        let a = (self.h.saturating_sub(title_h() + u * 2)).min(self.w / 3);
        let x0 = self.x + u + a + u;
        (a, x0, (self.x + self.w).saturating_sub(x0 + u), ct)
    }
    fn play_btn(&self) -> (usize, usize, usize) {
        let (a, x0, _, ct) = self.audio_geo();
        let r = body() * 6 / 5;
        (x0 + r, ct + a - r, r)
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

const LAUNCHERS: [Launcher; 10] = [
    Launcher { label: "Zamin", kind: Kind::About, color: 0x34c759 },
    Launcher { label: "Brauzer", kind: Kind::Browser, color: 0x1e88e5 },
    Launcher { label: "Fayllar", kind: Kind::Files, color: 0x2f88ff },
    Launcher { label: "Terminal", kind: Kind::Terminal, color: 0x2b3440 },
    Launcher { label: "Kod", kind: Kind::Code, color: 0x7c5cff },
    Launcher { label: "Video", kind: Kind::Video, color: 0x8257ff },
    Launcher { label: "Musiqa", kind: Kind::Music, color: 0xff7a2f },
    Launcher { label: "Rasmlar", kind: Kind::Image, color: 0x3a9bff },
    Launcher { label: "Sozlamalar", kind: Kind::Settings, color: 0x8a93a6 },
    Launcher { label: "Savat", kind: Kind::Trash, color: 0xcfd7e3 },
];

/// Oyna şu dok ilovasiga tegişlimi (oçiq ilova belgisi va qayta oçiş uçun)?
fn win_of_launcher(w: &Window, lk: Kind) -> bool {
    let is_video = crate::video::is_video_ext(&w.path);
    match lk {
        Kind::Music => w.kind == Kind::Audio,
        Kind::Video => w.kind == Kind::Image && is_video,
        Kind::Image => w.kind == Kind::Image && !is_video,
        Kind::Code => w.kind == Kind::Editor,
        k => w.kind == k,
    }
}

/// Kattalashtirilgan oyna maydoni (menyu va dok orasida).
fn zoom_rect() -> (usize, usize, usize, usize) {
    let (sw, sh) = gfx::size();
    let (_, dy, _, _, _, _) = dock_geom(sw, sh);
    let m = body() / 3;
    (m, menubar_h() + m, sw - 2 * m, dy.saturating_sub(menubar_h() + 3 * m))
}

const RESIZE: usize = 1 << 40; // drag ofsetidagi belgi: ölçam özgartiriş rejimi

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
        side: Vec::new(),
        thumbs: Vec::new(),
        id: NEXT_WIN_ID.fetch_add(1, Ordering::Relaxed),
        hidden: false,
        saved: None,
    }
}

static NEXT_WIN_ID: AtomicUsize = AtomicUsize::new(1);

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
            let (w, h) = (u * 38, u * 24);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Fayllar"), Vec::new());
            // Yon panel: uy papkasidagi papkalar
            let mut dirs: Vec<String> =
                fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir).map(|e| e.name).collect();
            dirs.sort();
            win.side = dirs;
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
            let (w, h) = (u * 24, u * 20);
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
        Kind::Browser => {
            let (w, h) = (u * 30, u * 22);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Zamin Brauzer"), alloc::vec![
                String::from("Zamin Brauzer"),
                String::new(),
                String::from("Veb brauzer tez orada qöşiladi."),
                String::from("Tarmoq: virtio-net + TLS tayyor."),
            ])
        }
        Kind::Settings => {
            let (w, h) = (u * 27, u * 18);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Sozlamalar"), alloc::vec![
                String::from("Sozlamalar"),
                String::new(),
                alloc::format!("Foydalanuvçi: {}", accounts::current().map(|a| a.username).unwrap_or_default()),
                alloc::format!("Versiya: {}", env!("CARGO_PKG_VERSION")),
                String::from("Til · Ekran · Övoz · Tarmoq"),
            ])
        }
        Kind::Trash => {
            let (w, h) = (u * 24, u * 13);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Savat"), alloc::vec![
                String::from("Savat böş"),
            ])
        }
        _ => {
            let (w, h) = (u * 25, u * 15);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Zamin OS"), Vec::new())
        }
    }
}

/// Fayllar oynasiga papka tarkibini yuklaydi (papkalar oldin, keyin alifbo).
fn load_dir(win: &mut Window, path: &str) {
    win.path = String::from(path);
    win.rows.clear();
    let mut entries = fs::list(path).unwrap_or_default();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    win.thumbs.clear();
    for e in entries {
        let kind = FKind::of(&e.name, e.is_dir);
        // Rasm bölsa — kiçik eskiz (bir marta dekodlanadi, keşlanadi)
        let thumb = if kind == FKind::Image {
            let fp = path_join(path, &e.name);
            fs::read_all(&fp)
                .ok()
                .filter(|d| d.len() < 4 * 1024 * 1024)
                .and_then(|d| crate::image::decode_file(&e.name, &d))
                .map(|img| make_thumb(&img, 128))
        } else {
            None
        };
        win.thumbs.push(thumb);
        win.rows.push((e.name.clone(), kind));
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
            win.env = clip.envelope(u * 5);
            win.clip = Some(clip);
        } else {
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
            win.lines = alloc::vec![alloc::format!("{} formati hali qöllanmaydi", ext), String::from("Qöllanadi: WAV, AIFF")];
        }
        windows.push(win);
        return;
    }
    // Videomi? (tur yoki konteyner sarlavhasidan)
    if crate::video::is_video_ext(name) || crate::video::format_name(&data).is_some() {
        let (w, h) = (u * 28, u * 22);
        let mut win = base(x.min(sw - w), y, w, h, Kind::Image, alloc::format!("Video — {}", name), Vec::new());
        win.path = String::from(path);
        if let Some(frames) = crate::video::decode_frames(&data).filter(|f| !f.is_empty()) {
            win.title = alloc::format!("Video — {} ({} kadr)", name, frames.len());
            win.frames = frames;
            win.frame_ms = crate::timer::uptime_ms();
        } else if let Some(vi) = crate::video::info(&data) {
            // taniladi, ammo dekod qilinmaydi — metama'lumotni körsatamiz
            win.lines = alloc::vec![
                alloc::format!("{} video — {}", vi.container, vi.codec),
                alloc::format!("öl'çam: {}x{}", vi.w, vi.h),
                alloc::format!("davomiylik: {}.{} s", vi.duration_ms / 1000, (vi.duration_ms % 1000) / 100),
                String::new(),
                String::from("Dekod qilinadi: MJPEG/AVI"),
                String::from("H.264/HEVC/VP9/AV1 — taniladi"),
            ];
        } else {
            win.lines = alloc::vec![String::from("Video oçib bölmadi")];
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
    // Qulf ekrani foni: ish stoli rasmi, bir marta xiralashtirilgan
    let bg = crate::image::decode(crate::wallpaper::JPG).map(|mut im| {
        for p in im.px.iter_mut() {
            let (r, g, b) = ((*p >> 16) & 0xff, (*p >> 8) & 0xff, *p & 0xff);
            *p = ((r * 9 / 20) << 16) | ((g * 9 / 20) << 8) | (b * 10 / 20);
        }
        im
    });

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
                draw_login(&accts, sel, 0, err, cx, cy, &bg);
                gfx::flush();
                task::sleep_ms(500);
            }
        }

        draw_login(&accts, sel, pass.len(), err, cx, cy, &bg);
        gfx::flush();
        task::sleep_ms(16);
    }
}

fn draw_login(
    accts: &[accounts::Account],
    sel: usize,
    dots: usize,
    err: bool,
    cx: usize,
    cy: usize,
    bg: &Option<crate::image::Image>,
) {
    let (sw, sh) = gfx::size();
    let u = body();
    let multi = accts.len() > 1;
    let g = login_geom(multi);
    if let Some(img) = bg {
        gfx::blit_scaled(0, 0, sw, sh, img.w, img.h, &img.px);
    } else {
        gradient(DESK_TOP, DESK_BOT);
    }
    // Katta soat va sana (qulf ekrani)
    let t = time::now();
    let big = (u * 4).min(g.cy / 2);
    gfx::draw_text_centered(sw / 2, g.cy.saturating_sub(big + u * 3), &alloc::format!("{:02}:{:02}", t.hour, t.minute), OQ, big);
    let full = ["Duşanba", "Seşanba", "Çorşanba", "Payşanba", "Juma", "Şanba", "Yakşanba"][weekday_index(t.year, t.month, t.day)];
    gfx::draw_text_centered(sw / 2, g.cy.saturating_sub(u * 5 / 2), &alloc::format!("{}, {}-{}", full, t.day, month_name(t.month)), 0xdbe3ee, small() * 6 / 5);

    // Karta (soya + shishasimon panel)
    gfx::shadow(g.cx, g.cy, g.cw, g.ch, radius() * 2, 12);
    fill_round_rect_a(g.cx, g.cy, g.cw, g.ch, radius() * 2, 0x0f1622, 200);
    gfx::round_outline(g.cx, g.cy, g.cw, g.ch, radius() * 2, 1, 0x3a4a66);

    // Avatar (doira + bosh harf)
    let acct = &accts[sel];
    let ar = u * 2;
    gfx::fill_circle(sw / 2, g.avatar, ar + 2, 0x58e06a);
    gfx::fill_circle(sw / 2, g.avatar, ar, 0x1f8a46);
    let initial = acct.full_name.chars().next().unwrap_or('Z').to_uppercase().collect::<String>();
    gfx::draw_text_centered(sw / 2, g.avatar - ar * 2 / 3, &initial, OQ, ar * 4 / 3);
    gfx::draw_text_centered(sw / 2, g.avatar + ar + u / 2, &acct.full_name, OQ, u);
    gfx::draw_text_centered(sw / 2, g.avatar + ar + u * 3 / 2 + u / 4, &alloc::format!("@{}", acct.username), KULRANG, small());

    if multi {
        for (i, a) in accts.iter().enumerate() {
            let ry = g.rows_y + i * g.row_h;
            let active = i == sel;
            fill_round_rect(g.cx + u, ry, g.cw - u * 2, g.row_h - u / 4, g.row_h / 4, if active { 0x1f8a46 } else { 0x263650 });
            gfx::draw_text(g.cx + u * 2, ry + (g.row_h.saturating_sub(small())) / 2, &a.username, OQ, small());
        }
    } else {
        // Parol maydoni
        let edge = if err { QIZIL } else { 0x4a5d80 };
        fill_round_rect_a(g.field_x, g.field_y, g.field_w, g.field_h, g.field_h / 2, 0x0a0f18, 210);
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

    // "Kiriş" tugmasi (yashil + chevron)
    fill_round_rect(g.btn_x, g.btn_y, g.btn_w, g.btn_h, g.btn_h / 2, 0x2fbf5f);
    let lw = gfx::text_width("Kiriş", small());
    let lx = g.btn_x + (g.btn_w.saturating_sub(lw + u)) / 2;
    gfx::draw_text(lx, g.btn_y + (g.btn_h.saturating_sub(small())) / 2, "Kiriş", 0x06230f, small());
    let (ax, ay) = ((lx + lw + u / 2) as f32, (g.btn_y + g.btn_h / 2) as f32);
    let q = u as f32 / 4.0;
    gfx::fill_poly(&[(ax, ay - q * 1.3), (ax + q * 1.3, ay), (ax, ay + q * 1.3)], 0x06230f);

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
    // Ish stoli foni (JPEG, ichki dekoder bilan bir marta dekodlanadi)
    let wp = crate::image::decode(crate::wallpaper::JPG);
    let mut windows: Vec<Window> = Vec::new();
    let mut cx = sw / 2;
    let mut cy = sh / 2;
    let mut pressed = false;
    let mut drag: Option<(usize, usize)> = None; // (oyna indeksi, offset)
    let mut shift = false;
    let mut opened = 0;
    let mut last_sec = time::now().second;
    let mut desk = Desk {
        menu: None,
        cal_off: 0,
        clip: None,
        playing: false,
        pos: 0,
        t: 0,
        cpu: task::cpu_load(),
        icons: home_icons(),
        sel: None,
        last_click: (0, usize::MAX),
        anim: None,
        switcher: None,
        toasts: Vec::new(),
        history: Vec::new(),
        notif_open: false,
    };
    let mut alt = false;
    {
        let name = accounts::current().map(|a| if a.full_name.is_empty() { a.username } else { a.full_name }).unwrap_or_default();
        notify("Xuş kelibsiz!", &alloc::format!("{} · Zamin OS {}", name, env!("CARGO_PKG_VERSION")), 0x2fbf5f);
    }
    let mut last_play_draw = 0u64;

    draw(&windows, cx, cy, &wp, &desk);
    gfx::flush();

    loop {
        let mut changed = false;
        // Soat oynasi oçiq bölsa — har soniyada yangilaymiz
        let sec = time::now().second;
        if sec != last_sec {
            last_sec = sec;
            // menyu soati, CPU halqasi va soat oynasi — har soniyada
            desk.cpu = task::cpu_load();
            if sec % 5 == 0 {
                desk.icons = home_icons();
            }
            changed = true;
        }
        // Bildirişnomalar navbati → ekran va tarix
        {
            let mut q = NOTIFY_Q.lock();
            for (t, b, c) in q.drain(..) {
                let tm = time::now();
                desk.history.push((t.clone(), b.clone(), c, alloc::format!("{:02}:{:02}", tm.hour, tm.minute)));
                if desk.history.len() > 20 {
                    desk.history.remove(0);
                }
                desk.toasts.push(Toast { title: t, body: b, color: c, t0: crate::timer::uptime_ms() });
                if desk.toasts.len() > 3 {
                    desk.toasts.remove(0);
                }
                changed = true;
            }
        }
        if !desk.toasts.is_empty() {
            let now = crate::timer::uptime_ms();
            let before = desk.toasts.len();
            desk.toasts.retain(|t| now.saturating_sub(t.t0) < TOAST_MS);
            let sliding = desk.toasts.iter().any(|t| {
                let a = now.saturating_sub(t.t0);
                a < SLIDE_MS || a > TOAST_MS - SLIDE_MS
            });
            if sliding || before != desk.toasts.len() {
                changed = true;
            }
        }
        // Animatsiya
        if let Some(a) = desk.anim {
            if crate::timer::uptime_ms().saturating_sub(a.t0) >= a.dur {
                if let Some(w) = windows.iter_mut().find(|w| w.id == a.reveal) {
                    w.hidden = false;
                }
                desk.anim = None;
            }
            changed = true;
        }
        // Vidjet pleeri
        if desk.playing {
            let now = crate::timer::uptime_ms();
            let dt = now.wrapping_sub(desk.t) as u32;
            desk.t = now;
            desk.pos = desk.pos.saturating_add(dt);
            let dur = desk.clip.as_ref().map(|c| c.duration_ms()).unwrap_or(0);
            if desk.pos >= dur {
                desk.pos = dur;
                desk.playing = false;
                changed = true;
            }
            if now.wrapping_sub(last_play_draw) >= 200 {
                last_play_draw = now;
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
                        // Davom etayotgan animatsiyani yakunlaymiz (oyna yashirin qolmasin)
                        if let Some(a) = desk.anim.take() {
                            if let Some(w) = windows.iter_mut().find(|w| w.id == a.reveal) {
                                w.hidden = false;
                            }
                        }
                        if !notif_click(&mut desk, cx, cy)
                            && !menu_click(&mut desk, &mut windows, cx, cy, &mut opened)
                            && !handle_click(&mut windows, cx, cy, sh, &mut drag, &mut opened, &mut desk.anim)
                            && !widget_click(&mut desk, sw, cx, cy)
                        {
                            desk_icon_click(&mut desk, &mut windows, cx, cy, &mut opened);
                        }
                    } else {
                        pressed = false;
                        drag = None;
                    }
                    changed = true;
                }
                (input::EV_KEY, 42) | (input::EV_KEY, 54) => shift = e.value != 0,
                (input::EV_KEY, 56) | (input::EV_KEY, 100) => {
                    alt = e.value != 0;
                    if !alt {
                        // Alt qoʻyib yuborildi — tanlangan oyna faollaşadi
                        if let Some(sel) = desk.switcher.take() {
                            let list = switcher_list(&windows);
                            if let Some(&wi) = list.get(sel % list.len().max(1)) {
                                activate_window(&mut windows, wi, &mut desk.anim);
                            }
                            changed = true;
                        }
                    }
                }
                (input::EV_KEY, 15) if e.value == 1 && alt => {
                    let n = windows.len().min(10);
                    if n > 0 {
                        let cur = desk.switcher.unwrap_or(0);
                        desk.switcher = Some(if shift { (cur + n - 1) % n } else { (cur + 1) % n });
                        changed = true;
                    }
                }
                (input::EV_KEY, 17) | (input::EV_KEY, 50) | (input::EV_KEY, 33) if e.value == 1 && alt => {
                    // Alt+W — yopiş, Alt+M — yashiriş, Alt+F — kattalaştiriş
                    if let Some(i) = windows.iter().rposition(|w| !w.hidden) {
                        match e.code {
                            17 => {
                                let mut w = windows.remove(i);
                                close_terminal(&mut w);
                            }
                            50 => minimize_window(&mut windows, i, &mut desk.anim),
                            _ => zoom_window(&mut windows, i, &mut desk.anim),
                        }
                        changed = true;
                    }
                }
                (input::EV_KEY, 1) if e.value == 1 => {
                    // Esc — menyu, panel va almaştirgiçni yopadi
                    desk.menu = None;
                    desk.notif_open = false;
                    desk.switcher = None;
                    changed = true;
                }
                (input::EV_KEY, _) if alt => {}
                (input::EV_KEY, code) if e.value == 1 => {
                    if let Some(w) = windows.last_mut().filter(|w| !w.hidden && w.kind.takes_keys()) {
                        type_key(w, code, shift);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        // Siljitiş
        if pressed {
            if let Some((idx, RESIZE)) = drag {
                if idx < windows.len() {
                    let u = body();
                    let w = &mut windows[idx];
                    w.w = cx.saturating_sub(w.x).clamp(u * 16, sw.saturating_sub(w.x));
                    w.h = cy.saturating_sub(w.y).clamp(u * 10, sh.saturating_sub(w.y));
                    changed = true;
                }
            } else if let Some((idx, off)) = drag {
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
            draw(&windows, cx, cy, &wp, &desk);
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

#[allow(clippy::too_many_arguments)]
fn handle_click(
    windows: &mut Vec<Window>,
    cx: usize,
    cy: usize,
    sh: usize,
    drag: &mut Option<(usize, usize)>,
    opened: &mut usize,
    anim: &mut Option<Anim>,
) -> bool {
    let now = crate::timer::uptime_ms();
    // Dok (pastdagi markazlashgan panel)?
    let (sw, _) = gfx::size();
    let (_, dy, _, _, icon, _) = dock_geom(sw, sh);
    if cy >= dy {
        for (i, l) in LAUNCHERS.iter().enumerate() {
            let (ix, iy, isz) = dock_icon_rect(sw, sh, i);
            let _ = icon;
            if cx >= ix && cx < ix + isz && cy >= iy && cy < iy + isz {
                // Ilova oçiq bölsa (yashirilgan ham) — uni old planga qaytaramiz
                if let Some(j) = windows.iter().rposition(|w| win_of_launcher(w, l.kind)) {
                    let w = windows.remove(j);
                    if w.hidden {
                        *anim = Some(Anim { from: (ix, iy, isz, isz), to: (w.x, w.y, w.w, w.h), t0: now, dur: ANIM_MS, reveal: w.id });
                    }
                    windows.push(w);
                    return true;
                }
                match l.kind {
                    Kind::Music => open_file(windows, "~/Musiqa/kuy.mp3", *opened),
                    Kind::Video => open_file(windows, "~/Videolar/zamin.avi", *opened),
                    Kind::Code => windows.push(new_window(Kind::Editor, *opened)),
                    _ => windows.push(new_window(l.kind, *opened)),
                }
                *opened += 1;
                // Yangi oyna dok ikonasidan kattalaşib çiqadi
                if let Some(w) = windows.last_mut() {
                    w.hidden = true;
                    *anim = Some(Anim { from: (ix, iy, isz, isz), to: (w.x, w.y, w.w, w.h), t0: now, dur: ANIM_MS, reveal: w.id });
                }
                return true;
            }
        }
        return true;
    }
    // Yuqoridan (oxirgi = eng tepada) oynalarni tekşiramiz
    for i in (0..windows.len()).rev() {
        if windows[i].hidden {
            continue;
        }
        match windows[i].dot_at(cx, cy) {
            Some(0) => {
                let mut w = windows.remove(i);
                close_terminal(&mut w);
                return true;
            }
            Some(1) => {
                // Yashiriş: oyna eng pastki qatlamga, dokdan qaytariladi
                minimize_window(windows, i, anim);
                return true;
            }
            Some(_) => {
                zoom_window(windows, i, anim);
                return true;
            }
            None => {}
        }
        if windows[i].on_grip(cx, cy) {
            let w = windows.remove(i);
            windows.push(w);
            *drag = Some((windows.len() - 1, RESIZE));
            return true;
        }
        if windows[i].kind == Kind::Editor && windows[i].in_save_btn(cx, cy) {
            let msg = save_notes(&windows[i]);
            notify("Matn muharriri", &msg, 0x4aa3df);
            windows[i].cur = msg.into_bytes(); // holat xabari
            return true;
        }
        if windows[i].kind == Kind::Audio && windows[i].clip.is_some() && windows[i].in_play_btn(cx, cy) {
            let w = &mut windows[i];
            w.playing = !w.playing;
            w.play_t = crate::timer::uptime_ms();
            if w.play_pos >= w.clip.as_ref().map(|c| c.duration_ms()).unwrap_or(0) {
                w.play_pos = 0; // oxirida — boşidan
            }
            audio_sync(w);
            return true;
        }
        if windows[i].title_bar(cx, cy) {
            let off = (cx - windows[i].x) | ((cy - windows[i].y) << 16);
            let w = windows.remove(i);
            windows.push(w); // old planga
            *drag = Some((windows.len() - 1, off));
            return true;
        }
        // Fayllar oynasida qator bosiş: papka — kiradi; fayl — oçadi
        if windows[i].kind == Kind::Files {
            let target = if files_back(&windows[i], cx, cy) {
                Some(path_join(&windows[i].path, ".."))
            } else {
                files_side_at(&windows[i], cx, cy)
            };
            if let Some(p) = target {
                let mut w = windows.remove(i);
                load_dir(&mut w, &p);
                windows.push(w);
                return true;
            }
            if let Some(row) = file_row_at(&windows[i], cx, cy) {
                // Oynani old planga olib, keyin faollaştiramiz
                let w = windows.remove(i);
                windows.push(w);
                let wi = windows.len() - 1;
                files_activate(windows, wi, row, opened);
                return true;
            }
        }
        if windows[i].contains(cx, cy) {
            let w = windows.remove(i);
            windows.push(w); // old planga — fokus
            return true;
        }
    }
    false
}

fn draw(windows: &[Window], cx: usize, cy: usize, wp: &Option<crate::image::Image>, desk: &Desk) {
    let (w, h) = gfx::size();
    gfx::reset_clip();
    // Fon: rasm (wallpaper) yoki gradient
    if let Some(img) = wp {
        gfx::blit_scaled(0, 0, w, h, img.w, img.h, &img.px);
    } else {
        let mid = h * 9 / 20;
        gradient_rect(0, 0, w, mid, 0x24406e, 0x172a4a);
        gradient_rect(0, mid, w, h - mid, 0x172a4a, 0x0a1320);
    }

    // Vidjetlar — ish stoli qatlamida (oynalar ularning ustida)
    draw_widgets(w, h, desk);
    draw_desk_icons(desk);

    draw_menubar(w, desk.menu);

    let top = windows.iter().rposition(|w| !w.hidden);
    for (i, win) in windows.iter().enumerate() {
        if win.hidden {
            continue;
        }
        draw_window(win, Some(i) == top, cx, cy);
    }
    gfx::reset_clip();
    if let Some(a) = &desk.anim {
        draw_anim(a);
    }

    draw_dock(w, h, cx, cy, windows);
    draw_toasts(desk);
    if desk.notif_open {
        draw_notif_panel(desk);
    }
    if let Some(m) = desk.menu {
        draw_menu(m, cx, cy);
    }
    if let Some(sel) = desk.switcher {
        draw_switcher(windows, sel);
    }
    cursor(cx, cy);
}

const MENUS: [&str; 3] = ["Ilovalar", "Joylar", "Tizim"];

/// Menyu sarlavhalarining x oraliqlari (çizilgan joy bilan bir xil).
fn menubar_titles() -> [(usize, usize); 3] {
    let mut x = body() + small() + body() / 2;
    x += gfx::text_width("Zamin OS", small()) + body() * 3 / 2;
    let mut out = [(0, 0); 3];
    for (i, m) in MENUS.iter().enumerate() {
        let w = gfx::text_width(m, small());
        out[i] = (x, x + w);
        x += w + body() * 5 / 4;
    }
    out
}

/// Menyu sarlavhasi bosildimi?
fn menubar_title_at(cx: usize, cy: usize) -> Option<usize> {
    if cy >= menubar_h() {
        return None;
    }
    let pad = body() / 2;
    menubar_titles().iter().position(|&(a, b)| cx + pad >= a && cx < b + pad)
}

/// Yuqori menyu paneli (shaffof): logo, menyular, holat belgilari, sana.
fn draw_menubar(w: usize, open: Option<usize>) {
    let mb = menubar_h();
    fill_round_rect_a(0, 0, w, mb, 0, 0x0b1424, 150);
    fill_rect(0, mb - 1, w, 1, 0x1f2c42);
    let ty = (mb.saturating_sub(small())) / 2;
    let mut x = body();
    // Logo (kiçik yashil olmos-yulduz)
    let ls = small();
    menu_logo(x, (mb.saturating_sub(ls)) / 2, ls);
    x += ls + body() / 2;
    gfx::draw_text(x, ty, "Zamin OS", 0xf2f6fc, small());
    for (i, &(a, b)) in menubar_titles().iter().enumerate() {
        if open == Some(i) {
            fill_round_rect_a(a - body() / 2, 3, b - a + body(), mb - 6, body() / 3, 0xffffff, 46);
        }
        gfx::draw_text(a, ty, MENUS[i], 0xe1e8f2, small());
    }
    // Öng tomon: holat belgilari + sana
    let t = time::now();
    let date = alloc::format!(
        "{}, {}-{} {:02}:{:02}",
        weekday_short(t.year, t.month, t.day),
        t.day,
        month_name(t.month),
        t.hour,
        t.minute
    );
    let dw = gfx::text_width(&date, small());
    let rx = w.saturating_sub(dw + body());
    gfx::draw_text(rx, ty, &date, 0xeaf0f8, small());
    // belgilar (sana çapida)
    let mut sx = rx.saturating_sub(body());
    let cy2 = mb / 2;
    // batareya
    sx = sx.saturating_sub(small() * 3 / 2);
    status_battery(sx, cy2, small());
    sx = sx.saturating_sub(small() * 3 / 2);
    status_wifi(sx, cy2, small());
    sx = sx.saturating_sub(small() * 3 / 2);
    status_bell(sx, cy2, small());
    let _ = sx;
}

/// Menyu logosi — yashil olmos-yulduz.
fn menu_logo(x: usize, y: usize, s: usize) {
    gfx::fill_round_rect(x, y, s, s, s / 4, 0x1f7a3f);
    let c = x + s / 2;
    let m = y + s / 2;
    gfx::fill_poly(
        &[
            (c as f32, (y + s / 6) as f32),
            ((x + s * 5 / 6) as f32, m as f32),
            (c as f32, (y + s * 5 / 6) as f32),
            ((x + s / 6) as f32, m as f32),
        ],
        0x58e06a,
    );
}

fn month_name(m: u64) -> &'static str {
    [
        "yanvar", "fevral", "mart", "aprel", "may", "iyun", "iyul", "avgust", "sentabr", "oktabr", "noyabr",
        "dekabr",
    ]
    .get((m.saturating_sub(1)) as usize)
    .copied()
    .unwrap_or("")
}

/// Hafta kuni (qisqa) — Zeller formulasi.
fn weekday_index(year: u64, month: u64, day: u64) -> usize {
    let (mut y, mut m) = (year as i64, month as i64);
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let k = y % 100;
    let j = y / 100;
    let h = (day as i64 + 13 * (m + 1) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // h: 0=shanba,1=yakshanba,...6=juma → dushanbadan boshlab indeks
    (((h + 5) % 7) as usize) % 7 // 0=Dush
}
fn weekday_short(year: u64, month: u64, day: u64) -> &'static str {
    ["Duş", "Seş", "Çor", "Pay", "Jum", "Şan", "Yak"][weekday_index(year, month, day)]
}

// Holat belgilari (markaz cy, baland s)
fn status_bell(cx: usize, cy: usize, s: usize) {
    let col = 0xdbe4f0;
    gfx::fill_round_rect(cx - s / 4, cy - s / 3, s / 2, s * 2 / 5, s / 6, col);
    gfx::fill_rect(cx - s / 3, cy + s / 10, s * 2 / 3, (s / 10).max(1), col);
    gfx::fill_circle(cx, cy + s / 4, (s / 10).max(1), col);
}
fn status_wifi(cx: usize, cy: usize, s: usize) {
    let col = 0xdbe4f0;
    for (i, rr) in [s / 2, s / 3, s / 6].iter().enumerate() {
        let _ = i;
        gfx::round_outline(cx - rr, cy - rr / 2, rr * 2, *rr, *rr, (s / 12).max(1), col);
    }
    gfx::fill_circle(cx, cy + s / 4, (s / 10).max(1), col);
}
fn status_battery(cx: usize, cy: usize, s: usize) {
    let bw = s * 9 / 5;
    let bh = s * 4 / 5;
    let x = cx - bw / 2;
    let y = cy - bh / 2;
    gfx::round_outline(x, y, bw, bh, bh / 4, (s / 12).max(1), 0xdbe4f0);
    gfx::fill_rect(x + bw, y + bh / 3, (s / 8).max(1), bh / 3, 0xdbe4f0);
    gfx::fill_round_rect(x + s / 10, y + s / 10, bw - s / 5, bh - s / 5, bh / 6, 0x34c759);
}

/// Ish stoli holati: oçiq menyu, kalendar siljişi, vidjet pleeri, CPU.
struct Desk {
    menu: Option<usize>,
    cal_off: i32,
    clip: Option<crate::audio::Clip>,
    playing: bool,
    pos: u32,
    t: u64,
    cpu: u32,
    icons: Vec<String>,         // ish stoli ikonalari (uy papkalari)
    sel: Option<usize>,         // tanlangan ikona
    last_click: (u64, usize),   // ikki marta bosiş: (vaqt, ikona)
    anim: Option<Anim>,
    switcher: Option<usize>,    // Alt+Tab: tanlangan element (oynalar yangiligi tartibida)
    toasts: Vec<Toast>,         // ekrandagi bildirişnomalar
    history: Vec<(String, String, Color, String)>, // (sarlavha, matn, rang, vaqt)
    notif_open: bool,           // bildirişnomalar paneli oçiqmi
}

// ---------------- Bildirişnomalar ----------------

struct Toast {
    title: String,
    body: String,
    color: Color,
    t0: u64,
}

const TOAST_MS: u64 = 4500;
const SLIDE_MS: u64 = 280;

static NOTIFY_Q: crate::sync::SpinLock<Vec<(String, String, Color)>> = crate::sync::SpinLock::new(Vec::new());

/// Bildirişnoma yuborish (yadroning istalgan joyidan; GUI keyingi kadrda körsatadi).
pub fn notify(title: &str, body: &str, color: Color) {
    let mut q = NOTIFY_Q.lock();
    if q.len() < 16 {
        q.push((String::from(title), String::from(body), color));
    }
}

fn toast_geo(sw: usize, k: usize) -> (usize, usize, usize, usize) {
    let u = body();
    let (tw, th) = (u * 16, u * 3);
    (sw.saturating_sub(tw + u / 2), menubar_h() + u / 2 + k * (th + u / 2), tw, th)
}

/// Ekrandagi bildirişnomalar (öngdan sirpanib kiradi va çiqadi).
fn draw_toasts(desk: &Desk) {
    let (sw, _) = gfx::size();
    let u = body();
    let now = crate::timer::uptime_ms();
    for (k, t) in desk.toasts.iter().enumerate() {
        let (x0, y, w, h) = toast_geo(sw, k);
        let age = now.saturating_sub(t.t0);
        let slide = if age < SLIDE_MS {
            1.0 - age as f32 / SLIDE_MS as f32
        } else if age > TOAST_MS - SLIDE_MS {
            (age - (TOAST_MS - SLIDE_MS)) as f32 / SLIDE_MS as f32
        } else {
            0.0
        };
        let e = slide * slide;
        let x = x0 + ((w + u) as f32 * e) as usize;
        if x >= sw {
            continue;
        }
        gfx::shadow(x, y, w, h, u / 2, 10);
        fill_round_rect_a(x, y, w, h, u / 2, 0x1c2434, 245);
        gfx::round_outline(x, y, w, h, u / 2, 1, 0x3a4660);
        let isz = u * 3 / 2;
        gfx::fill_round_rect(x + u / 2, y + (h - isz) / 2, isz, isz, isz / 4, t.color);
        zamin_logo(x + u / 2 + isz / 2, y + h / 2, isz * 3 / 5);
        let tx = x + u + isz;
        let tw = w.saturating_sub(isz + u * 3 / 2);
        gfx::draw_text(tx, y + u / 3, &fit_text(&t.title, tw, small()), OQ, small());
        gfx::draw_text(tx, y + u / 3 + small() + u / 4, &fit_text(&t.body, tw, small() * 9 / 10), 0xaebccf, small() * 9 / 10);
        gfx::draw_text(x + w - u * 2, y + u / 3, "hozir", 0x6f7f95, small() * 4 / 5);
    }
}

/// Qoʻngʻiroq belgisi markazi (menyu panelida, sanadan çapda).
fn bell_pos(sw: usize) -> (usize, usize) {
    let t = time::now();
    let date = alloc::format!(
        "{}, {}-{} {:02}:{:02}",
        weekday_short(t.year, t.month, t.day),
        t.day,
        month_name(t.month),
        t.hour,
        t.minute
    );
    let rx = sw.saturating_sub(gfx::text_width(&date, small()) + body());
    (rx.saturating_sub(body() + small() * 9 / 2), menubar_h() / 2)
}

fn notif_panel_geo(sw: usize, n: usize) -> (usize, usize, usize, usize) {
    let u = body();
    let pw = u * 17;
    let ph = u * 2 + n.max(1) * (u * 3 + u / 3) + u / 2;
    (sw.saturating_sub(pw + u / 2), menubar_h() + 3, pw, ph)
}

/// Bildirişnomalar markazi: oxirgi xabarlar (yangisi tepada).
fn draw_notif_panel(desk: &Desk) {
    let (sw, _) = gfx::size();
    let u = body();
    let n = desk.history.len().min(6);
    let (x, y, w, h) = notif_panel_geo(sw, n);
    gfx::shadow(x, y, w, h, u / 2, 12);
    fill_round_rect_a(x, y, w, h, u / 2, 0x151c2a, 248);
    gfx::round_outline(x, y, w, h, u / 2, 1, 0x34405a);
    gfx::draw_text(x + u, y + u / 2, "Bildirişnomalar", OQ, small());
    if n > 0 {
        let cw = gfx::text_width("Tozalaş", small() * 9 / 10);
        gfx::draw_text(x + w - cw - u, y + u / 2, "Tozalaş", 0x63a8ff, small() * 9 / 10);
    } else {
        gfx::draw_text_centered(x + w / 2, y + u * 2 + u, "Bildirişnomalar yöq", 0x6f7f95, small());
    }
    for (k, (t, b, c, tm)) in desk.history.iter().rev().take(6).enumerate() {
        let iy = y + u * 2 + k * (u * 3 + u / 3);
        fill_round_rect_a(x + u / 2, iy, w - u, u * 3, u / 2, 0xffffff, 14);
        let isz = u * 5 / 4;
        gfx::fill_round_rect(x + u, iy + (u * 3 - isz) / 2, isz, isz, isz / 4, *c);
        let tx = x + u * 3 / 2 + isz;
        let tw = w.saturating_sub(isz + u * 4);
        gfx::draw_text(tx, iy + u / 3, &fit_text(t, tw, small() * 9 / 10), OQ, small() * 9 / 10);
        gfx::draw_text(tx, iy + u / 3 + small(), &fit_text(b, tw, small() * 4 / 5), 0x9fb0c3, small() * 4 / 5);
        let tmw = gfx::text_width(tm, small() * 4 / 5);
        gfx::draw_text(x + w - u - tmw, iy + u / 3, tm, 0x6f7f95, small() * 4 / 5);
    }
}

/// Bildirişnoma qatlamiga bosiş (toast, qoʻngʻiroq, panel). Bosildi — true.
fn notif_click(desk: &mut Desk, cx: usize, cy: usize) -> bool {
    let (sw, _) = gfx::size();
    let u = body();
    if desk.notif_open {
        let n = desk.history.len().min(6);
        let (x, y, w, h) = notif_panel_geo(sw, n);
        if cx >= x && cx < x + w && cy >= y && cy < y + h {
            if n > 0 && cy < y + u * 2 && cx >= x + w - u * 5 {
                desk.history.clear(); // "Tozalaş"
            }
            return true;
        }
        desk.notif_open = false;
        return true;
    }
    let (bx, by) = bell_pos(sw);
    if cy < menubar_h() && cx + u >= bx && cx <= bx + u {
        let _ = by;
        desk.notif_open = true;
        desk.menu = None;
        return true;
    }
    for k in 0..desk.toasts.len() {
        let (x, y, w, h) = toast_geo(sw, k);
        if cx >= x && cx < x + w && cy >= y && cy < y + h {
            desk.toasts.remove(k); // bosilgan xabar yopiladi
            return true;
        }
    }
    false
}

// ---------------- Ilova almaştirgiç (Alt+Tab) va yorliqlar ----------------

/// Oyna uçun ikona turi va rangi.
fn win_icon(w: &Window) -> (Kind, Color) {
    match w.kind {
        Kind::Audio => (Kind::Audio, 0xff7a2f),
        Kind::Image if crate::video::is_video_ext(&w.path) => (Kind::Video, 0x8257ff),
        k => (k, menu_kind_color(k)),
    }
}

/// Alt+Tab ro'yxati: oynalar yangiligi tartibida (oxirgi faol — birinçi).
fn switcher_list(windows: &[Window]) -> Vec<usize> {
    (0..windows.len()).rev().collect()
}

fn draw_switcher(windows: &[Window], sel: usize) {
    let (sw, sh) = gfx::size();
    let u = body();
    let list = switcher_list(windows);
    let n = list.len().min(10);
    if n == 0 {
        return;
    }
    let isz = u * 4;
    let gap = u;
    let pw = n * isz + (n + 1) * gap;
    let ph = isz + u * 3 + u / 2;
    let (x, y) = ((sw - pw) / 2, (sh - ph) / 2);
    gfx::shadow(x, y, pw, ph, u, 14);
    fill_round_rect_a(x, y, pw, ph, u, 0x141b28, 235);
    gfx::round_outline(x, y, pw, ph, u, 1, 0x3a4660);
    for (k, &wi) in list.iter().take(n).enumerate() {
        let ix = x + gap + k * (isz + gap);
        let iy = y + gap;
        if k == sel % n {
            fill_round_rect_a(ix - u / 3, iy - u / 3, isz + u * 2 / 3, isz + u * 2 / 3, u / 2, 0xffffff, 40);
        }
        let (kind, col) = win_icon(&windows[wi]);
        draw_icon(kind, col, ix, iy, isz);
        if windows[wi].hidden {
            gfx::fill_circle(ix + isz / 2, iy + isz + u / 3, (u / 6).max(2), 0x8a93a6);
        }
    }
    let title = &windows[list[sel % n]].title;
    gfx::draw_text_centered(sw / 2, y + gap + isz + u * 2 / 3, &fit_text(title, pw - u * 2, small()), OQ, small());
}

/// i-oynani old planga oladi (yashirilgan bölsa — dokdan çiqadi).
fn activate_window(windows: &mut Vec<Window>, i: usize, anim: &mut Option<Anim>) {
    if i >= windows.len() {
        return;
    }
    let w = windows.remove(i);
    if w.hidden {
        *anim = Some(Anim { from: dock_rect_for(&w), to: (w.x, w.y, w.w, w.h), t0: crate::timer::uptime_ms(), dur: ANIM_MS, reveal: w.id });
    }
    windows.push(w);
}

/// i-oynani dokka yashiradi (animatsiya bilan).
fn minimize_window(windows: &mut Vec<Window>, i: usize, anim: &mut Option<Anim>) {
    let mut w = windows.remove(i);
    w.hidden = true;
    *anim = Some(Anim { from: (w.x, w.y, w.w, w.h), to: dock_rect_for(&w), t0: crate::timer::uptime_ms(), dur: ANIM_MS, reveal: 0 });
    windows.insert(0, w);
}

/// i-oynani kattalaştiradi yoki oldingi ölçamga qaytaradi.
fn zoom_window(windows: &mut Vec<Window>, i: usize, anim: &mut Option<Anim>) {
    let mut w = windows.remove(i);
    let from = (w.x, w.y, w.w, w.h);
    if let Some((x, y, ww, hh)) = w.saved.take() {
        (w.x, w.y, w.w, w.h) = (x, y, ww, hh);
    } else {
        w.saved = Some((w.x, w.y, w.w, w.h));
        (w.x, w.y, w.w, w.h) = zoom_rect();
    }
    w.hidden = true;
    *anim = Some(Anim { from, to: (w.x, w.y, w.w, w.h), t0: crate::timer::uptime_ms(), dur: ANIM_MS, reveal: w.id });
    windows.push(w);
}

/// Oyna animatsiyasi: "arvoh" töğriburçak bir joydan ikkinçisiga oʻtadi.
#[derive(Clone, Copy)]
struct Anim {
    from: (usize, usize, usize, usize),
    to: (usize, usize, usize, usize),
    t0: u64,
    dur: u64,
    reveal: usize, // tugaganda körsatiladigan oyna id (0 — yöq)
}

const ANIM_MS: u64 = 230;

fn home_icons() -> Vec<String> {
    let mut v: Vec<String> =
        fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir).map(|e| e.name).collect();
    v.sort();
    v
}

/// Ish stoli ikonasi joyi: (x, y, katak eni, katak boʻyi).
fn desk_icon_rect(i: usize) -> (usize, usize, usize, usize) {
    let u = body();
    let (cw, ch) = (u * 5, u * 5);
    (u / 2, menubar_h() + u / 2 + i * ch, cw, ch)
}

/// Ish stoli papka ikonalari (oq yorliq, soyali).
fn draw_desk_icons(desk: &Desk) {
    let u = body();
    let isz = u * 3;
    let px = small() * 9 / 10;
    for (i, name) in desk.icons.iter().enumerate() {
        let (x, y, cw, ch) = desk_icon_rect(i);
        let sel = desk.sel == Some(i);
        if sel {
            fill_round_rect_a(x + (cw - isz) / 2 - u / 4, y + u / 8, isz + u / 2, isz + u / 4, u / 3, 0xffffff, 50);
        }
        big_icon(FKind::Dir, name, x + (cw - isz) / 2, y + u / 4, isz);
        let lbl = fit_text(name, cw - u / 4, px);
        let ty = y + isz + u / 2;
        if sel {
            let lw = gfx::text_width(&lbl, px);
            fill_round_rect(x + cw / 2 - lw / 2 - u / 4, ty - 1, lw + u / 2, px + 4, u / 4, 0x2f6fd6);
        } else {
            gfx::draw_text_centered(x + cw / 2 + 1, ty + 1, &lbl, 0x000000, px);
        }
        gfx::draw_text_centered(x + cw / 2, ty, &lbl, OQ, px);
        let _ = ch;
    }
}

/// Ish stoli ikonasiga bosiş: bir marta — tanlaş, ikki marta — oçiş.
fn desk_icon_click(desk: &mut Desk, windows: &mut Vec<Window>, cx: usize, cy: usize, opened: &mut usize) -> bool {
    let hit = (0..desk.icons.len()).find(|&i| {
        let (x, y, cw, ch) = desk_icon_rect(i);
        cx >= x && cx < x + cw && cy >= y && cy < y + ch
    });
    let now = crate::timer::uptime_ms();
    match hit {
        Some(i) => {
            if desk.last_click.1 == i && desk.sel == Some(i) && now.wrapping_sub(desk.last_click.0) < 500 {
                let mut w = new_window(Kind::Files, *opened);
                load_dir(&mut w, &alloc::format!("~/{}", desk.icons[i]));
                *opened += 1;
                let (x, y, cw, _) = desk_icon_rect(i);
                let u = body();
                w.hidden = true;
                desk.anim = Some(Anim {
                    from: (x + (cw - u * 3) / 2, y + u / 4, u * 3, u * 3),
                    to: (w.x, w.y, w.w, w.h),
                    t0: now,
                    dur: ANIM_MS,
                    reveal: w.id,
                });
                windows.push(w);
                desk.sel = None;
            } else {
                desk.sel = Some(i);
            }
            desk.last_click = (now, i);
            true
        }
        None => {
            let had = desk.sel.is_some();
            desk.sel = None;
            had
        }
    }
}

/// Animatsiya arvohini chizadi (easeOutCubic).
fn draw_anim(a: &Anim) {
    let now = crate::timer::uptime_ms();
    let t = (now.saturating_sub(a.t0) as f32 / a.dur as f32).min(1.0);
    let e = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
    let lerp = |p: usize, q: usize| (p as f32 + (q as f32 - p as f32) * e) as usize;
    let (x, y, w, h) = (lerp(a.from.0, a.to.0), lerp(a.from.1, a.to.1), lerp(a.from.2, a.to.2), lerp(a.from.3, a.to.3));
    if w < 4 || h < 4 {
        return;
    }
    let r = radius().min(w / 2).min(h / 2);
    let th = title_h().min(h / 3);
    fill_round_rect_a(x, y, w, h, r, WIN_BODY, 215);
    fill_round_rect_a(x, y, w, th, r, WIN_TITLE, 230);
    gfx::round_outline(x, y, w, h, r, 1, 0x4a5670);
}

/// Oyna uçun dok ikonasining töğriburçagi (yashiriş/qaytariş animatsiyasi).
fn dock_rect_for(w: &Window) -> (usize, usize, usize, usize) {
    let (sw, sh) = gfx::size();
    let i = LAUNCHERS.iter().position(|l| win_of_launcher(w, l.kind)).unwrap_or(LAUNCHERS.len() / 2);
    let (ix, iy, isz) = dock_icon_rect(sw, sh, i);
    (ix, iy, isz, isz)
}

/// Vidjetlar geometriyasi: (px, pw, [y, h] × 4: ob-havo, kalendar, pleer, tizim).
fn widget_geo(w: usize) -> (usize, usize, [(usize, usize); 4]) {
    let u = body();
    let pw = (w / 4).clamp(u * 15, u * 20);
    let px = w.saturating_sub(pw + u);
    let gap = u * 3 / 4;
    let hs = [u * 7, u * 9, u * 5, u * 6];
    let mut y = menubar_h() + u;
    let mut out = [(0, 0); 4];
    for i in 0..4 {
        out[i] = (y, hs[i]);
        y += hs[i] + gap;
    }
    (px, pw, out)
}

/// Kalendar siljişidan (yil, oy).
fn cal_month(off: i32) -> (u64, u64) {
    let t = time::now();
    let m0 = t.year as i64 * 12 + (t.month as i64 - 1) + off as i64;
    ((m0 / 12) as u64, (m0 % 12) as u64 + 1)
}

/// Pleer vidjeti tugmalari: (oldingi, ijro, keyingi) markazlari, y va radius.
fn music_buttons(px: usize, my: usize, mh: usize) -> ([usize; 3], usize, usize) {
    let u = body();
    let tx = px + u + (mh - u * 2) + u;
    let by = my + mh - u * 3 / 2;
    ([tx + u / 2, tx + u * 2, tx + u * 7 / 2], by, u * 3 / 5)
}

/// Öng tomondagi vidjetlar paneli: ob-havo, kalendar, pleer, tizim halqalari.
fn draw_widgets(w: usize, _h: usize, desk: &Desk) {
    let u = body();
    let (px, pw, g) = widget_geo(w);

    // --- Ob-havo ---
    let (py, wh) = g[0];
    widget_card(px, py, pw, wh);
    gfx::draw_text(px + u, py + u, "Toşkent", 0xeaf0f8, small());
    gfx::fill_circle(px + pw - u * 3 / 2, py + u * 5 / 4, (u / 4).max(2), 0x8ab4ff);
    gfx::fill_circle(px + u * 2, py + u * 3, u * 4 / 5, 0xffd36b);
    gfx::fill_round_rect(px + u * 3 / 2, py + u * 3, u * 2, u, u / 2, 0xe7edf6);
    gfx::draw_text(px + u * 9 / 2, py + u * 9 / 5, "16°", 0xffffff, u * 2);
    gfx::draw_text(px + u, py + u * 9 / 2, "Qisman bulutli", 0xaebccf, small());
    gfx::draw_text(px + u, py + u * 11 / 2, "H:19°   L:9°", 0x8ea0b6, small() * 9 / 10);

    // --- Kalendar (oylar almaşadi) ---
    let (py, ch) = g[1];
    widget_card(px, py, pw, ch);
    let (cy_, cm) = cal_month(desk.cal_off);
    let today = time::now();
    let title = alloc::format!("{} {}", month_name(cm), cy_);
    gfx::draw_text(px + u, py + u * 2 / 3, &title, 0xeaf0f8, small());
    // ‹ › tugmalari
    let ay = py + u;
    for (k, ax) in [px + pw - u * 3, px + pw - u * 3 / 2].iter().enumerate() {
        let d = if k == 0 { -1.0f32 } else { 1.0 };
        let (fx, fy) = (*ax as f32, ay as f32);
        let q = u as f32 / 4.0;
        gfx::fill_poly(&[(fx - d * q, fy - q * 1.4), (fx - d * q, fy + q * 1.4), (fx + d * q, fy)], 0xaebccf);
    }
    let cols = 7;
    let cellw = (pw - u) / cols;
    let hdr = ["Du", "Se", "Ço", "Pa", "Ju", "Şa", "Ya"];
    let gy0 = py + u * 2;
    for (i, d) in hdr.iter().enumerate() {
        gfx::draw_text_centered(px + u / 2 + i * cellw + cellw / 2, gy0, d, 0x7f8ea3, small() * 4 / 5);
    }
    let first = weekday_index(cy_, cm, 1);
    let dim = days_in_month(cy_, cm);
    let cellh = (ch - (gy0 - py) - u / 2) / 6;
    for d in 1..=dim {
        let idx = first + (d as usize - 1);
        let (row, col) = (idx / 7, idx % 7);
        let cxp = px + u / 2 + col * cellw + cellw / 2;
        let cyp = gy0 + small() + u / 2 + row * cellh;
        let is_today = desk.cal_off == 0 && d == today.day;
        if is_today {
            gfx::fill_circle(cxp, cyp, (cellh / 2).min(cellw / 2), 0x34c759);
        }
        let col_txt = if is_today { 0x06230f } else if col >= 5 { 0x8ea0b6 } else { 0xd3dcea };
        gfx::draw_text_centered(cxp, cyp.saturating_sub(small() / 2), &alloc::format!("{}", d), col_txt, small() * 4 / 5);
    }

    // --- Pleer ---
    let (py, mh) = g[2];
    widget_card(px, py, pw, mh);
    let asz = mh - u * 2;
    gradient_round_rect(px + u, py + u, asz, asz, u / 3, 0xff8a3d, 0x7a3cff);
    let (nc, ny) = (px + u + asz / 2, py + u + asz / 2);
    gfx::fill_circle(nc - asz / 8, ny + asz / 6, asz / 9, OQ);
    fill_rect(nc - asz / 8 + asz / 9 - 2, ny - asz / 4, (asz / 18).max(2), asz * 5 / 12, OQ);
    fill_rect(nc - asz / 8 + asz / 9 - 2, ny - asz / 4, asz / 4, (asz / 14).max(2), OQ);
    let tx = px + u + asz + u;
    gfx::draw_text(tx, py + u * 2 / 3, "Zamin kuyi", 0xffffff, small());
    gfx::draw_text(tx, py + u * 2 / 3 + small() + u / 4, "Anʼanaviy musiqa", 0xaebccf, small() * 9 / 10);
    let (bx, by, br) = music_buttons(px, py, mh);
    // oldingi / keyingi
    for (k, cxb) in [bx[0], bx[2]].iter().enumerate() {
        let d = if k == 0 { -1.0f32 } else { 1.0 };
        let (fx, fy) = (*cxb as f32, by as f32);
        let q = br as f32 * 0.55;
        gfx::fill_poly(&[(fx - d * q, fy - q), (fx - d * q, fy + q), (fx + d * q * 0.6, fy)], 0xdbe3ee);
        fill_rect((fx + d * q * 0.6 - if d > 0.0 { 0.0 } else { 2.0 }) as usize, (fy - q) as usize, 2, (q * 2.0) as usize, 0xdbe3ee);
    }
    gfx::fill_circle(bx[1], by, br, 0x34c759);
    if desk.playing {
        fill_rect(bx[1] - br / 3, by - br / 2, br / 4, br, 0x06230f);
        fill_rect(bx[1] + br / 8, by - br / 2, br / 4, br, 0x06230f);
    } else {
        let t = br * 2 / 5;
        gfx::fill_poly(
            &[((bx[1] - t / 2) as f32, (by - t) as f32), ((bx[1] + t) as f32, by as f32), ((bx[1] - t / 2) as f32, (by + t) as f32)],
            0x06230f,
        );
    }
    // progress
    let dur = desk.clip.as_ref().map(|c| c.duration_ms()).unwrap_or(0);
    let pbx = bx[2] + br + u;
    let pbw = (px + pw).saturating_sub(pbx + u);
    gfx::fill_round_rect(pbx, by - 2, pbw, 4, 2, 0x2a3346);
    if dur > 0 {
        let pw2 = (desk.pos.min(dur) as usize * pbw / dur as usize).max(4);
        gfx::fill_round_rect(pbx, by - 2, pw2, 4, 2, 0x34c759);
    }

    // --- Tizim halqalari (haqiqiy: CPU — rejalaştiruvçi namunalari, RAM — sahifalar, Disk — ZaminFS bloklari) ---
    let (py, sh2) = g[3];
    widget_card(px, py, pw, sh2);
    let (free_p, total_p) = crate::memory::frame::stats();
    let used_p = total_p.saturating_sub(free_p);
    let ram_pct = if total_p > 0 { (used_p * 100 / total_p) as u32 } else { 0 };
    let used_mb = used_p * 4 / 1024;
    let (_, _, dused, dtotal) = crate::fs::zaminfs::state();
    let (disk_pct, disk_lbl) = if dtotal > 0 {
        let mb = dused as usize * crate::fs::zaminfs::BLOCK / (1024 * 1024);
        ((dused as u64 * 100 / dtotal as u64) as u32, alloc::format!("{}MB", mb.max(1)))
    } else {
        (0, String::from("—"))
    };
    let ry = py + sh2 / 2 - u / 3;
    let rr = (pw / 8).min(sh2 / 3);
    let slot = pw / 3;
    let rings = [
        (slot / 2, desk.cpu, 0x34c759, alloc::format!("{}%", desk.cpu), "CPU"),
        (slot + slot / 2, ram_pct, 0x2f9bff, alloc::format!("{}MB", used_mb), "RAM"),
        (slot * 2 + slot / 2, disk_pct, 0xb06bff, disk_lbl, "Disk"),
    ];
    for (ox, pct, col, val, lab) in rings {
        let cxp = px + ox;
        ring(cxp, ry, rr, (rr / 4).max(3), pct, 0x2a3346, col);
        gfx::draw_text_centered(cxp, ry.saturating_sub(small() * 3 / 5), &val, 0xffffff, small() * 4 / 5);
        gfx::draw_text_centered(cxp, py + sh2 - small() - u / 3, lab, 0x9fb0c3, small() * 4 / 5);
    }
}

/// Vidjetlarga bosiş (kalendar ‹ ›, pleer tugmalari). Bosildi — true.
fn widget_click(desk: &mut Desk, sw: usize, cx: usize, cy: usize) -> bool {
    let u = body();
    let (px, pw, g) = widget_geo(sw);
    let near = |x: usize, y: usize, r: usize| {
        let (dx, dy) = (cx as isize - x as isize, cy as isize - y as isize);
        ((dx * dx + dy * dy) as usize) <= r * r
    };
    let (cyy, _) = g[1];
    if near(px + pw - u * 3, cyy + u, u * 3 / 4) {
        desk.cal_off -= 1;
        return true;
    }
    if near(px + pw - u * 3 / 2, cyy + u, u * 3 / 4) {
        desk.cal_off += 1;
        return true;
    }
    if near(px + pw / 2, cyy + u, u) && desk.cal_off != 0 {
        desk.cal_off = 0; // sarlavhani bosiş — bugungi oyga qaytiş
        return true;
    }
    let (my, mh) = g[2];
    let (bx, by, br) = music_buttons(px, my, mh);
    if near(bx[1], by, br + 4) {
        if desk.clip.is_none() {
            desk.clip = crate::audio::decode("kuy.mp3", crate::audio::SAMPLE_KUY);
        }
        if let Some(c) = &desk.clip {
            let dur = c.duration_ms();
            if desk.pos >= dur {
                desk.pos = 0;
            }
            desk.playing = !desk.playing;
            desk.t = crate::timer::uptime_ms();
            if desk.playing && !crate::audio::sound::present() {
                notify("Övoz qurilmasi topilmadi", "Ijro faqat vizual körsatiladi (virtio-sound yöq)", 0xff9f0a);
            }
            if desk.playing {
                crate::audio::sound::play(&c.samples, c.rate, c.channels, desk.pos);
            } else {
                crate::audio::sound::stop();
            }
        }
        return true;
    }
    if near(bx[0], by, br + 4) || near(bx[2], by, br + 4) {
        // boşiga (keyingi — yagona kuy bölgani uçun ham boşiga)
        desk.pos = 0;
        desk.t = crate::timer::uptime_ms();
        if let (true, Some(c)) = (desk.playing, &desk.clip) {
            crate::audio::sound::play(&c.samples, c.rate, c.channels, 0);
        }
        return true;
    }
    false
}

// ---------------- Oçiladigan menyular ----------------

#[derive(Clone)]
enum MAct {
    Open(Kind),
    Path(String),
    Reboot,
    Shutdown,
    Sep,
}

fn menu_items(i: usize) -> Vec<(String, MAct)> {
    let s = |t: &str| String::from(t);
    match i {
        0 => alloc::vec![
            (s("Terminal"), MAct::Open(Kind::Terminal)),
            (s("Fayllar"), MAct::Open(Kind::Files)),
            (s("Matn muharriri"), MAct::Open(Kind::Editor)),
            (s("Soat"), MAct::Open(Kind::Clock)),
            (s("Sep"), MAct::Sep),
            (s("Musiqa"), MAct::Open(Kind::Music)),
            (s("Video"), MAct::Open(Kind::Video)),
            (s("Rasmlar"), MAct::Open(Kind::Image)),
            (s("Brauzer"), MAct::Open(Kind::Browser)),
            (s("Sep"), MAct::Sep),
            (s("Sozlamalar"), MAct::Open(Kind::Settings)),
        ],
        1 => {
            let mut v = alloc::vec![(s("Uy"), MAct::Path(s("~"))), (s("Sep"), MAct::Sep)];
            let mut dirs: Vec<String> =
                fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir).map(|e| e.name).collect();
            dirs.sort();
            for d in dirs {
                let p = alloc::format!("~/{}", d);
                v.push((d, MAct::Path(p)));
            }
            v.push((s("Sep"), MAct::Sep));
            v.push((s("Kompyuter (/)"), MAct::Path(s("/"))));
            v
        }
        _ => alloc::vec![
            (s("Zamin OS haqida"), MAct::Open(Kind::About)),
            (s("Sozlamalar"), MAct::Open(Kind::Settings)),
            (s("Sep"), MAct::Sep),
            (s("Qayta yuklaş…"), MAct::Reboot),
            (s("Öçiriş…"), MAct::Shutdown),
        ],
    }
}

/// Menyu joylaşuvi: (x, y, kenglik, [(y0, y1, nom, amal)]).
fn menu_layout(i: usize) -> (usize, usize, usize, Vec<(usize, usize, String, MAct)>) {
    let u = body();
    let items = menu_items(i);
    let w = items
        .iter()
        .filter(|(_, a)| !matches!(a, MAct::Sep))
        .map(|(n, _)| gfx::text_width(n, small()))
        .max()
        .unwrap_or(0)
        .max(u * 7)
        + u * 3;
    let x = menubar_titles()[i].0.saturating_sub(u / 2);
    let y = menubar_h() + 3;
    let mut yy = y + u / 3;
    let mut out = Vec::new();
    for (n, a) in items {
        let h = if matches!(a, MAct::Sep) { u / 2 } else { u * 3 / 2 };
        out.push((yy, yy + h, n, a));
        yy += h;
    }
    (x, y, w, out)
}

fn menu_kind_color(k: Kind) -> Color {
    match k {
        Kind::Editor => 0x4aa3df,
        Kind::Clock => 0xff9f0a,
        Kind::Terminal => 0x5b6b82,
        _ => LAUNCHERS.iter().find(|l| l.kind == k).map(|l| l.color).unwrap_or(0x8a93a6),
    }
}

/// Oçiq menyuni chizadi (siçqon ostidagi band ajratiladi).
fn draw_menu(i: usize, cx: usize, cy: usize) {
    let u = body();
    let (x, y, w, items) = menu_layout(i);
    let h = items.last().map(|it| it.1).unwrap_or(y) + u / 3 - y;
    gfx::shadow(x, y, w, h, u / 2, 10);
    fill_round_rect_a(x, y, w, h, u / 2, 0x1a2131, 245);
    gfx::round_outline(x, y, w, h, u / 2, 1, 0x34405a);
    for (y0, y1, name, act) in &items {
        if matches!(act, MAct::Sep) {
            fill_rect(x + u / 2, (y0 + y1) / 2, w - u, 1, 0x34405a);
            continue;
        }
        let hover = cx >= x && cx < x + w && cy >= *y0 && cy < *y1;
        if hover {
            fill_round_rect_a(x + u / 4, *y0, w - u / 2, y1 - y0, u / 3, 0x2f88ff, 230);
        }
        let ty = y0 + (y1 - y0 - small()) / 2;
        let isz = small() * 4 / 5;
        let iy = y0 + (y1 - y0 - isz) / 2;
        let ix = x + u * 2 / 3;
        match act {
            MAct::Open(k) => gfx::fill_round_rect(ix, iy, isz, isz, isz / 4, menu_kind_color(*k)),
            MAct::Path(_) => {
                gfx::fill_round_rect(ix, iy + isz / 8, isz * 2 / 5, isz / 4, isz / 10, 0x62adff);
                gfx::fill_round_rect(ix, iy + isz / 4, isz, isz * 3 / 4, isz / 6, 0x62adff);
            }
            MAct::Reboot | MAct::Shutdown => {
                gfx::fill_circle(ix + isz / 2, iy + isz / 2, isz / 2, if matches!(act, MAct::Shutdown) { 0xff453a } else { 0xff9f0a });
            }
            MAct::Sep => {}
        }
        gfx::draw_text(ix + isz + u / 2, ty, name, if hover { OQ } else { 0xe1e8f2 }, small());
    }
}

/// Menyuga oid bosişni qayta işlaydi. Bosiş menyuga tegişli bölsa — true.
fn menu_click(desk: &mut Desk, windows: &mut Vec<Window>, cx: usize, cy: usize, opened: &mut usize) -> bool {
    if let Some(i) = desk.menu {
        let (x, _, w, items) = menu_layout(i);
        if cx >= x && cx < x + w {
            if let Some((_, _, _, act)) = items.iter().find(|it| cy >= it.0 && cy < it.1) {
                let act = act.clone();
                if matches!(act, MAct::Sep) {
                    return true; // ajratgiçni bosiş — menyu oçiq qoladi
                }
                desk.menu = None;
                menu_act(act, windows, opened);
                return true;
            }
        }
        desk.menu = match menubar_title_at(cx, cy) {
            Some(t) if t != i => Some(t),
            _ => None,
        };
        return true;
    }
    if let Some(t) = menubar_title_at(cx, cy) {
        desk.menu = Some(t);
        desk.notif_open = false;
        return true;
    }
    false
}

fn menu_act(act: MAct, windows: &mut Vec<Window>, opened: &mut usize) {
    match act {
        MAct::Open(Kind::Music) => open_file(windows, "~/Musiqa/kuy.mp3", *opened),
        MAct::Open(Kind::Video) => open_file(windows, "~/Videolar/zamin.avi", *opened),
        MAct::Open(k) => windows.push(new_window(k, *opened)),
        MAct::Path(p) => {
            let mut w = new_window(Kind::Files, *opened);
            load_dir(&mut w, &p);
            windows.push(w);
        }
        MAct::Reboot | MAct::Shutdown => {
            let off = matches!(act, MAct::Shutdown);
            let (w, h) = gfx::size();
            gradient_rect(0, 0, w, h, 0x101826, 0x05080d);
            zamin_logo(w / 2, h / 2 - body() * 3, body() * 4);
            let msg = if off { "Öçirilmoqda…" } else { "Qayta yuklanmoqda…" };
            gfx::draw_text_centered(w / 2, h / 2, msg, 0xdbe3ee, body());
            gfx::flush();
            let _ = fs::commit();
            crate::audio::sound::stop();
            if off {
                crate::power::shutdown();
            } else {
                crate::power::reboot();
            }
        }
        MAct::Sep => {}
    }
    *opened += 1;
}

/// Vidjet karti foni (shaffof, yumaloq).
fn widget_card(x: usize, y: usize, w: usize, h: usize) {
    gfx::shadow(x, y, w, h, radius(), 6);
    fill_round_rect_a(x, y, w, h, radius(), 0x121a2a, 205);
    gfx::round_outline(x, y, w, h, radius(), 1, 0x2b3650);
}

/// Progress halqasi: foizga qarab rangli yoy.
fn ring(cx: usize, cy: usize, r: usize, thick: usize, pct: u32, track: Color, col: Color) {
    use crate::image::vector::{cos, sin};
    let steps = 120usize;
    let pct = pct.min(100);
    for i in 0..steps {
        let a = -core::f32::consts::FRAC_PI_2 + (i as f32 / steps as f32) * core::f32::consts::PI * 2.0;
        let c = if (i as u32 * 100 / steps as u32) < pct { col } else { track };
        let fx = cx as f32 + cos(a) * r as f32;
        let fy = cy as f32 + sin(a) * r as f32;
        gfx::fill_circle(fx as usize, fy as usize, thick / 2, c);
    }
}

fn days_in_month(y: u64, m: u64) -> u64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Bitta oynani so'nggi mac uslubida chizadi (yumşoq soya, shaffof tana,
/// birlaşgan sarlavha, svetofor nuqtalari). Mazmun oyna içiga qirqiladi.
fn draw_window(win: &Window, active: bool, mx: usize, my: usize) {
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
    let hover = win.over_dots(mx, my);
    let dim = |c: Color| if active || hover { c } else { 0x4a5366 };
    gfx::fill_circle(dx, dy, dr, dim(0xff5f57));
    gfx::fill_circle(dx + step, dy, dr, dim(0xfebc2e));
    gfx::fill_circle(dx + step * 2, dy, dr, dim(0x28c840));
    if hover {
        // macOS kabi belgilar: × − +
        let (fx, fy) = (dx as f32, dy as f32);
        let q = dr as f32 * 0.5;
        let t = (dr as f32 * 0.16).max(0.9);
        let bar = |x0: f32, y0: f32, x1: f32, y1: f32, c: Color| {
            let (ddx, ddy) = (x1 - x0, y1 - y0);
            let l = crate::image::vector::fsqrt(ddx * ddx + ddy * ddy).max(0.001);
            let (nx, ny) = (-ddy / l * t, ddx / l * t);
            gfx::fill_poly(&[(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)], c);
        };
        bar(fx - q, fy - q, fx + q, fy + q, 0x6e1410);
        bar(fx - q, fy + q, fx + q, fy - q, 0x6e1410);
        let mx2 = fx + step as f32;
        bar(mx2 - q * 1.2, fy, mx2 + q * 1.2, fy, 0x7a5300);
        let zx = fx + step as f32 * 2.0;
        bar(zx - q * 1.2, fy, zx + q * 1.2, fy, 0x0b5a17);
        bar(zx, fy - q * 1.2, zx, fy + q * 1.2, 0x0b5a17);
    }
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
        Kind::Terminal => draw_terminal(win),
        Kind::About => draw_about(win),
        Kind::Settings => draw_settings(win),
        Kind::Browser => draw_browser(win),
        Kind::Trash => draw_trash(win),
        _ => draw_text_window(win),
    }
    gfx::reset_clip();
    // Ölçam tutqiçi (faol oynada, öng-pastki burçak)
    if active {
        let (ex, ey) = ((win.x + win.w) as f32 - 3.0, (win.y + win.h) as f32 - 3.0);
        for k in 1..=2 {
            let d = (body() as f32 * 0.33) * k as f32;
            gfx::fill_poly(&[(ex - d, ey), (ex, ey - d), (ex, ey - d + 1.4), (ex - d + 1.4, ey)], 0x6a7891);
        }
    }
}

/// Fayllar oynasi geometriyasi.
struct FilesGeo {
    sbw: usize,  // yon panel kengligi
    top: usize,  // mazmun boşi (sarlavha ostida)
    tx: usize,   // asboblar qatori x
    th: usize,   // asboblar qatori balandligi
    gx: usize,   // grid boşi
    gy: usize,
    gh: usize,
    cw: usize,   // katak ölçami
    ch: usize,
    cols: usize,
    item_h: usize, // yon panel qatori
    side_y: usize, // yon panel birinçi qatori
}

fn files_geo(win: &Window) -> FilesGeo {
    let u = body();
    let top = win.y + title_h();
    let sbw = u * 8;
    let tx = win.x + sbw;
    let th = u * 2;
    let gx = tx + u / 2;
    let gy = top + th + u / 2;
    let gw = (win.x + win.w).saturating_sub(gx + u / 2);
    let gh = (win.y + win.h).saturating_sub(gy + u / 2);
    let cw = u * 5;
    let ch = u * 5;
    FilesGeo {
        sbw, top, tx, th, gx, gy, gh, cw, ch,
        cols: (gw / cw).max(1),
        item_h: u * 7 / 4,
        side_y: top + u * 2,
    }
}

/// Bosilgan nuqtadagi grid elementi (Fayllar oynasi).
fn file_row_at(win: &Window, cx: usize, cy: usize) -> Option<usize> {
    let g = files_geo(win);
    if cx < g.gx || cy < g.gy || cx >= g.gx + g.cols * g.cw || cy >= g.gy + g.gh {
        return None;
    }
    let i = ((cy - g.gy) / g.ch) * g.cols + (cx - g.gx) / g.cw;
    if i < win.rows.len() { Some(i) } else { None }
}

/// Yon paneldagi bosilgan joy — papka yöli.
fn files_side_at(win: &Window, cx: usize, cy: usize) -> Option<String> {
    let g = files_geo(win);
    if cx < win.x || cx >= win.x + g.sbw || cy < g.side_y {
        return None;
    }
    let i = (cy - g.side_y) / g.item_h;
    if i == 0 {
        Some(String::from("~"))
    } else {
        win.side.get(i - 1).map(|n| alloc::format!("~/{}", n))
    }
}

/// "Orqaga" tugmasi bosildimi?
fn files_back(win: &Window, cx: usize, cy: usize) -> bool {
    let g = files_geo(win);
    let u = body();
    let (bx, by, bs) = (g.tx + u / 2, g.top + u / 4, u * 3 / 2);
    cx >= bx && cx < bx + bs && cy >= by && cy < by + bs
}

/// Matnni kenglikka sigʻdiradi (oşsa "…" bilan qisqartiradi).
fn fit_text(s: &str, maxw: usize, px: usize) -> String {
    if gfx::text_width(s, px) <= maxw {
        return String::from(s);
    }
    let ell = gfx::text_width("…", px);
    let mut out = String::new();
    for ch in s.chars() {
        out.push(ch);
        if gfx::text_width(&out, px) + ell > maxw {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}

const SIDE_COLORS: [Color; 6] = [0x2f88ff, 0x34c759, 0xff9f0a, 0xbf5af2, 0xff375f, 0x64d2ff];

/// Fayllar oynasi: yon panel + asboblar qatori + katta ikonali grid.
fn draw_files(win: &Window) {
    let u = body();
    let g = files_geo(win);
    let r = radius();
    let bottom = win.y + win.h;
    // --- Yon panel ---
    let sh = bottom - g.top;
    let scol = 0x151b27;
    fill_rect(win.x, g.top, g.sbw, sh.saturating_sub(r), scol);
    gfx::fill_round_rect(win.x, bottom.saturating_sub(2 * r), g.sbw, 2 * r, r, scol);
    fill_rect(win.x + r, bottom.saturating_sub(2 * r), g.sbw - r, 2 * r, scol);
    fill_rect(win.x + g.sbw, g.top, 1, sh, 0x2a3346);
    gfx::draw_text(win.x + u * 3 / 4, g.top + u / 2, "Joylar", 0x6f7f95, small() * 4 / 5);
    let n_items = 1 + win.side.len();
    for i in 0..n_items {
        let iy = g.side_y + i * g.item_h;
        if iy + g.item_h > bottom - u / 2 {
            break;
        }
        let (label, path) = if i == 0 {
            (String::from("Uy"), String::from("~"))
        } else {
            (win.side[i - 1].clone(), alloc::format!("~/{}", win.side[i - 1]))
        };
        if win.path == path {
            fill_round_rect_a(win.x + u / 3, iy, g.sbw - u * 2 / 3, g.item_h - 2, u / 3, 0x2f88ff, 70);
        }
        let isz = small();
        let ix = win.x + u * 3 / 4;
        let icy = iy + (g.item_h - isz) / 2;
        if i == 0 {
            // uy belgisi
            let c = ix + isz / 2;
            gfx::fill_poly(
                &[(c as f32, icy as f32), ((ix + isz) as f32, (icy + isz / 2) as f32), (ix as f32, (icy + isz / 2) as f32)],
                0x64d2ff,
            );
            fill_rect(ix + isz / 6, icy + isz / 2, isz * 2 / 3, isz / 2, 0x64d2ff);
        } else {
            let c = SIDE_COLORS[(i - 1) % SIDE_COLORS.len()];
            gfx::fill_round_rect(ix, icy + isz / 8, isz * 2 / 5, isz / 4, isz / 10, c);
            gfx::fill_round_rect(ix, icy + isz / 4, isz, isz * 3 / 4, isz / 6, c);
        }
        let lbl = fit_text(&label, g.sbw - u * 3, small() * 9 / 10);
        gfx::draw_text(ix + isz + u / 2, iy + (g.item_h - small()) / 2, &lbl, 0xdbe3ee, small() * 9 / 10);
    }
    // --- Asboblar qatori ---
    let u2 = u * 3 / 2;
    let (bx, by) = (g.tx + u / 2, g.top + u / 4);
    fill_round_rect_a(bx, by, u2, u2, u / 3, 0xffffff, 18);
    let cy0 = by + u2 / 2;
    let cxp = bx + u2 / 2;
    gfx::fill_poly(
        &[
            ((cxp + u / 5) as f32, (cy0 - u / 3) as f32),
            ((cxp + u / 5) as f32, (cy0 + u / 3) as f32),
            ((cxp - u / 4) as f32, cy0 as f32),
        ],
        if win.path == "~" || win.path == "/" { 0x4a5568 } else { 0xdbe3ee },
    );
    // yöl (breadcrumb)
    let crumb = if win.path == "~" {
        String::from("Uy")
    } else if let Some(rest) = win.path.strip_prefix("~/") {
        alloc::format!("Uy  /  {}", rest.replace('/', "  /  "))
    } else {
        win.path.clone()
    };
    gfx::draw_text(bx + u2 + u / 2, g.top + (g.th - small()) / 2, &crumb, 0xf0f4fa, small());
    let cnt = alloc::format!("{} ta element", win.rows.len());
    let cw = gfx::text_width(&cnt, small() * 4 / 5);
    gfx::draw_text((win.x + win.w).saturating_sub(cw + u), g.top + (g.th - small()) / 2 + 2, &cnt, 0x6f7f95, small() * 4 / 5);
    fill_rect(g.tx, g.top + g.th, win.w - g.sbw, 1, 0x2a3346);
    // --- Grid ---
    if win.rows.is_empty() {
        gfx::draw_text_centered(g.tx + (win.w - g.sbw) / 2, g.gy + g.gh / 3, "Papka böş", 0x6f7f95, body());
        return;
    }
    let isz = u * 3;
    let npx = small() * 9 / 10;
    for (i, (name, kind)) in win.rows.iter().enumerate() {
        let row = i / g.cols;
        let col = i % g.cols;
        let x0 = g.gx + col * g.cw;
        let y0 = g.gy + row * g.ch;
        if y0 + g.ch > g.gy + g.gh {
            break;
        }
        let (ix, iy) = (x0 + (g.cw - isz) / 2, y0 + u / 4);
        if let Some(Some(t)) = win.thumbs.get(i) {
            fill_round_rect_a(ix, iy, isz, isz, u / 3, 0xffffff, 14);
            let (tx, ty, tw, th) = gfx::blit_fit(ix + 2, iy + 2, isz - 4, isz - 4, t.w, t.h, &t.px);
            gfx::round_outline(tx.saturating_sub(1), ty.saturating_sub(1), tw + 2, th + 2, 2, 1, 0xd8dee9);
        } else {
            big_icon(*kind, name, ix, iy, isz);
        }
        let lbl = fit_text(name, g.cw - u / 3, npx);
        gfx::draw_text_centered(x0 + g.cw / 2, y0 + isz + u / 2, &lbl, 0xe7edf6, npx);
    }
}

/// Rasmdan kiçik eskiz (eng katta tomoni `max`, eng-yaqin namuna).
fn make_thumb(img: &crate::image::Image, max: usize) -> crate::image::Image {
    let (w, h) = (img.w.max(1), img.h.max(1));
    let (tw, th) = if w >= h { (max, (h * max / w).max(1)) } else { ((w * max / h).max(1), max) };
    let (tw, th) = if w <= max && h <= max { (w, h) } else { (tw, th) };
    let mut px = Vec::with_capacity(tw * th);
    for y in 0..th {
        let sy = y * h / th;
        for x in 0..tw {
            px.push(img.px[sy * w + x * w / tw]);
        }
    }
    crate::image::Image { w: tw, h: th, px }
}

/// Fayl kengaytmasi (katta harflarda, 4 belgigaça).
fn ext_label(name: &str) -> String {
    let e = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    e.chars().take(4).collect::<String>().to_ascii_uppercase()
}

/// Katta fayl/papka ikonasi (grid uçun).
fn big_icon(kind: FKind, name: &str, x: usize, y: usize, s: usize) {
    if kind == FKind::Dir {
        // macOS uslubidagi koʻk papka
        gfx::fill_round_rect(x + s / 12, y + s / 6, s * 2 / 5, s / 4, s / 12, 0x3f8fe8);
        gfx::fill_round_rect(x + s / 12, y + s / 4, s * 5 / 6, s * 3 / 5, s / 10, 0x3f8fe8);
        gfx::fill_round_rect(x + s / 12, y + s / 3, s * 5 / 6, s * 13 / 20, s / 10, 0x62adff);
        gfx::fill_round_rect_a(x + s / 12, y + s / 3, s * 5 / 6, s / 6, s / 10, 0xffffff, 40);
        return;
    }
    // Hujjat varaqi
    let pw = s * 3 / 5;
    let ph = s * 5 / 6;
    let px0 = x + (s - pw) / 2;
    let py0 = y + s / 12;
    gfx::fill_round_rect(px0, py0, pw, ph, s / 14, 0xeef2f7);
    let fold = pw / 4;
    gfx::fill_poly(
        &[
            ((px0 + pw - fold) as f32, py0 as f32),
            ((px0 + pw) as f32, (py0 + fold) as f32),
            ((px0 + pw - fold) as f32, (py0 + fold) as f32),
        ],
        0xc4cfdd,
    );
    let c = kind.color();
    let inner_x = px0 + pw / 6;
    let inner_w = pw * 2 / 3;
    match kind {
        FKind::Image => {
            let ih = ph * 2 / 5;
            let iy = py0 + ph / 6;
            gfx::gradient_rect(inner_x, iy, inner_w, ih, 0x8fd0ff, 0xffd59e);
            hill(inner_x, iy, inner_w, ih, inner_w / 2, ih * 3 / 5, 0x2e8b58);
        }
        FKind::Audio => {
            let cx = px0 + pw / 2;
            let cy = py0 + ph * 3 / 8;
            gfx::fill_circle(cx - s / 12, cy + s / 12, s / 14, 0xff7a2f);
            fill_rect(cx - s / 12 + s / 16, cy - s / 8, (s / 30).max(1), s / 5, 0xff7a2f);
        }
        FKind::Video => {
            let cx = px0 + pw / 2;
            let cy = py0 + ph * 3 / 8;
            let t = s / 9;
            gfx::fill_poly(
                &[((cx - t / 2) as f32, (cy - t) as f32), ((cx + t) as f32, cy as f32), ((cx - t / 2) as f32, (cy + t) as f32)],
                0x8257ff,
            );
        }
        _ => {
            for k in 0..4 {
                fill_rect(inner_x, py0 + ph / 6 + k * ph / 10, inner_w * (4 - k % 2) / 4, (s / 30).max(1), 0x9aa8bb);
            }
        }
    }
    // Rangli belgi (kengaytma)
    let ext = ext_label(name);
    if !ext.is_empty() {
        let bh = s / 5;
        let by = py0 + ph - bh - s / 14;
        gfx::fill_round_rect(px0 + pw / 10, by, pw * 4 / 5, bh, bh / 3, c);
        gfx::draw_text_centered(px0 + pw / 2, by + bh / 10, &ext, OQ, bh * 4 / 5);
    }
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

/// Oddiy matnli oyna (xabarlar).
fn draw_text_window(win: &Window) {
    let px = small();
    let lh = gfx::line_height(px, Font::Ui).max(px + 3);
    let pad = body();
    let mut ty = win.content_top() + pad / 3;
    for line in &win.lines {
        if ty + lh > win.y + win.h - pad / 2 {
            break;
        }
        gfx::draw_text(win.x + pad, ty, line, 0xe7edf6, px);
        ty += lh;
    }
}

/// Terminal qatorini ranglab chizadi: user@zamin (yashil), yöl (koʻk).
fn draw_term_line(x: usize, y: usize, s: &str, px: usize) -> usize {
    let base_col = 0xd3dae5;
    if let (Some(at), Some(gt)) = (s.find("@zamin:"), s.find("> ")) {
        if at < gt {
            let colon = at + "@zamin".len();
            let mut cx = gfx::draw_text_mono(x, y, &s[..colon], 0x5be37d, px);
            cx = gfx::draw_text_mono(cx, y, ":", base_col, px);
            cx = gfx::draw_text_mono(cx, y, &s[colon + 1..gt], 0x63a8ff, px);
            cx = gfx::draw_text_mono(cx, y, "> ", base_col, px);
            return gfx::draw_text_mono(cx, y, &s[gt + 2..], 0xf2f5fa, px);
        }
    }
    gfx::draw_text_mono(x, y, s, base_col, px)
}

/// Terminal oynasi: toʻq fon, rangli prompt, blok kursor.
fn draw_terminal(win: &Window) {
    let px = small();
    let lh = gfx::line_height(px, Font::Mono).max(px + 3);
    let pad = body() * 3 / 4;
    let top = win.y + title_h();
    let r = radius();
    let bottom = win.y + win.h;
    fill_rect(win.x + 1, top, win.w.saturating_sub(2), bottom.saturating_sub(top + r), 0x0e121a);
    gfx::fill_round_rect(win.x + 1, bottom.saturating_sub(2 * r + 1), win.w.saturating_sub(2), 2 * r, r, 0x0e121a);
    let avail = (win.y + win.h).saturating_sub(top + pad * 2);
    let max_lines = (avail / lh).max(1);
    // faqat oxirgi satrlar (pastga yopişgan)
    let total = win.lines.len() + 1;
    let skip = total.saturating_sub(max_lines);
    let mut ty = top + pad;
    for line in win.lines.iter().skip(skip) {
        draw_term_line(win.x + pad, ty, line, px);
        ty += lh;
    }
    let cur = String::from_utf8_lossy(&win.cur);
    let full = alloc::format!("{}{}", cur, win.input);
    let ex = draw_term_line(win.x + pad, ty, &full, px);
    gfx::blend_rect(ex, ty, gfx::mono_advance(px), px + 2, 0x5be37d, 190);
}

/// Soat oynasi: analog siferblat + raqamli vaqt va sana.
fn draw_clock(win: &Window) {
    use crate::image::vector::{cos, sin};
    let t = time::now();
    let top = win.y + title_h();
    let avail = (win.y + win.h).saturating_sub(top);
    let rr = (avail * 2 / 5).min(win.w / 5);
    let cx = win.x + body() + rr + body() / 2;
    let cy = top + avail / 2;
    gfx::fill_circle(cx, cy, rr + 3, 0x3a4458);
    gfx::fill_circle(cx, cy, rr, 0xf4f6fa);
    for k in 0..12 {
        let a = k as f32 * core::f32::consts::PI / 6.0;
        let d = rr as f32 * 0.82;
        let big = k % 3 == 0;
        gfx::fill_circle(
            (cx as f32 + cos(a) * d) as usize,
            (cy as f32 + sin(a) * d) as usize,
            if big { (rr / 18).max(2) } else { (rr / 30).max(1) },
            0x2b3240,
        );
    }
    let hand = |ang: f32, len: f32, wid: f32, col: Color| {
        let a = ang - core::f32::consts::FRAC_PI_2;
        let (dx, dy) = (cos(a), sin(a));
        let (nx, ny) = (-dy * wid, dx * wid);
        let (fx, fy) = (cx as f32, cy as f32);
        gfx::fill_poly(
            &[
                (fx + nx - dx * wid * 2.0, fy + ny - dy * wid * 2.0),
                (fx + dx * len + nx * 0.4, fy + dy * len + ny * 0.4),
                (fx + dx * len - nx * 0.4, fy + dy * len - ny * 0.4),
                (fx - nx - dx * wid * 2.0, fy - ny - dy * wid * 2.0),
            ],
            col,
        );
    };
    let tau = core::f32::consts::PI * 2.0;
    let hf = (t.hour % 12) as f32 + t.minute as f32 / 60.0;
    hand(hf / 12.0 * tau, rr as f32 * 0.5, rr as f32 * 0.05, 0x1b2130);
    hand((t.minute as f32 + t.second as f32 / 60.0) / 60.0 * tau, rr as f32 * 0.72, rr as f32 * 0.035, 0x1b2130);
    hand(t.second as f32 / 60.0 * tau, rr as f32 * 0.8, rr as f32 * 0.015, 0xff453a);
    gfx::fill_circle(cx, cy, (rr / 16).max(2), 0xff453a);
    // raqamli
    let tx = cx + rr + body() * 3 / 2;
    let big = (avail / 4).clamp(24, 72);
    gfx::draw_text(tx, cy.saturating_sub(big), &alloc::format!("{:02}:{:02}", t.hour, t.minute), 0xf4f8ff, big);
    let sana = alloc::format!("{}, {}-{} {}", weekday_short(t.year, t.month, t.day), t.day, month_name(t.month), t.year);
    gfx::draw_text(tx, cy + big / 4, &sana, 0x8ab4ff, small());
}

/// Matn muharriri: asboblar qatori (fayl nomi + Saqla), qator raqamlari, kursor.
fn draw_editor(win: &Window) {
    let px = small();
    let lh = gfx::line_height(px, Font::Mono).max(px + 3);
    let u = body();
    // Asboblar qatori
    let (bx, by, bw, bh) = win.save_btn();
    let fname = win.path.rsplit('/').next().unwrap_or("");
    let status = String::from_utf8_lossy(&win.cur);
    let left = if status.is_empty() { String::from(fname) } else { alloc::format!("{}  ·  {}", fname, status) };
    gfx::draw_text(win.x + u, by + (bh.saturating_sub(small())) / 2, &left, 0x9fb0c3, small());
    gfx::fill_round_rect(bx, by, bw, bh, bh / 2, 0x30c85a);
    gfx::draw_text_centered(bx + bw / 2, by + (bh.saturating_sub(small())) / 2, "Saqla", 0x06230f, small());
    let top = by + bh + u / 3;
    fill_rect(win.x + 1, top - 1, win.w.saturating_sub(2), 1, 0x2a3346);
    // Gutter
    let adv = gfx::mono_advance(px);
    let gut = adv * 3 + u / 2;
    let bottom = (win.y + win.h).saturating_sub(radius());
    fill_rect(win.x + 1, top, gut, bottom.saturating_sub(top), 0x161b26);
    let mut ty = top + u / 3;
    let tx = win.x + gut + u / 2;
    let mut n = 1usize;
    for line in &win.lines {
        if ty + lh > win.y + win.h - u / 2 {
            break;
        }
        let num = alloc::format!("{:>3}", n);
        gfx::draw_text_mono(win.x + u / 4, ty, &num, 0x55627a, px);
        gfx::draw_text_mono(tx, ty, line, 0xe7edf6, px);
        ty += lh;
        n += 1;
    }
    if ty + lh <= win.y + win.h - u / 2 {
        gfx::blend_rect(win.x + gut, ty.saturating_sub(1), win.w.saturating_sub(gut + 2), lh, 0x2f88ff, 28);
        let num = alloc::format!("{:>3}", n);
        gfx::draw_text_mono(win.x + u / 4, ty, &num, 0x9fb0c3, px);
        let w = gfx::text_width_mono(&win.input, px);
        gfx::draw_text_mono(tx, ty, &win.input, 0xf2f5fa, px);
        fill_rect(tx + w + 1, ty, (px / 10).max(2), px + 2, 0x7fb0ea);
    }
}

/// Zamin logosi (olmos-yulduz), markaz va ölçam böyiça.
fn zamin_logo(cx: usize, cy: usize, s: usize) {
    let h = s / 2;
    let (fx, fy) = (cx as f32, cy as f32);
    let hf = h as f32;
    // tashqi yulduz: ikki kvadrat (biri 45° burilgan)
    gfx::fill_poly(&[(fx, fy - hf), (fx + hf, fy), (fx, fy + hf), (fx - hf, fy)], 0x2fbf5f);
    let q = hf * 0.72;
    gfx::fill_poly(&[(fx - q, fy - q), (fx + q, fy - q), (fx + q, fy + q), (fx - q, fy + q)], 0x2fbf5f);
    let i = hf * 0.55;
    gfx::fill_poly(&[(fx, fy - i), (fx + i, fy), (fx, fy + i), (fx - i, fy)], 0x0f2a1a);
    // tog' çöqqisi
    let m = hf * 0.4;
    gfx::fill_poly(&[(fx - m, fy + m * 0.6), (fx - m * 0.1, fy - m * 0.5), (fx + m * 0.3, fy + m * 0.1), (fx + m * 0.6, fy - m * 0.2), (fx + m, fy + m * 0.6)], 0x58e06a);
}

/// "Zamin OS haqida" oynasi.
fn draw_about(win: &Window) {
    let u = body();
    let cx = win.x + win.w / 2;
    let top = win.y + title_h();
    let ls = u * 4;
    zamin_logo(cx, top + u + ls / 2, ls);
    let mut y = top + u * 2 + ls;
    gfx::draw_text_centered(cx, y, "Zamin OS", 0xffffff, u * 3 / 2);
    y += u * 2;
    gfx::draw_text_centered(cx, y, &alloc::format!("Versiya {}", env!("CARGO_PKG_VERSION")), 0x5be37d, small());
    y += u * 3 / 2;
    for l in ["Rust'da noldan yozilgan ARM64 OS", "Oçiq · Xavfsiz · Tez · Barqaror", "Dasturçi: Abbosjon Dev", "zamin.app · info@zamin.app"] {
        gfx::draw_text_centered(cx, y, l, 0xaebccf, small() * 9 / 10);
        y += small() + u / 3;
    }
}

/// Sozlamalar oynasi: çapda bölimlar, öngda tizim maʼlumoti.
fn draw_settings(win: &Window) {
    let u = body();
    let top = win.y + title_h();
    let sbw = u * 9;
    let r = radius();
    let bottom = win.y + win.h;
    fill_rect(win.x, top, sbw, (bottom - top).saturating_sub(r), 0x151b27);
    gfx::fill_round_rect(win.x, bottom.saturating_sub(2 * r), sbw, 2 * r, r, 0x151b27);
    fill_rect(win.x + r, bottom.saturating_sub(2 * r), sbw - r, 2 * r, 0x151b27);
    fill_rect(win.x + sbw, top, 1, bottom - top, 0x2a3346);
    let cats = [
        ("Umumiy", 0x8a93a6),
        ("Ekran", 0x2f88ff),
        ("Övoz", 0xff375f),
        ("Tarmoq", 0x34c759),
        ("Foydalanuvçilar", 0xff9f0a),
        ("Xavfsizlik", 0xbf5af2),
    ];
    let ih = u * 7 / 4;
    for (i, (name, col)) in cats.iter().enumerate() {
        let iy = top + u / 2 + i * ih;
        if iy + ih > bottom - u / 2 {
            break;
        }
        if i == 0 {
            fill_round_rect_a(win.x + u / 3, iy, sbw - u * 2 / 3, ih - 2, u / 3, 0x2f88ff, 70);
        }
        let isz = small();
        gfx::fill_round_rect(win.x + u * 2 / 3, iy + (ih - isz) / 2, isz, isz, isz / 4, *col);
        let lbl = fit_text(name, sbw - u * 3, small() * 9 / 10);
        gfx::draw_text(win.x + u * 2 / 3 + isz + u / 2, iy + (ih - small()) / 2, &lbl, 0xdbe3ee, small() * 9 / 10);
    }
    // Öng panel
    let x = win.x + sbw + u;
    let mut y = top + u * 2 / 3;
    gfx::draw_text(x, y, "Umumiy", 0xffffff, u * 6 / 5);
    y += u * 2;
    let (sw, sh) = gfx::size();
    let (free_p, total_p) = crate::memory::frame::stats();
    let user = accounts::current().map(|a| a.username).unwrap_or_default();
    let rows = [
        ("Tizim", String::from("Zamin OS")),
        ("Versiya", String::from(env!("CARGO_PKG_VERSION"))),
        ("Foydalanuvçi", user),
        ("Protsessor", String::from("AArch64 · 4 yadro")),
        ("Xotira", alloc::format!("{} / {} MB", (total_p - free_p) * 4 / 1024, total_p * 4 / 1024)),
        ("Ekran", alloc::format!("{} x {}", sw, sh)),
    ];
    let rw = (win.x + win.w).saturating_sub(x + u);
    let rh = u * 3 / 2;
    fill_round_rect_a(x, y, rw, rh * rows.len(), u / 2, 0xffffff, 12);
    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = y + i * rh;
        if i > 0 {
            fill_rect(x + u / 2, ry, rw - u, 1, 0x2a3346);
        }
        let ty = ry + (rh - small()) / 2;
        gfx::draw_text(x + u / 2, ty, k, 0xdbe3ee, small() * 9 / 10);
        let vw = gfx::text_width(v, small() * 9 / 10);
        gfx::draw_text((x + rw).saturating_sub(vw + u / 2), ty, v, 0x8a9ab0, small() * 9 / 10);
    }
}

/// Brauzer oynasi: manzil qatori + boşlang'iç sahifa.
fn draw_browser(win: &Window) {
    let u = body();
    let top = win.y + title_h();
    // asboblar
    let by = top + u / 3;
    let bh = u * 3 / 2;
    for k in 0..2 {
        let bx = win.x + u / 2 + k * (bh + u / 4);
        let c = bx + bh / 2;
        let m = by + bh / 2;
        let d = if k == 0 { -1.0f32 } else { 1.0 };
        gfx::fill_poly(
            &[
                ((c as f32) - d * (u as f32) / 5.0, (m - u / 3) as f32),
                ((c as f32) - d * (u as f32) / 5.0, (m + u / 3) as f32),
                ((c as f32) + d * (u as f32) / 4.0, m as f32),
            ],
            0x6f7f95,
        );
    }
    let ax = win.x + u / 2 + 2 * (bh + u / 4) + u / 2;
    let aw = (win.x + win.w).saturating_sub(ax + u);
    gfx::fill_round_rect(ax, by, aw, bh, bh / 2, 0x10151f);
    gfx::round_outline(ax, by, aw, bh, bh / 2, 1, 0x2a3346);
    gfx::fill_circle(ax + u * 3 / 4, by + bh / 2, (u / 5).max(2), 0x34c759);
    gfx::draw_text(ax + u * 3 / 2, by + (bh - small()) / 2, "zamin.app", 0xdbe3ee, small());
    let ptop = by + bh + u / 3;
    fill_rect(win.x + 1, ptop, win.w.saturating_sub(2), 1, 0x2a3346);
    // sahifa
    let cx = win.x + win.w / 2;
    let ls = u * 3;
    zamin_logo(cx, ptop + u * 3 / 2 + ls / 2, ls);
    let mut y = ptop + u * 2 + ls;
    gfx::draw_text_centered(cx, y, "Zamin Brauzer", 0xffffff, u * 6 / 5);
    y += u * 2;
    let sw2 = (win.w * 3 / 5).max(u * 10);
    gfx::fill_round_rect(cx - sw2 / 2, y, sw2, bh, bh / 2, 0x10151f);
    gfx::round_outline(cx - sw2 / 2, y, sw2, bh, bh / 2, 1, 0x34405a);
    gfx::draw_text(cx - sw2 / 2 + u, y + (bh - small()) / 2, "Qidiriş yoki manzil…", 0x6f7f95, small());
    y += bh + u * 3 / 2;
    let tiles = [("zamin.app", 0x34c759), ("GitHub", 0x8a93a6), ("Hujjatlar", 0x2f88ff), ("Yangiliklar", 0xff9f0a)];
    let ts = u * 5 / 2;
    let tg = u * 9 / 2;
    let total = tiles.len() * ts + (tiles.len() - 1) * (tg - ts);
    let mut tx = cx.saturating_sub(total / 2);
    for (name, col) in tiles {
        gfx::fill_round_rect(tx, y, ts, ts, ts / 4, col);
        gfx::draw_text_centered(tx + ts / 2, y + ts / 4, &name[..1], OQ, ts / 2);
        gfx::draw_text_centered(tx + ts / 2, y + ts + u / 4, name, 0xaebccf, small() * 4 / 5);
        tx += tg;
    }
}

/// Savat oynasi (böş holat).
fn draw_trash(win: &Window) {
    let u = body();
    let cx = win.x + win.w / 2;
    let top = win.y + title_h();
    let s = u * 4;
    let tw = s * 3 / 5;
    let th = s * 2 / 3;
    let x = cx - tw / 2;
    let y = top + u * 3 / 2;
    gfx::fill_round_rect(x - tw / 8, y, tw * 5 / 4, s / 10, s / 30, 0x8a93a6);
    gfx::fill_round_rect(x, y + s / 8, tw, th, tw / 8, 0x5a6476);
    for k in 0..3 {
        fill_rect(x + tw / 4 + k * tw / 4 - 1, y + s / 8 + th / 6, (s / 30).max(2), th * 2 / 3, 0x3a4252);
    }
    gfx::draw_text_centered(cx, y + s / 8 + th + u, "Savat böş", 0xe7edf6, body());
    gfx::draw_text_centered(cx, y + s / 8 + th + u * 5 / 2, "Öçirilgan fayllar şu yerda turadi", 0x6f7f95, small() * 9 / 10);
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

/// Yumaloq burçakli vertikal gradient (qatorma-qator, burçak çeti hisoblanadi).
fn gradient_round_rect(x: usize, y: usize, w: usize, h: usize, r: usize, top: Color, bot: Color) {
    let lerp = |a: u32, b: u32, t: u32, n: u32| -> u32 {
        let ch = |sh: u32| -> u32 {
            let (ca, cb) = ((a >> sh) & 0xff, (b >> sh) & 0xff);
            ((ca * (n - t) + cb * t) / n) << sh
        };
        ch(16) | ch(8) | ch(0)
    };
    let r = r.min(w / 2).min(h / 2);
    for dy in 0..h {
        let inset = if dy < r || dy >= h - r {
            let d = if dy < r { r - dy } else { dy + 1 - (h - r) } as f32 - 0.5;
            let rr = r as f32;
            (rr - crate::image::vector::fsqrt((rr * rr - d * d).max(0.0))) as usize
        } else {
            0
        };
        let c = lerp(top, bot, dy as u32, h.max(1) as u32);
        fill_rect(x + inset, y + dy, w.saturating_sub(inset * 2), 1, c);
    }
}

/// Musiqa pleeri: muqova, nom, toʻlqin (ijro qilingan qismi rangli), tugma.
fn draw_audio(win: &Window) {
    let Some(clip) = &win.clip else { return };
    let u = body();
    let (a, x0, w0, ct) = win.audio_geo();
    // Muqova
    let ax = win.x + u;
    gradient_round_rect(ax, ct, a, a, u / 2, 0xff8a3d, 0x7a3cff);
    let (nc, ny) = (ax + a / 2, ct + a / 2);
    let s = a / 2;
    gfx::fill_circle(nc - s / 4, ny + s / 4, s / 7, OQ);
    gfx::fill_circle(nc + s / 4, ny + s / 6, s / 7, OQ);
    fill_rect(nc - s / 4 + s / 7 - 2, ny - s / 3, (s / 14).max(2), s * 7 / 12, OQ);
    fill_rect(nc + s / 4 + s / 7 - 2, ny - s * 2 / 5, (s / 14).max(2), s * 7 / 12, OQ);
    gfx::fill_poly(
        &[
            ((nc - s / 4 + s / 7 - 2) as f32, (ny - s / 3) as f32),
            ((nc + s / 4 + s / 7) as f32, (ny - s * 2 / 5) as f32),
            ((nc + s / 4 + s / 7) as f32, (ny - s * 2 / 5 + s / 7) as f32),
            ((nc - s / 4 + s / 7 - 2) as f32, (ny - s / 3 + s / 7) as f32),
        ],
        OQ,
    );
    // Nom va maʼlumot
    let name = win.path.rsplit('/').next().unwrap_or("");
    let stem = name.rsplit_once('.').map(|(n, _)| n).unwrap_or(name);
    let fmt = name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
    gfx::draw_text(x0, ct, &fit_text(stem, w0, u * 6 / 5), 0xffffff, u * 6 / 5);
    let dur = clip.duration_ms();
    let info = alloc::format!(
        "{} · {} Hz · {} kanal · {}:{:02}",
        fmt,
        clip.rate,
        clip.channels,
        dur / 60000,
        (dur / 1000) % 60
    );
    gfx::draw_text(x0, ct + u * 3 / 2 + 2, &fit_text(&info, w0, small() * 9 / 10), 0x8ea0b6, small() * 9 / 10);
    // Toʻlqin
    let (_, _, r) = win.play_btn();
    let wy = ct + u * 3;
    let wh = a.saturating_sub(u * 3 + r * 2 + u);
    let mid = wy + wh / 2;
    let n = win.env.len().max(1);
    let played = if dur > 0 { win.play_pos.min(dur) as usize * w0 / dur as usize } else { 0 };
    let bw = (w0 / n).max(1);
    let peak = win.env.iter().map(|&(mn, mx)| (mn as i32).abs().max((mx as i32).abs())).max().unwrap_or(1).max(1) as usize;
    for (i, &(mn, mx)) in win.env.iter().enumerate() {
        let bx = x0 + i * w0 / n;
        let amp = ((mx as i32).abs().max((mn as i32).abs()) as usize * (wh / 2) * 9 / 10 / peak).max(1);
        let col = if bx < x0 + played { 0x34c759 } else { 0x3a4458 };
        gfx::fill_rect(bx, mid - amp, bw.saturating_sub(1).max(1), amp * 2, col);
    }
    // Tugma + vaqt
    let (px, py, r) = win.play_btn();
    gfx::fill_circle(px, py, r, if win.playing { 0xffd426 } else { 0x34c759 });
    if win.playing {
        fill_rect(px - r / 3, py - r / 2, r / 4, r, 0x06230f);
        fill_rect(px + r / 8, py - r / 2, r / 4, r, 0x06230f);
    } else {
        let t = r * 2 / 5;
        gfx::fill_poly(
            &[((px - t / 2) as f32, (py - t) as f32), ((px + t) as f32, py as f32), ((px - t / 2) as f32, (py + t) as f32)],
            0x06230f,
        );
    }
    let pos = win.play_pos.min(dur);
    let tt = alloc::format!("{}:{:02} / {}:{:02}", pos / 60000, (pos / 1000) % 60, dur / 60000, (dur / 1000) % 60);
    gfx::draw_text(px + r + u, py.saturating_sub(small() / 2), &tt, 0xe7edf6, small());
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
        let open = windows.iter().any(|w| win_of_launcher(w, l.kind));
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
            // Zamin logosi — olmos-yulduz
            gfx::fill_poly(
                &[
                    (c as f32, (y + s / 5) as f32),
                    ((x + s * 4 / 5) as f32, (y + m) as f32),
                    (c as f32, (y + s * 4 / 5) as f32),
                    ((x + s / 5) as f32, (y + m) as f32),
                ],
                OQ,
            );
            gfx::fill_poly(
                &[
                    (c as f32, (y + s * 7 / 20) as f32),
                    ((x + s * 13 / 20) as f32, (y + m) as f32),
                    (c as f32, (y + s * 13 / 20) as f32),
                    ((x + s * 7 / 20) as f32, (y + m) as f32),
                ],
                color,
            );
        }
        Kind::Audio | Kind::Music => {
            // nota belgisi
            gfx::fill_circle(c - s / 6, y + s * 2 / 3, s / 9, OQ);
            gfx::fill_circle(c + s / 5, y + s * 3 / 5, s / 9, OQ);
            fill_rect(c - s / 6 + s / 9, y + s / 4, (s / 16).max(1), s * 5 / 12, OQ);
            fill_rect(c + s / 5 + s / 9, y + s / 5, (s / 16).max(1), s * 2 / 5, OQ);
            fill_rect(c - s / 6 + s / 9, y + s / 4, s * 13 / 36, (s / 16).max(1), OQ);
        }
        Kind::Browser => {
            let rr = s * 7 / 20;
            gfx::fill_circle(c, y + m, rr, 0xdff0ff);
            gfx::fill_circle(c, y + m, rr * 9 / 10, color);
            // meridianlar
            gfx::round_outline(c - rr * 2 / 5, y + m - rr, rr * 4 / 5, rr * 2, rr * 2 / 5, (s / 24).max(1), 0xdff0ff);
            fill_rect(c - rr, y + m, rr * 2, (s / 24).max(1), 0xdff0ff);
            // kompas strelkasi
            gfx::fill_poly(
                &[
                    (c as f32, (y + m - rr / 2) as f32),
                    ((c + rr / 4) as f32, (y + m + rr / 5) as f32),
                    (c as f32, (y + m) as f32),
                    ((c - rr / 4) as f32, (y + m + rr / 5) as f32),
                ],
                0xff5f57,
            );
        }
        Kind::Code => {
            gfx::draw_text_centered(c, y + s / 4, "</>", OQ, s * 9 / 20);
        }
        Kind::Video => {
            let pr = s / 4;
            gfx::fill_circle(c, y + m, s * 7 / 20, 0xffffff);
            gfx::fill_poly(
                &[
                    ((c - pr / 2) as f32, (y + m - pr) as f32),
                    ((c + pr) as f32, (y + m) as f32),
                    ((c - pr / 2) as f32, (y + m + pr) as f32),
                ],
                color,
            );
        }
        Kind::Settings => {
            // tişli gʻildirak (oddiy)
            let rr = s * 3 / 10;
            for k in 0..8 {
                let a = k as f32 * core::f32::consts::FRAC_PI_4;
                let fx = c as f32 + crate::image::vector::cos(a) * (rr as f32 * 1.35);
                let fy = (y + m) as f32 + crate::image::vector::sin(a) * (rr as f32 * 1.35);
                gfx::fill_circle(fx as usize, fy as usize, s / 12, 0xe7edf6);
            }
            gfx::fill_circle(c, y + m, rr, 0xe7edf6);
            gfx::fill_circle(c, y + m, rr / 2, color);
        }
        Kind::Trash => {
            let tw = s * 2 / 5;
            let th = s * 9 / 20;
            let tx = c - tw / 2;
            let tyy = y + s / 2 - th / 3;
            gfx::fill_round_rect(tx - tw / 6, tyy - s / 10, tw * 4 / 3, s / 10, s / 24, 0x3a4252);
            gfx::fill_round_rect(tx, tyy, tw, th, tw / 8, 0x4a5366);
            for k in 0..3 {
                fill_rect(tx + tw / 4 + k * tw / 4, tyy + th / 6, (s / 24).max(1), th * 2 / 3, 0x2b3240);
            }
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
