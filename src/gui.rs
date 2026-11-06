//! Oynalar menejeri: siljitiladigan oynalar, grafik terminal, vazifalar paneli.
//! Grafika (virtio-gpu) va kiriş (virtio-input) bölganda işlaydi.

use crate::drivers::virtio_input as input;
use crate::gfx::{self, Font, *};
use crate::task::{Handle, SpawnOptions};
use crate::{accounts, fs, pipe, task, time};
use alloc::boxed::Box;
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
    Calc,
    Monitor,
    Info,
    Puzzle,
    Paint,
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
    tab: usize,         // Sozlamalar: tanlangan bölim
    hidden: bool,       // dokka yashirilgan (sariq tugma)
    saved: Option<(usize, usize, usize, usize)>, // kattalashtirişdan oldingi (x, y, w, h)
    fsel: Option<usize>, // Fayllar/Savat: tanlangan element; Monitor: tanlangan PID
    lclick: (u64, usize), // ikki marta bosiş: (vaqt, element)
    rename: Option<String>, // Fayllar: nomini özgartiriş maydoni
    video: bool,        // Rasm oynasi video ijro qilyaptimi
    crow: usize,        // Muharrir: kursor qatori
    ccol: usize,        // Muharrir: kursor ustuni (belgilarda)
    scroll: usize,      // Muharrir: birinçi körinadigan qator
    term_scroll: usize, // Terminal: pastdan necha vizual qator yuqoriga aylantirilgan (0 — eng past)
    dirty: bool,        // Muharrir: saqlanmagan özgarişlar
    tiles: Vec<u8>,     // Boşqotirma: 16 ta katak (0 — böş)
    web: Option<Box<Web>>, // Brauzer holati (faol tab)
    web_tabs: Vec<Box<Web>>, // Brauzer: boşqa (nofaol) tablar; faol tab örni — `tab`
    pen: Option<(usize, usize)>, // Chizma: oxirgi nuqta (tuvalda)
    hist: crate::edit::EditHistory, // Muharrir: bekor qiliş/qaytariş tarixi
    find: Option<String>, // Muharrir: qidiruv maydoni (oçiq bölsa — so'rov)
    find_at: usize,     // Muharrir: joriy moslik indeksi
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

const LAUNCHERS: [Launcher; 14] = [
    Launcher { label: "Zamin", kind: Kind::About, color: 0x34c759 },
    Launcher { label: "Brauzer", kind: Kind::Browser, color: 0x1e88e5 },
    Launcher { label: "Fayllar", kind: Kind::Files, color: 0x2f88ff },
    Launcher { label: "Terminal", kind: Kind::Terminal, color: 0x2b3440 },
    Launcher { label: "Kod", kind: Kind::Code, color: 0x7c5cff },
    Launcher { label: "Kalkulyator", kind: Kind::Calc, color: 0xff9f0a },
    Launcher { label: "Tizim monitori", kind: Kind::Monitor, color: 0x1f9d55 },
    Launcher { label: "Boşqotirma", kind: Kind::Puzzle, color: 0xe0457b },
    Launcher { label: "Chizma", kind: Kind::Paint, color: 0x2bb3a8 },
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
        tab: 0,
        hidden: false,
        saved: None,
        fsel: None,
        lclick: (0, usize::MAX),
        rename: None,
        video: false,
        crow: 0,
        ccol: 0,
        scroll: 0,
        term_scroll: 0,
        dirty: false,
        tiles: Vec::new(),
        web: None,
        web_tabs: Vec::new(),
        pen: None,
        hist: crate::edit::EditHistory::new(),
        find: None,
        find_at: 0,
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
                fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir && !e.name.starts_with('.')).map(|e| e.name).collect();
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
            lines.push(input);
            let mut win = base(x, y, w, h, kind, String::from("Matn — qaydlar.txt"), lines);
            win.path = String::from(NOTES_PATH);
            win.crow = win.lines.len() - 1;
            win.ccol = win.lines[win.crow].chars().count();
            editor_fix(&mut win);
            win
        }
        Kind::Clock => {
            let (w, h) = (u * 24, u * 15);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Soat"), Vec::new());
            win.frame = 300; // taymer: 5 daqiqa
            win
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
            let (w, h) = (u * 40, u * 30);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Zamin Brauzer"), Vec::new());
            web_go(&mut win, "zamin://bosh", false);
            win
        }
        Kind::Paint => {
            let (w, h) = (u * 38, u * 29);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Chizma"), Vec::new());
            paint_new(&mut win);
            win
        }
        Kind::Settings => {
            let (w, h) = (u * 34, u * 24);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Sozlamalar"), Vec::new());
            // Fon eskizlari (bir marta yasaladi)
            let (tw, th) = (192, 108);
            let pt = PHOTO_THUMB.lock().clone();
            win.thumbs = (0..wall_count()).map(|i| build_wallpaper(i, tw, th, &pt)).collect();
            win
        }
        Kind::Trash => {
            let (w, h) = (u * 30, u * 18);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Savat"), Vec::new());
            load_trash(&mut win);
            win
        }
        Kind::Calc => {
            let (w, h) = (u * 15, u * 22);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Kalkulyator"), Vec::new());
            win.input = String::from("0");
            win
        }
        Kind::Puzzle => {
            let (w, h) = (u * 20, u * 24);
            let (x, y) = place(w, h);
            let mut win = base(x, y, w, h, kind, String::from("Boşqotirma"), Vec::new());
            puzzle_new(&mut win);
            win
        }
        Kind::Monitor => {
            let (w, h) = (u * 34, u * 25);
            let (x, y) = place(w, h);
            base(x, y, w, h, kind, String::from("Tizim monitori"), Vec::new())
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
    entries.retain(|e| !e.name.starts_with('.')); // yaşirin fayllar (.Savat va h.k.)
    win.fsel = None;
    win.rename = None;
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    win.thumbs.clear();
    for e in entries {
        let kind = FKind::of(&e.name, e.is_dir);
        // Rasm bölsa — kiçik eskiz (bir marta dekodlanadi, keşlanadi)
        let thumb = if kind == FKind::Image {
            let fp = path_join(path, &e.name);
            let size = fs::read_all(&fp).map(|d| d.len()).unwrap_or(0);
            // Keşdan (yöl + ölçam mos kelsa) — qayta dekodlamaymiz
            let cached = THUMB_CACHE.lock().iter().find(|(p, s, _)| *p == fp && *s == size).map(|(_, _, i)| i.clone());
            cached.or_else(|| {
                let t = fs::read_all(&fp)
                    .ok()
                    .filter(|d| d.len() < 4 * 1024 * 1024)
                    .and_then(|d| crate::image::decode_file(&e.name, &d))
                    .map(|img| make_thumb(&img, 128));
                if let Some(t) = &t {
                    let mut c = THUMB_CACHE.lock();
                    if c.len() >= 48 {
                        c.remove(0);
                    }
                    c.push((fp.clone(), size, t.clone()));
                }
                t
            })
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
            win.lines = alloc::vec![alloc::format!("{} formati hali qöllanmaydi", ext), String::from("Qöllanadi: WAV, AIFF, FLAC, MP3, OGG")];
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
            win.video = true;
            win.playing = true;
        } else if let Some(vi) = crate::video::info(&data) {
            // taniladi, ammo dekod qilinmaydi — metama'lumotni körsatamiz
            win.lines = alloc::vec![
                alloc::format!("{} video — {}", vi.container, vi.codec),
                alloc::format!("ölçam: {}x{}", vi.w, vi.h),
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
        let mut win = base(x.min(sw - w), y, w, h, Kind::Image, String::new(), Vec::new());
        image_load(&mut win, path, &data);
        windows.push(win);
        return;
    }
    // HTML — brauzerda
    let low = name.to_ascii_lowercase();
    if low.ends_with(".html") || low.ends_with(".htm") {
        let (w, h) = (u * 40, u * 30);
        let mut win = base(x.min(sw - w), y, w, h, Kind::Browser, String::from("Zamin Brauzer"), Vec::new());
        web_go(&mut win, path, false);
        windows.push(win);
        return;
    }
    // ELF dastur?
    if data.starts_with(b"\x7fELF") {
        let (w, h) = (u * 24, u * 10);
        let mut win = base(x.min(sw - w), y, w, h, Kind::Info, alloc::format!("Dastur — {}", name), Vec::new());
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
    lines.push(input);
    let mut win = base(x.min(sw - w), y, w, h, Kind::Editor, alloc::format!("Matn — {}", name), lines);
    win.path = String::from(path);
    editor_fix(&mut win);
    windows.push(win);
}

/// Rasm oynasiga faylni yuklaydi (oldingi/keyingi rasmga ötişda ham).
fn image_load(win: &mut Window, path: &str, data: &[u8]) {
    let name = path.rsplit('/').next().unwrap_or(path);
    win.path = String::from(path);
    win.title = alloc::format!("Rasm — {}", name);
    win.img = None;
    win.frames.clear();
    win.frame = 0;
    win.lines.clear();
    // Harakatli rasm (GIF/APNG)?
    if let Some(frames) = crate::image::decode_frames(name, data).filter(|f| f.len() > 1) {
        win.title = alloc::format!("Rasm — {} ({} kadr)", name, frames.len());
        win.frames = frames;
        win.frame_ms = crate::timer::uptime_ms();
    } else {
        win.img = crate::image::decode_file(name, data);
    }
    if win.img.is_none() && win.frames.is_empty() {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
        win.lines = alloc::vec![
            alloc::format!("{} formati hali qöllanmaydi", ext),
            String::new(),
            String::from("Qöllanadi: PNG, APNG, JPEG, GIF, BMP, TGA,"),
            String::from("PCX, ICO, QOI, PNM, SVG, PDF, EPS, DXF, WMF"),
        ];
    }
}

/// "Rasmlar" ilovasi: ~/Rasmlar dagi birinçi rasm (yöq bölsa — namuna manzara).
fn open_pictures(windows: &mut Vec<Window>, n: usize) {
    let dir = String::from("~/Rasmlar");
    let first = image_siblings(&path_join(&dir, "_")).into_iter().next();
    match first {
        Some(p) => open_file(windows, &p, n),
        None => windows.push(new_window(Kind::Image, n)),
    }
}

/// Şu papkadagi rasmlar (video emas), alifbo tartibida.
fn image_siblings(path: &str) -> Vec<String> {
    let dir = match path.rsplit_once('/') {
        Some((d, _)) if !d.is_empty() => d,
        _ => "/",
    };
    let mut v: Vec<String> = fs::list(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.is_dir && !e.name.starts_with('.') && FKind::of(&e.name, false) == FKind::Image && !crate::video::is_video_ext(&e.name))
        .map(|e| path_join(dir, &e.name))
        .collect();
    v.sort();
    v
}

/// Oldingi (-1) yoki keyingi (+1) rasmga ötadi.
fn image_step(win: &mut Window, d: isize) {
    let sib = image_siblings(&win.path);
    if sib.len() < 2 {
        return;
    }
    let cur = sib.iter().position(|p| *p == win.path).unwrap_or(0) as isize;
    let n = sib.len() as isize;
    let next = &sib[((cur + d) % n + n) as usize % n as usize];
    let data = fs::read_all(next).unwrap_or_default();
    let next = next.clone();
    image_load(win, &next, &data);
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
                // \x1b[K (qator oxirigaça öçiriş): \r allaqaçon qatorni böşatadi, kursor doim
                // oxirida — qöşimça iş kerak emas.
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
    // yangi çiqiş — pastga qaytamiz; tarix çegarasi (eskilar bitta drain bilan kesiladi)
    if !bytes.is_empty() {
        win.term_scroll = 0;
    }
    crate::termbuf::trim(&mut win.lines);
}

fn push_line(win: &mut Window) {
    let line = String::from_utf8_lossy(&core::mem::take(&mut win.cur)).into_owned();
    win.lines.push(line);
}

/// Terminal joylaşuvi: (ustunlar soni, körinadigan vizual qatorlar soni).
fn term_geom(win: &Window) -> (usize, usize) {
    let px = small();
    let lh = gfx::line_height(px, Font::Mono).max(px + 3);
    let pad = body() * 3 / 4;
    let avail = win.h.saturating_sub(title_h() + pad * 2);
    let adv = gfx::mono_advance(px).max(1);
    // öng çetda aylantiriş çiziği va kursor uçun joy
    let cols = win.w.saturating_sub(pad * 2 + 6 + adv) / adv;
    (cols.max(1), (avail / lh).max(1))
}

/// Terminalning barça vizual qatorlari: tarix + joriy (tugamagan çiqiş + kiriş) qator.
fn term_text(win: &Window) -> (String, Vec<(usize, usize, usize)>) {
    let (cols, _) = term_geom(win);
    let full = alloc::format!("{}{}", String::from_utf8_lossy(&win.cur), win.input);
    let mut rows = crate::termbuf::wrap_lines(&win.lines, cols);
    let mut tmp = Vec::new();
    crate::termbuf::wrap_line(&full, cols, &mut tmp);
    let li = win.lines.len();
    rows.extend(tmp.into_iter().map(|(a, b)| (li, a, b)));
    (full, rows)
}

/// Terminalni `d` vizual qatorga aylantiradi (musbat — yuqoriga, tarixga).
fn term_scroll_by(win: &mut Window, d: isize) {
    let (_, vis) = term_geom(win);
    let total = term_text(win).1.len();
    let max = total.saturating_sub(vis) as isize;
    win.term_scroll = (win.term_scroll.min(max as usize) as isize + d).clamp(0, max) as usize;
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
                    crate::termbuf::trim(&mut win.lines);
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
    let wsel = sel_wall();
    let photo = if wsel == 0 || wsel == WALL_CUSTOM { crate::image::decode(crate::wallpaper::JPG) } else { None };
    let base_bg = build_wallpaper(wsel, sw, sh, &photo);
    let bg = base_bg.map(|mut im| {
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
    let photo = crate::image::decode(crate::wallpaper::JPG);
    *PHOTO_THUMB.lock() = photo.as_ref().map(|p| make_thumb(p, 192));
    let mut cur_wall = sel_wall();
    let mut cur_path = crate::config::UI_WALL_PATH.lock().clone();
    let mut wp = build_wallpaper(cur_wall, sw, sh, &photo);
    let mut windows: Vec<Window> = Vec::new();
    let mut cx = sw / 2;
    let mut cy = sh / 2;
    let mut hover_at = (usize::MAX, usize::MAX); // :hover oxirgi tekşirilgan kursor joyi
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
        search: None,
        ctx: None,
        fclip: None,
        snap: None,
        fdrag: None,
        flash: 0,
        drag_start: (0, 0),
    };
    TRASH_N.store(usize::MAX, Ordering::Relaxed); // yangi foydalanuvçi — qayta hisoblanadi
    CPU_HIST.lock().clear();
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
            {
                let mut h = CPU_HIST.lock();
                h.push(desk.cpu);
                if h.len() > 60 {
                    h.remove(0);
                }
            }
            if sec % 5 == 0 {
                desk.icons = home_icons();
            }
            changed = true;
        }
        // Fon rasmi özgargan bölsa — yangisini yasaymiz
        let want = sel_wall();
        let want_path = crate::config::UI_WALL_PATH.lock().clone();
        if want != cur_wall || (want == WALL_CUSTOM && want_path != cur_path) {
            cur_wall = want;
            cur_path = want_path;
            wp = build_wallpaper(want, sw, sh, &photo);
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
            if win.frames.len() > 1 && (!win.video || win.playing) {
                let delay = win.frames[win.frame.min(win.frames.len() - 1)].1.max(20) as u64;
                if now_ms.wrapping_sub(win.frame_ms) >= delay {
                    win.frame = (win.frame + 1) % win.frames.len(); // video ham takrorlanadi
                    win.frame_ms = now_ms;
                    changed = true;
                }
            }
            // Övoz ijrosi bosh (playhead) vaqtga qarab oldinga suriladi
            if win.playing && !win.video {
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
            // Qidiruv oçiq: klaviatura unga tegişli
            if e.typ == input::EV_KEY && e.code != input::BTN_LEFT && desk.search.is_some() {
                match e.code {
                    42 | 54 => shift = e.value != 0,
                    56 => alt = e.value != 0,
                    100 => ALTGR.store(e.value != 0, Ordering::Relaxed),
                    _ if e.value == 0 => {}
                    1 => desk.search = None,
                    57 if alt => desk.search = None,
                    103 => {
                        let s = desk.search.as_mut().unwrap();
                        s.sel = s.sel.saturating_sub(1);
                    }
                    108 => {
                        let s = desk.search.as_mut().unwrap();
                        if s.sel + 1 < s.res.len() {
                            s.sel += 1;
                        }
                    }
                    28 => {
                        let s = desk.search.take().unwrap();
                        if let Some((_, _, a)) = s.res.get(s.sel).cloned() {
                            search_exec(a, &mut windows, &mut opened);
                        }
                    }
                    14 => {
                        let s = desk.search.as_mut().unwrap();
                        s.q.pop();
                        s.res = search_results(&s.q);
                        s.sel = 0;
                    }
                    code => {
                        if let Some(c) = keycode_char(code, shift) {
                            let s = desk.search.as_mut().unwrap();
                            if s.q.len() < 40 {
                                s.q.push(c);
                                s.res = search_results(&s.q);
                                s.sel = 0;
                            }
                        }
                    }
                }
                changed = true;
                continue;
            }
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
                        if !ctx_click(&mut desk, &mut windows, cx, cy, &mut opened)
                            && !search_click(&mut desk, &mut windows, cx, cy, &mut opened)
                            && !notif_click(&mut desk, cx, cy)
                            && !menu_click(&mut desk, &mut windows, cx, cy, &mut opened)
                            && !handle_click(&mut windows, cx, cy, sh, &mut drag, &mut opened, &mut desk.anim)
                            && !widget_click(&mut desk, sw, cx, cy)
                        {
                            desk_icon_click(&mut desk, &mut windows, cx, cy, &mut opened);
                        }
                        desk.drag_start = (cx, cy);
                    } else {
                        pressed = false;
                        if let Some(d) = desk.fdrag.take() {
                            drop_file(&mut windows, &mut desk, d, cx, cy, shift);
                        }
                        *FDRAG_PENDING.lock() = None;
                        for w in windows.iter_mut() {
                            w.pen = None;
                        }
                        if let (Some((idx, off)), Some(r)) = (drag, desk.snap.take()) {
                            if off != RESIZE && idx < windows.len() {
                                let w = &mut windows[idx];
                                let from = (w.x, w.y, w.w, w.h);
                                if w.saved.is_none() {
                                    w.saved = Some(from);
                                }
                                (w.x, w.y, w.w, w.h) = r;
                                w.hidden = true;
                                desk.anim = Some(Anim { from, to: r, t0: crate::timer::uptime_ms(), dur: ANIM_MS, reveal: w.id });
                            }
                        }
                        desk.snap = None;
                        drag = None;
                    }
                    changed = true;
                }
                (input::EV_KEY, BTN_RIGHT) => {
                    if e.value == 1 {
                        desk.menu = None;
                        desk.notif_open = false;
                        desk.ctx = build_ctx(&windows, &desk, cx, cy);
                        changed = true;
                    }
                }
                (input::EV_KEY, 42) | (input::EV_KEY, 54) => shift = e.value != 0,
                (input::EV_KEY, 100) => ALTGR.store(e.value != 0, Ordering::Relaxed),
                (input::EV_KEY, 56) => {
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
                (input::EV_KEY, 57) if e.value == 1 && alt => {
                    open_search(&mut desk); // Alt+Space — tezkor qidiruv
                    changed = true;
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
                            17 if windows[i].kind == Kind::Browser && !windows[i].web_tabs.is_empty() => {
                                tab_close(&mut windows[i]); // Alt+W — faol tabni yopiş
                            }
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
                (input::EV_KEY, 1) if e.value == 1 && windows.last().is_some_and(|w| !w.hidden && w.kind == Kind::Editor && w.find.is_some()) => {
                    // Esc — muharrir qidiruvini yopiş
                    editor_find_toggle(windows.last_mut().unwrap());
                    changed = true;
                }
                (input::EV_KEY, 1) if e.value == 1 && windows.last().is_some_and(|w| w.rename.is_some()) => {
                    windows.last_mut().unwrap().rename = None; // nomini özgartirişni bekor qiliş
                    changed = true;
                }
                (input::EV_KEY, 1) if e.value == 1 && windows.last().is_some_and(|w| w.web.as_ref().is_some_and(|b| b.editing || b.focus.is_some())) => {
                    let b = windows.last_mut().unwrap().web.as_mut().unwrap();
                    b.editing = false;
                    b.focus = None;
                    changed = true;
                }
                (2, 8) => {
                    // Siçqon ğildiragi: ostidagi brauzer sahifasini aylantiradi
                    let v = e.value as i32;
                    if let Some(w) = windows.iter_mut().rev().find(|w| !w.hidden && w.contains(cx, cy)) {
                        if w.kind == Kind::Browser {
                            web_scroll(w, -(v as isize) * body() as isize * 3);
                            changed = true;
                        } else if w.kind == Kind::Terminal {
                            term_scroll_by(w, v as isize * 3);
                            changed = true;
                        }
                    }
                }
                (input::EV_KEY, 1) if e.value == 1 => {
                    // Esc — menyu, panel va almaştirgiçni yopadi
                    desk.menu = None;
                    desk.notif_open = false;
                    desk.switcher = None;
                    desk.ctx = None;
                    changed = true;
                }
                (input::EV_KEY, 46) | (input::EV_KEY, 45) | (input::EV_KEY, 47) if e.value == 1 && alt => {
                    // Alt+C — nusxa, Alt+X — kesiş, Alt+V — joylaş (Fayllar)
                    if files_clip_key(&mut windows, &mut desk, e.code) {
                        changed = true;
                    }
                }
                (input::EV_KEY, 31) if e.value == 1 && alt => {
                    // Alt+S — muharrirda saqlaş
                    if let Some(w) = windows.last_mut().filter(|w| !w.hidden && w.kind == Kind::Editor) {
                        let msg = save_notes(w);
                        notify("Matn muharriri", &msg, 0x4aa3df);
                        w.cur = msg.into_bytes();
                        changed = true;
                    }
                }
                (input::EV_KEY, 99) if e.value == 1 => SHOT_REQ.store(true, Ordering::Relaxed),
                (input::EV_KEY, 20) if e.value == 1 && alt => {
                    // Alt+T — brauzerda yangi tab
                    if let Some(w) = windows.last_mut().filter(|w| !w.hidden && w.kind == Kind::Browser) {
                        tab_new(w, "zamin://bosh");
                        changed = true;
                    }
                }
                (input::EV_KEY, 44) | (input::EV_KEY, 37) if e.value == 1 && alt => {
                    // Alt+Z — bekor qiliş, Alt+Shift+Z — qaytariş, Alt+K — qidiruv (muharrir)
                    if let Some(w) = windows.last_mut().filter(|w| !w.hidden && w.kind == Kind::Editor) {
                        if e.code == 37 {
                            editor_find_toggle(w);
                        } else {
                            editor_undo(w, shift);
                        }
                        changed = true;
                    }
                }
                (input::EV_KEY, _) if alt => {}
                (input::EV_KEY, code) if e.value == 1 => {
                    let top = windows.last().filter(|w| !w.hidden).map(|w| w.kind);
                    match top {
                        Some(Kind::Files) => files_key(&mut windows, &mut desk, code, shift, &mut opened),
                        Some(Kind::Calc) => calc_key(windows.last_mut().unwrap(), code, shift),
                        Some(Kind::Image) => image_key(windows.last_mut().unwrap(), code),
                        Some(Kind::Monitor) => monitor_key(windows.last_mut().unwrap(), code),
                        Some(Kind::Puzzle) => puzzle_key(windows.last_mut().unwrap(), code),
                        Some(Kind::Browser) => web_key(windows.last_mut().unwrap(), code, shift),
                        Some(Kind::Trash) if code == 111 => {
                            let w = windows.last_mut().unwrap();
                            if let Some(name) = w.fsel.and_then(|s| w.rows.get(s)).map(|r| r.0.clone()) {
                                match trash_purge(&name) {
                                    Ok(()) => notify("Butunlay öçirildi", &name, 0xff453a),
                                    Err(e) => notify("Öçirib bölmadi", &e, 0xff453a),
                                }
                                w.fsel = None;
                                reload_all_files(&mut windows);
                            }
                        }
                        Some(k) if k.takes_keys() => type_key(windows.last_mut().unwrap(), code, shift),
                        _ => {}
                    }
                    changed = true;
                }
                _ => {}
            }
        }
        // Siljitiş
        if pressed {
            // Fayl sudraş: bosilgan element 8 pikseldan köp siljisa boşlanadi
            if desk.fdrag.is_none() {
                let pend = FDRAG_PENDING.lock().clone();
                if let Some((p, n, k, x0, y0)) = pend {
                    if cx.abs_diff(x0) + cy.abs_diff(y0) > 8 {
                        desk.fdrag = Some(FDrag { path: p, name: n, kind: k });
                        *FDRAG_PENDING.lock() = None;
                    }
                }
            }
            if desk.fdrag.is_some() {
                changed = true;
            }
            if let Some(w) = windows.last_mut().filter(|w| w.kind == Kind::Paint && w.pen.is_some()) {
                paint_drag(w, cx, cy);
                changed = true;
            }
            // Yopiştirilgan / kattalaştirilgan oyna sudralsa — oldingi ölçamiga qaytadi
            if let Some((idx, off)) = drag {
                if off != RESIZE && idx < windows.len() && windows[idx].saved.is_some()
                    && cx.abs_diff(desk.drag_start.0) + cy.abs_diff(desk.drag_start.1) > 8
                {
                    let w = &mut windows[idx];
                    let (_, _, ow, oh) = w.saved.take().unwrap();
                    let ox = ((off & 0xffff) * ow / w.w.max(1)).min(ow.saturating_sub(1));
                    w.w = ow;
                    w.h = oh;
                    drag = Some((idx, ox | ((off >> 16) << 16)));
                }
                if off != RESIZE {
                    let z = snap_zone(cx, cy);
                    if z != desk.snap {
                        desk.snap = z;
                        changed = true;
                    }
                }
            }
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
        // Brauzer: fonda yuklangan sahifalar va JS taymerlari
        if web_poll(&mut windows) {
            changed = true;
        }
        // Brauzer: @media (oyna eni) va :hover (siçqonça ostidagi element)
        for w in windows.iter_mut().filter(|w| w.kind == Kind::Browser && !w.hidden) {
            if web_media(w) {
                changed = true;
            }
        }
        if (cx, cy) != hover_at {
            hover_at = (cx, cy);
            if let Some(t) = windows.iter().rposition(|w| !w.hidden).filter(|&t| windows[t].kind == Kind::Browser) {
                if web_hover(&mut windows[t], cx, cy) {
                    changed = true;
                }
            }
        }
        // Soat: sekundomer / taymer
        {
            let now = crate::timer::uptime_ms();
            let mut tick = false;
            for w in windows.iter_mut().filter(|w| w.kind == Kind::Clock) {
                if w.frame_ms != 0 && now >= w.frame_ms {
                    w.frame_ms = 0;
                    notify("Taymer tugadi", &alloc::format!("{} vaqt ötdi", fmt_ms(w.frame as u64 * 1000, false)), 0xff9f0a);
                    changed = true;
                }
                if (w.playing && w.tab == 1) || (w.frame_ms != 0 && w.tab == 2) {
                    tick = true;
                }
            }
            if tick && now.wrapping_sub(last_play_draw) >= 50 {
                last_play_draw = now;
                changed = true;
            }
        }
        if SHOT_REQ.swap(false, Ordering::Relaxed) {
            desk.menu = None;
            take_screenshot(&windows, &wp, &desk);
            desk.flash = crate::timer::uptime_ms();
            changed = true;
        }
        if crate::timer::uptime_ms().saturating_sub(desk.flash) < 400 {
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
        editor_key(w, code, shift);
        return;
    }
    // Terminal: Shift+PageUp/PageDown — tarixni aylantiriş; boşqa tugma — pastga qaytiş
    if shift && (code == 104 || code == 109) {
        let step = (term_geom(w).1.saturating_sub(1)).max(1) as isize;
        term_scroll_by(w, if code == 104 { step } else { -step });
        return;
    }
    w.term_scroll = 0;
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
            crate::termbuf::trim(&mut w.lines);
        }
        // ↑ / ↓ / Tab (va böş qatorda Backspace): terminalning özi qatorni tahrirlaydi —
        // terilgan matn + boşqaruv baytlarini quvurga yuboramiz, u \r + qator + ESC[K bilan qayta çizadi.
        // Faqat terminalning özi kiriş kutayotganda (oldingi planda dastur yöq).
        103 | 108 | 15 | 14 if w.pin != NONE_PIPE && task::foreground() == 0 && (code != 14 || w.input.is_empty()) => {
            let mut data = core::mem::take(&mut w.input).into_bytes();
            data.extend_from_slice(match code {
                103 => b"\x1b[A",
                108 => b"\x1b[B",
                15 => b"\t",
                _ => b"\x7f",
            });
            let _ = pipe::try_write(w.pin, &data);
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
fn save_notes(w: &mut Window) -> String {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    let mut text = w.lines.join("\n");
    text.push('\n');
    let path = if w.path.is_empty() { String::from(NOTES_PATH) } else { w.path.clone() };
    match fs::open(&path, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE) {
        Ok(id) => match fs::write(id, Some(0), text.as_bytes()) {
            Ok(n) => {
                w.dirty = false;
                let _ = fs::commit();
                alloc::format!("saqlandi: {} bayt", n)
            }
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
                    Kind::Image => open_pictures(windows, *opened),
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
        if windows[i].kind == Kind::Settings && settings_click(&mut windows[i], cx, cy) {
            let w = windows.remove(i);
            windows.push(w);
            return true;
        }
        let hit = match windows[i].kind {
            Kind::Calc => calc_click(&mut windows[i], cx, cy),
            Kind::Monitor => monitor_click(&mut windows[i], cx, cy),
            Kind::Image => viewer_click(&mut windows[i], cx, cy),
            Kind::Puzzle => puzzle_click(&mut windows[i], cx, cy),
            Kind::Browser => web_click(&mut windows[i], cx, cy),
            Kind::Paint => paint_click(&mut windows[i], cx, cy),
            Kind::Clock => clock_click(&mut windows[i], cx, cy),
            Kind::Editor => editor_click(&mut windows[i], cx, cy),
            Kind::Trash => match trash_click(&mut windows[i], cx, cy) {
                2 => {
                    reload_all_files(windows);
                    true
                }
                1 => true,
                _ => false,
            },
            _ => false,
        };
        if hit {
            let w = windows.remove(i);
            if !(w.kind == Kind::Browser && w.web.is_none()) {
                windows.push(w); // (brauzerda oxirgi tab yopilgan bölsa — oyna yopiladi)
            }
            return true;
        }
        if windows[i].on_grip(cx, cy) {
            let w = windows.remove(i);
            windows.push(w);
            *drag = Some((windows.len() - 1, RESIZE));
            return true;
        }
        if windows[i].kind == Kind::Editor && windows[i].in_save_btn(cx, cy) {
            let msg = save_notes(&mut windows[i]);
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
            // Sarlavhani ikki marta bosiş — kattalaştiriş / qaytariş
            let id = windows[i].id;
            let dbl = {
                let mut t = TITLE_CLICK.lock();
                let d = t.1 == id && now.saturating_sub(t.0) < 450;
                *t = if d { (0, 0) } else { (now, id) };
                d
            };
            if dbl {
                zoom_window(windows, i, anim);
                return true;
            }
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
                // Bir marta — tanlaş, ikki marta — oçiş
                let w = windows.remove(i);
                windows.push(w);
                let wi = windows.len() - 1;
                let w = &mut windows[wi];
                if w.rename.is_some() {
                    if w.fsel != Some(row) {
                        files_rename_commit(w);
                    }
                    return true;
                }
                let dbl = w.lclick.1 == row && now.saturating_sub(w.lclick.0) < 500;
                w.fsel = Some(row);
                w.lclick = (now, row);
                if !dbl {
                    let (name, kind) = w.rows[row].clone();
                    *FDRAG_PENDING.lock() = Some((path_join(&w.path, &name), name, kind, cx, cy));
                }
                if dbl {
                    w.lclick = (0, usize::MAX);
                    files_activate(windows, wi, row, opened);
                }
                return true;
            }
            let g = files_geo(&windows[i]);
            if cx >= g.gx && cy >= g.gy && windows[i].contains(cx, cy) {
                // böş joy — tanlov bekor, nom saqlanadi
                files_rename_commit(&mut windows[i]);
                windows[i].fsel = None;
                let w = windows.remove(i);
                windows.push(w);
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
    if let Some((x, y, w2, h2)) = desk.snap {
        fill_round_rect_a(x, y, w2, h2, radius(), accent(), 45);
        gfx::round_outline(x, y, w2, h2, radius(), 2, accent());
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
    if let Some(sr) = &desk.search {
        draw_search(sr);
    }
    if let Some(c) = &desk.ctx {
        draw_ctx(c, cx, cy);
    }
    if let Some(d) = &desk.fdrag {
        draw_fdrag(windows, desk, d, cx, cy);
    }
    let fa = crate::timer::uptime_ms().saturating_sub(desk.flash);
    if desk.flash != 0 && fa < 400 {
        gfx::blend_rect(0, 0, w, h, 0xffffff, ((400 - fa) * 150 / 400) as u32);
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
    sx = sx.saturating_sub(small() * 3 / 2);
    status_search(sx, cy2, small());
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
fn status_search(cx: usize, cy: usize, s: usize) {
    let r = s * 3 / 10;
    let (fx, fy) = ((cx - s / 10) as f32, (cy - s / 10) as f32);
    gfx::fill_circle(fx as usize, fy as usize, r, 0xdbe4f0);
    gfx::fill_circle(fx as usize, fy as usize, r * 3 / 5, 0x0f1726);
    let q = r as f32;
    gfx::fill_poly(&[(fx + q * 0.55, fy + q * 0.8), (fx + q * 0.8, fy + q * 0.55), (fx + q * 1.7, fy + q * 1.45), (fx + q * 1.45, fy + q * 1.7)], 0xdbe4f0);
}

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
    search: Option<Search>,     // tezkor qidiruv (Alt+Space)
    ctx: Option<Ctx>,           // öng tugma menyusi
    fclip: Option<(String, bool)>, // fayl buferi: (yöl, kesilganmi)
    snap: Option<(usize, usize, usize, usize)>, // yopiştiriş körinişi (sudralayotganda)
    fdrag: Option<FDrag>,       // sudralayotgan fayl
    flash: u64,                 // ekran rasmi çaqnaşi vaqti
    drag_start: (usize, usize), // oyna sudraş boşlangan nuqta
}

// ---------------- Kontekst menyusi (öng tugma) ----------------

const BTN_RIGHT: u16 = 0x111;

#[derive(Clone)]
enum CAct {
    Open(String),            // fayl — mos ilovada, papka — Fayllarda
    Nav(String, usize),      // papkaga şu Fayllar oynasida kiriş
    SetWall(String),         // rasmni fon qilib qöyiş
    Delete(String),          // savatga tashlaş
    Rename(usize, usize),    // (oyna, element) — nomini özgartiriş
    Copy(String, bool),      // buferga: (yöl, kesiş)
    Paste(String),           // buferdagini papkaga joylaş
    Restore(String),         // savatdan tiklaş
    Purge(String),           // savatdan butunlay öçiriş
    EmptyTrash,
    Focus(Kind),             // dok: ilova oynasini old planga
    CloseApp(Kind),          // dok: ilovaning barça oynalarini yopiş
    NewFolder(String, usize),
    Refresh(usize),
    App(Kind),
    WallSettings,
    Sep,
}

struct Ctx {
    x: usize,
    y: usize,
    items: Vec<(String, CAct)>,
}

/// Öng tugma bosilgan joyga qarab kontekst menyusini tuzadi.
fn build_ctx(windows: &[Window], desk: &Desk, cx: usize, cy: usize) -> Option<Ctx> {
    let (sw, sh) = gfx::size();
    let (_, dy, _, _, _, _) = dock_geom(sw, sh);
    if cy >= dy {
        return dock_ctx(windows, cx, cy);
    }
    if cy < menubar_h() {
        return None;
    }
    let s = |t: &str| String::from(t);
    let mut items: Vec<(String, CAct)> = Vec::new();
    if let Some(i) = windows.iter().rposition(|w| !w.hidden && w.contains(cx, cy)) {
        let w = &windows[i];
        if w.kind == Kind::Trash {
            if let Some(r) = trash_row_at(w, cx, cy) {
                let n = w.rows[r].0.clone();
                items.push((s("Tiklaş"), CAct::Restore(n.clone())));
                items.push((s("Sep"), CAct::Sep));
                items.push((s("Butunlay öçiriş"), CAct::Purge(n)));
            } else if !w.rows.is_empty() {
                items.push((s("Savatni böşatiş"), CAct::EmptyTrash));
            } else {
                return None;
            }
            return Some(Ctx { x: cx, y: cy, items });
        }
        if w.kind != Kind::Files {
            return None;
        }
        let g = files_geo(w);
        if cx < g.gx || cy < g.gy {
            return None;
        }
        if let Some(r) = file_row_at(w, cx, cy) {
            let (name, kind) = &w.rows[r];
            let p = path_join(&w.path, name);
            if *kind == FKind::Dir {
                items.push((s("Oçiş"), CAct::Nav(p.clone(), w.id)));
            } else {
                items.push((s("Oçiş"), CAct::Open(p.clone())));
                if *kind == FKind::Image && !crate::video::is_video_ext(name) {
                    items.push((s("Fon qilib qöyiş"), CAct::SetWall(p.clone())));
                }
            }
            items.push((s("Sep"), CAct::Sep));
            items.push((s("Nomini özgartiriş"), CAct::Rename(w.id, r)));
            items.push((s("Nusxa olish"), CAct::Copy(p.clone(), false)));
            items.push((s("Kesiş"), CAct::Copy(p.clone(), true)));
            items.push((s("Sep"), CAct::Sep));
            items.push((s("Savatga tashlaş"), CAct::Delete(p)));
        } else {
            items.push((s("Yangi papka"), CAct::NewFolder(w.path.clone(), w.id)));
            if desk.fclip.is_some() {
                items.push((s("Joylaş"), CAct::Paste(w.path.clone())));
            }
            items.push((s("Yangilaş"), CAct::Refresh(w.id)));
        }
    } else if let Some(k) = (0..desk.icons.len()).find(|&k| {
        let (x, y, cw, ch) = desk_icon_rect(k);
        cx >= x && cx < x + cw && cy >= y && cy < y + ch
    }) {
        let p = alloc::format!("~/{}", desk.icons[k]);
        items.push((s("Oçiş"), CAct::Open(p.clone())));
        items.push((s("Nusxa olish"), CAct::Copy(p.clone(), false)));
        items.push((s("Sep"), CAct::Sep));
        items.push((s("Savatga tashlaş"), CAct::Delete(p)));
    } else {
        items.push((s("Fon rasmini almaştiriş…"), CAct::WallSettings));
        items.push((s("Yangi papka"), CAct::NewFolder(s("~"), 0)));
        if desk.fclip.is_some() {
            items.push((s("Joylaş"), CAct::Paste(s("~"))));
        }
        items.push((s("Sep"), CAct::Sep));
        items.push((s("Terminal"), CAct::App(Kind::Terminal)));
        items.push((s("Sozlamalar"), CAct::App(Kind::Settings)));
    }
    Some(Ctx { x: cx, y: cy, items })
}

/// Kontekst menyusi joylaşuvi: (x, y, eni, [(y0, y1)]).
fn ctx_layout(c: &Ctx) -> (usize, usize, usize, Vec<(usize, usize)>) {
    let (sw, sh) = gfx::size();
    let u = body();
    let w = c.items.iter().map(|(n, _)| gfx::text_width(n, small())).max().unwrap_or(0).max(u * 6) + u * 2;
    let h: usize = c.items.iter().map(|(_, a)| if matches!(a, CAct::Sep) { u / 2 } else { u * 3 / 2 }).sum::<usize>() + u / 2;
    let x = if c.y == usize::MAX { c.x.saturating_sub(w / 2) } else { c.x }.min(sw.saturating_sub(w + 2));
    let y = if c.y == usize::MAX {
        // dok ustida
        let (_, dy, _, _, _, _) = dock_geom(sw, sh);
        dy.saturating_sub(h + u / 2)
    } else {
        c.y.min(sh.saturating_sub(h + 2))
    };
    let mut yy = y + u / 4;
    let mut rows = Vec::new();
    for (_, a) in &c.items {
        let hh = if matches!(a, CAct::Sep) { u / 2 } else { u * 3 / 2 };
        rows.push((yy, yy + hh));
        yy += hh;
    }
    (x, y, w, rows)
}

fn draw_ctx(c: &Ctx, mx: usize, my: usize) {
    let u = body();
    let (x, y, w, rows) = ctx_layout(c);
    let h = rows.last().map(|r| r.1).unwrap_or(y) + u / 4 - y;
    gfx::shadow(x, y, w, h, u / 2, 10);
    fill_round_rect_a(x, y, w, h, u / 2, 0x1a2131, 248);
    gfx::round_outline(x, y, w, h, u / 2, 1, 0x34405a);
    for ((name, act), (y0, y1)) in c.items.iter().zip(rows.iter()) {
        if matches!(act, CAct::Sep) {
            fill_rect(x + u / 2, (y0 + y1) / 2, w - u, 1, 0x34405a);
            continue;
        }
        let hover = mx >= x && mx < x + w && my >= *y0 && my < *y1;
        if hover {
            fill_round_rect_a(x + u / 4, *y0, w - u / 2, y1 - y0, u / 3, accent(), 230);
        }
        let col = if matches!(act, CAct::Delete(_) | CAct::Purge(_) | CAct::EmptyTrash) && !hover { 0xff6b61 } else if hover { OQ } else { 0xe1e8f2 };
        gfx::draw_text(x + u, y0 + (y1 - y0 - small()) / 2, name, col, small());
    }
}

/// Kontekst menyusiga bosiş: element — bajaradi; taşqarisi — yopadi.
fn ctx_click(desk: &mut Desk, windows: &mut Vec<Window>, cx: usize, cy: usize, opened: &mut usize) -> bool {
    let Some(c) = desk.ctx.take() else { return false };
    let (x, _, w, rows) = ctx_layout(&c);
    if cx >= x && cx < x + w {
        if let Some(k) = rows.iter().position(|r| cy >= r.0 && cy < r.1) {
            let act = c.items[k].1.clone();
            if matches!(act, CAct::Sep) {
                desk.ctx = Some(c);
                return true;
            }
            ctx_exec(act, desk, windows, opened);
        }
    }
    true
}

fn reload_files(windows: &mut [Window], id: usize) {
    if let Some(w) = windows.iter_mut().find(|w| w.id == id) {
        let p = w.path.clone();
        load_dir(w, &p);
    }
}

fn ctx_exec(act: CAct, desk: &mut Desk, windows: &mut Vec<Window>, opened: &mut usize) {
    match act {
        CAct::Open(p) => {
            let is_dir = fs::list(&p).is_ok();
            if is_dir {
                menu_act(MAct::Path(p), windows, opened);
            } else {
                open_file(windows, &p, *opened);
                *opened += 1;
            }
        }
        CAct::Nav(p, id) => {
            if let Some(w) = windows.iter_mut().find(|w| w.id == id) {
                load_dir(w, &p);
            }
        }
        CAct::SetWall(p) => {
            let name = String::from(p.rsplit('/').next().unwrap_or(""));
            *crate::config::UI_WALL_PATH.lock() = p;
            crate::config::UI_WALL.store(WALL_CUSTOM as u8, Ordering::Relaxed);
            let _ = crate::config::save_current();
            notify("Fon özgartirildi", &name, 0x2fbf5f);
        }
        CAct::Delete(p) => {
            match to_trash(&p) {
                Ok(n) => notify("Savatga tashlandi", &n, 0xff9f0a),
                Err(e) => notify("Savatga tashlab bölmadi", &e, 0xff453a),
            }
            reload_all_files(windows);
            desk.icons = home_icons();
        }
        CAct::Rename(id, row) => {
            if let Some(w) = windows.iter_mut().find(|w| w.id == id) {
                if let Some(r) = w.rows.get(row) {
                    w.fsel = Some(row);
                    w.rename = Some(r.0.clone());
                }
            }
        }
        CAct::Copy(p, cut) => clip_set(desk, p, cut),
        CAct::Paste(dir) => paste_into(windows, desk, &dir),
        CAct::Restore(n) => {
            match trash_restore(&n) {
                Ok(dest) => notify("Tiklandi", &dest, 0x2fbf5f),
                Err(e) => notify("Tiklab bölmadi", &e, 0xff453a),
            }
            reload_all_files(windows);
            desk.icons = home_icons();
        }
        CAct::Purge(n) => {
            match trash_purge(&n) {
                Ok(()) => notify("Butunlay öçirildi", &n, 0xff453a),
                Err(e) => notify("Öçirib bölmadi", &e, 0xff453a),
            }
            reload_all_files(windows);
        }
        CAct::EmptyTrash => {
            let n = empty_trash();
            notify("Savat böşatildi", &alloc::format!("{} ta element butunlay öçirildi", n), 0xff453a);
            reload_all_files(windows);
        }
        CAct::NewFolder(dir, id) => {
            let mut n = 1;
            let path = loop {
                let name = if n == 1 { String::from("Yangi papka") } else { alloc::format!("Yangi papka {}", n) };
                let p = path_join(&dir, &name);
                if fs::node_of(&p).is_err() {
                    break p;
                }
                n += 1;
                if n > 99 {
                    return;
                }
            };
            match fs::mkdir(&path) {
                Ok(_) => {
                    let _ = fs::commit();
                    notify("Papka yaratildi", path.rsplit('/').next().unwrap_or(""), 0x2f88ff);
                }
                Err(e) => notify("Papka yaratib bölmadi", &alloc::format!("{:?}", e), 0xff9f0a),
            }
            reload_files(windows, id);
            desk.icons = home_icons();
        }
        CAct::Refresh(id) => reload_files(windows, id),
        CAct::Focus(k) => {
            if let Some(j) = windows.iter().rposition(|w| win_of_launcher(w, k)) {
                let mut w = windows.remove(j);
                w.hidden = false;
                windows.push(w);
            }
        }
        CAct::CloseApp(k) => {
            let mut i = 0;
            while i < windows.len() {
                if win_of_launcher(&windows[i], k) {
                    let mut w = windows.remove(i);
                    close_terminal(&mut w);
                } else {
                    i += 1;
                }
            }
        }
        CAct::App(k) => menu_act(MAct::Open(k), windows, opened),
        CAct::WallSettings => {
            let mut w = new_window(Kind::Settings, *opened);
            w.tab = 1;
            *opened += 1;
            windows.push(w);
        }
        CAct::Sep => {}
    }
}

// ---------------- Tezkor qidiruv (Alt+Space) ----------------

#[derive(Clone)]
enum SAct {
    App(Kind),
    Dir(String),
    File(String),
    Calc,
    Reboot,
    Shutdown,
}

struct Search {
    q: String,
    sel: usize,
    res: Vec<(String, String, SAct)>, // (nom, izoh, amal)
}

const SEARCH_APPS: [(&str, Kind); 15] = [
    ("Terminal", Kind::Terminal),
    ("Fayllar", Kind::Files),
    ("Matn muharriri", Kind::Editor),
    ("Soat", Kind::Clock),
    ("Musiqa", Kind::Music),
    ("Video", Kind::Video),
    ("Rasmlar", Kind::Image),
    ("Brauzer", Kind::Browser),
    ("Sozlamalar", Kind::Settings),
    ("Zamin OS haqida", Kind::About),
    ("Savat", Kind::Trash),
    ("Kalkulyator", Kind::Calc),
    ("Tizim monitori", Kind::Monitor),
    ("Boşqotirma", Kind::Puzzle),
    ("Chizma", Kind::Paint),
];

/// Uy papkasidagi fayl va papkalar (çuqurlik ≤ 3).
fn collect_files(path: &str, depth: usize, out: &mut Vec<(String, String, bool)>) {
    if depth > 3 || out.len() > 400 {
        return;
    }
    for e in fs::list(path).unwrap_or_default() {
        if e.name.starts_with('.') {
            continue;
        }
        let p = path_join(path, &e.name);
        out.push((e.name.clone(), p.clone(), e.is_dir));
        if e.is_dir {
            collect_files(&p, depth + 1, out);
        }
    }
}

/// Oddiy hisoblaş: + - * / ( ) va kasrlar. Natija yoki None.
fn calc(expr: &str) -> Option<f64> {
    let b: Vec<u8> = expr.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if b.is_empty() || !b.iter().any(|c| b"+-*/".contains(c)) || !b.iter().all(|c| b"0123456789.+-*/()".contains(c)) {
        return None;
    }
    fn num(b: &[u8], i: &mut usize) -> Option<f64> {
        if *i < b.len() && b[*i] == b'(' {
            *i += 1;
            let v = sum(b, i)?;
            if *i < b.len() && b[*i] == b')' {
                *i += 1;
                return Some(v);
            }
            return None;
        }
        if *i < b.len() && b[*i] == b'-' {
            *i += 1;
            return num(b, i).map(|v| -v);
        }
        let st = *i;
        while *i < b.len() && (b[*i].is_ascii_digit() || b[*i] == b'.') {
            *i += 1;
        }
        let txt = core::str::from_utf8(&b[st..*i]).ok()?;
        let (ip, fp) = txt.split_once('.').unwrap_or((txt, ""));
        if ip.is_empty() && fp.is_empty() {
            return None;
        }
        let mut v = 0f64;
        for c in ip.bytes() {
            v = v * 10.0 + (c - b'0') as f64;
        }
        let mut sc = 0.1f64;
        for c in fp.bytes() {
            v += (c - b'0') as f64 * sc;
            sc *= 0.1;
        }
        Some(v)
    }
    fn prod(b: &[u8], i: &mut usize) -> Option<f64> {
        let mut v = num(b, i)?;
        while *i < b.len() && (b[*i] == b'*' || b[*i] == b'/') {
            let op = b[*i];
            *i += 1;
            let r = num(b, i)?;
            v = if op == b'*' { v * r } else if r == 0.0 { return None } else { v / r };
        }
        Some(v)
    }
    fn sum(b: &[u8], i: &mut usize) -> Option<f64> {
        let mut v = prod(b, i)?;
        while *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
            let op = b[*i];
            *i += 1;
            let r = prod(b, i)?;
            v = if op == b'+' { v + r } else { v - r };
        }
        Some(v)
    }
    let mut i = 0;
    let v = sum(&b, &mut i)?;
    if i == b.len() { Some(v) } else { None }
}

fn fmt_num(v: f64) -> String {
    if v == (v as i64) as f64 && v.abs() < 1e15 {
        alloc::format!("{}", v as i64)
    } else {
        let s = alloc::format!("{:.6}", v);
        String::from(s.trim_end_matches('0').trim_end_matches('.'))
    }
}

/// Soʻrov böyiça natijalar: hisob, ilovalar, tizim amallari, papka/fayllar.
fn search_results(q: &str) -> Vec<(String, String, SAct)> {
    let mut out = Vec::new();
    let ql = q.trim().to_lowercase();
    if ql.is_empty() {
        for (n, k) in SEARCH_APPS.iter().take(6) {
            out.push((String::from(*n), String::from("Ilova"), SAct::App(*k)));
        }
        return out;
    }
    if let Some(v) = calc(q) {
        out.push((alloc::format!("= {}", fmt_num(v)), alloc::format!("Hisob: {}", q.trim()), SAct::Calc));
    }
    let score = |name: &str| -> Option<u8> {
        let n = name.to_lowercase();
        if n.starts_with(&ql) {
            Some(0)
        } else if n.contains(&ql) {
            Some(1)
        } else {
            None
        }
    };
    let mut apps: Vec<(u8, &str, Kind)> =
        SEARCH_APPS.iter().filter_map(|(n, k)| score(n).map(|sc| (sc, *n, *k))).collect();
    apps.sort_by_key(|a| a.0);
    for (_, n, k) in apps {
        out.push((String::from(n), String::from("Ilova"), SAct::App(k)));
    }
    for (n, a) in [("Qayta yuklaş", SAct::Reboot), ("Öçiriş", SAct::Shutdown)] {
        if score(n).is_some() {
            out.push((String::from(n), String::from("Tizim"), a));
        }
    }
    let mut files = Vec::new();
    collect_files("~", 0, &mut files);
    let mut hits: Vec<(u8, String, String, bool)> =
        files.into_iter().filter_map(|(n, p, d)| score(&n).map(|sc| (sc, n, p, d))).collect();
    hits.sort_by_key(|h| h.0);
    for (_, n, p, d) in hits {
        out.push((n, p.clone(), if d { SAct::Dir(p) } else { SAct::File(p) }));
    }
    out.truncate(8);
    out
}

fn search_geo(n: usize) -> (usize, usize, usize, usize, usize) {
    let (sw, sh) = gfx::size();
    let u = body();
    let w = (u * 30).min(sw - u * 2);
    let bar = u * 5 / 2;
    let row = u * 2;
    let h = bar + if n > 0 { n * row + u / 2 } else { 0 };
    ((sw - w) / 2, sh / 5, w, h, row)
}

fn draw_search(s: &Search) {
    let u = body();
    let (x, y, w, h, row) = search_geo(s.res.len());
    gfx::shadow(x, y, w, h, u, 16);
    fill_round_rect_a(x, y, w, h, u, 0x161d2b, 248);
    gfx::round_outline(x, y, w, h, u, 1, 0x3a4660);
    let bar = u * 5 / 2;
    // lupa
    let (lx, ly) = (x + u * 3 / 2, y + bar / 2);
    let lr = u / 2;
    gfx::fill_circle(lx, ly, lr, 0x9fb0c3);
    gfx::fill_circle(lx, ly, lr * 2 / 3, 0x161d2b);
    gfx::fill_poly(
        &[
            ((lx + lr / 2) as f32, (ly + lr / 2 + 2) as f32),
            ((lx + lr / 2 + 2) as f32, (ly + lr / 2) as f32),
            ((lx + lr * 3 / 2) as f32, (ly + lr * 3 / 2 - 1) as f32),
            ((lx + lr * 3 / 2 - 1) as f32, (ly + lr * 3 / 2) as f32),
        ],
        0x9fb0c3,
    );
    let tx = x + u * 3;
    let big = small() * 6 / 5;
    let ty = y + (bar - big) / 2;
    if s.q.is_empty() {
        gfx::draw_text(tx, ty, "Ilova, fayl yoki hisob…", 0x6f7f95, big);
        fill_rect(tx, ty, 2, big + 2, 0x63a8ff);
    } else {
        let ex = gfx::draw_text(tx, ty, &s.q, OQ, big);
        fill_rect(ex + 1, ty, 2, big + 2, 0x63a8ff);
    }
    if s.res.is_empty() {
        return;
    }
    fill_rect(x + u / 2, y + bar, w - u, 1, 0x2a3346);
    for (k, (name, sub, act)) in s.res.iter().enumerate() {
        let ry = y + bar + u / 4 + k * row;
        if k == s.sel {
            fill_round_rect_a(x + u / 3, ry, w - u * 2 / 3, row, u / 2, accent(), 220);
        }
        let isz = row * 3 / 4;
        let (ix, iy) = (x + u, ry + (row - isz) / 2);
        match act {
            SAct::App(kd) => {
                let col = LAUNCHERS.iter().find(|l| l.kind == *kd).map(|l| l.color).unwrap_or(menu_kind_color(*kd));
                draw_icon(*kd, col, ix, iy, isz);
            }
            SAct::Dir(_) => big_icon(FKind::Dir, name, ix, iy, isz),
            SAct::File(_) => big_icon(FKind::of(name, false), name, ix, iy, isz),
            SAct::Calc => {
                gfx::fill_round_rect(ix, iy, isz, isz, isz / 4, 0xff9f0a);
                gfx::draw_text_centered(ix + isz / 2, iy + isz / 8, "=", OQ, isz * 3 / 4);
            }
            SAct::Reboot | SAct::Shutdown => {
                gfx::fill_circle(ix + isz / 2, iy + isz / 2, isz / 2, if matches!(act, SAct::Shutdown) { 0xff453a } else { 0xff9f0a });
            }
        }
        let nx = ix + isz + u * 2 / 3;
        let nw = w.saturating_sub(isz + u * 3);
        gfx::draw_text(nx, ry + (row - small()) / 2, &fit_text(name, nw / 2 + nw / 4, small()), OQ, small());
        let sw2 = gfx::text_width(&fit_text(sub, nw / 2, small() * 4 / 5), small() * 4 / 5);
        gfx::draw_text(
            (x + w).saturating_sub(sw2 + u),
            ry + (row - small() * 4 / 5) / 2,
            &fit_text(sub, nw / 2, small() * 4 / 5),
            if k == s.sel { 0xdbe6ff } else { 0x6f7f95 },
            small() * 4 / 5,
        );
    }
}

/// Qidiruv natijasini bajaradi.
fn search_exec(act: SAct, windows: &mut Vec<Window>, opened: &mut usize) {
    match act {
        SAct::App(k) => menu_act(MAct::Open(k), windows, opened),
        SAct::Dir(p) => menu_act(MAct::Path(p), windows, opened),
        SAct::File(p) => {
            open_file(windows, &p, *opened);
            *opened += 1;
        }
        SAct::Reboot => menu_act(MAct::Reboot, windows, opened),
        SAct::Shutdown => menu_act(MAct::Shutdown, windows, opened),
        SAct::Calc => {}
    }
}

fn open_search(desk: &mut Desk) {
    desk.menu = None;
    desk.notif_open = false;
    desk.search = Some(Search { q: String::new(), sel: 0, res: search_results("") });
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

/// Qidiruvga oid bosiş: natija — bajaradi, taşqarisi — yopadi, lupa — oçadi.
fn search_click(desk: &mut Desk, windows: &mut Vec<Window>, cx: usize, cy: usize, opened: &mut usize) -> bool {
    if let Some(s) = &desk.search {
        let (x, y, w, h, row) = search_geo(s.res.len());
        if cx >= x && cx < x + w && cy >= y && cy < y + h {
            let bar = body() * 5 / 2;
            if cy >= y + bar + body() / 4 {
                let k = (cy - y - bar - body() / 4) / row;
                if let Some((_, _, a)) = s.res.get(k).cloned() {
                    desk.search = None;
                    search_exec(a, windows, opened);
                }
            }
            return true;
        }
        desk.search = None;
        return true;
    }
    let (sw, _) = gfx::size();
    let (bx, _) = bell_pos(sw);
    let mx = bx.saturating_sub(small() * 3 / 2);
    if cy < menubar_h() && cx + body() * 2 / 3 >= mx && cx <= mx + body() * 2 / 3 {
        open_search(desk);
        return true;
    }
    false
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
        fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir && !e.name.starts_with('.')).map(|e| e.name).collect();
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
            fill_round_rect(x + cw / 2 - lw / 2 - u / 4, ty - 1, lw + u / 2, px + 4, u / 4, accent());
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
    Shot,
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
            (s("Kalkulyator"), MAct::Open(Kind::Calc)),
            (s("Tizim monitori"), MAct::Open(Kind::Monitor)),
            (s("Boşqotirma"), MAct::Open(Kind::Puzzle)),
            (s("Chizma"), MAct::Open(Kind::Paint)),
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
                fs::list("~").unwrap_or_default().into_iter().filter(|e| e.is_dir && !e.name.starts_with('.')).map(|e| e.name).collect();
            dirs.sort();
            for d in dirs {
                let p = alloc::format!("~/{}", d);
                v.push((d, MAct::Path(p)));
            }
            v.push((s("Sep"), MAct::Sep));
            v.push((s("Savat"), MAct::Open(Kind::Trash)));
            v.push((s("Kompyuter (/)"), MAct::Path(s("/"))));
            v
        }
        _ => alloc::vec![
            (s("Zamin OS haqida"), MAct::Open(Kind::About)),
            (s("Sozlamalar"), MAct::Open(Kind::Settings)),
            (s("Tizim monitori"), MAct::Open(Kind::Monitor)),
            (s("Ekran rasmi (PrtSc)"), MAct::Shot),
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
            fill_round_rect_a(x + u / 4, *y0, w - u / 2, y1 - y0, u / 3, accent(), 230);
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
            MAct::Shot => {
                gfx::round_outline(ix, iy + isz / 6, isz, isz * 3 / 4, isz / 6, 1, 0x9fb0c3);
                gfx::fill_circle(ix + isz / 2, iy + isz / 2 + isz / 8, isz / 5, 0x9fb0c3);
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
        MAct::Open(Kind::Image) => open_pictures(windows, *opened),
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
        MAct::Shot => SHOT_REQ.store(true, Ordering::Relaxed),
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
        Kind::Calc => draw_calc(win),
        Kind::Monitor => draw_monitor(win),
        Kind::Puzzle => draw_puzzle(win),
        Kind::Paint => draw_paint(win),
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
            fill_round_rect_a(win.x + u / 3, iy, g.sbw - u * 2 / 3, g.item_h - 2, u / 3, accent(), 80);
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
        let selected = win.fsel == Some(i);
        if selected {
            fill_round_rect_a(x0 + 2, y0, g.cw - 4, g.ch - 2, u / 2, accent(), 70);
        }
        if let Some(Some(t)) = win.thumbs.get(i) {
            fill_round_rect_a(ix, iy, isz, isz, u / 3, 0xffffff, 14);
            let (tx, ty, tw, th) = gfx::blit_fit(ix + 2, iy + 2, isz - 4, isz - 4, t.w, t.h, &t.px);
            gfx::round_outline(tx.saturating_sub(1), ty.saturating_sub(1), tw + 2, th + 2, 2, 1, 0xd8dee9);
        } else {
            big_icon(*kind, name, ix, iy, isz);
        }
        if let (true, Some(r)) = (selected, &win.rename) {
            // Nomini özgartiriş maydoni (oxiri körinadi)
            let (bx, by, bw, bh) = (x0 + 1, y0 + isz + u / 3, g.cw - 2, npx + u / 2);
            gfx::fill_round_rect(bx, by, bw, bh, u / 4, 0x0d1220);
            gfx::round_outline(bx, by, bw, bh, u / 4, 1, accent());
            let v: Vec<char> = r.chars().collect();
            let mut k = v.len();
            while k > 0 {
                let t: String = v[k - 1..].iter().collect();
                if gfx::text_width(&t, npx) > bw - u / 2 {
                    break;
                }
                k -= 1;
            }
            let t: String = v[k..].iter().collect();
            let ex = gfx::draw_text(bx + u / 4, by + (bh - npx) / 2, &t, OQ, npx);
            fill_rect(ex + 1, by + 3, 2, bh - 6, 0x63a8ff);
        } else {
            let lbl = fit_text(name, g.cw - u / 3, npx);
            let col = if selected { OQ } else { 0xe7edf6 };
            gfx::draw_text_centered(x0 + g.cw / 2, y0 + isz + u / 2, &lbl, col, npx);
        }
    }
}

/// Eskizlar keşi: (yöl, fayl ölçami, eskiz).
static THUMB_CACHE: crate::sync::SpinLock<Vec<(String, usize, crate::image::Image)>> = crate::sync::SpinLock::new(Vec::new());

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
    let (_, vis) = term_geom(win);
    let (full, rows) = term_text(win);
    let total = rows.len();
    let max = total.saturating_sub(vis);
    let sc = win.term_scroll.min(max);
    // pastga yopişgan; `sc` vizual qator yuqoriga aylantirilgan bölişi mumkin
    let first = total - vis.min(total) - sc;
    let li_cur = win.lines.len();
    let mut ty = top + pad;
    let mut ex = win.x + pad;
    for &(li, a, b) in &rows[first..(first + vis).min(total)] {
        let src: &str = if li == li_cur { &full } else { &win.lines[li] };
        ex = draw_term_line(win.x + pad, ty, &src[a..b], px);
        ty += lh;
    }
    if sc == 0 {
        // kursor — oxirgi vizual qator oxirida
        gfx::blend_rect(ex, ty - lh, gfx::mono_advance(px), px + 2, 0x5be37d, 190);
    }
    // öng çetda ingiçka aylantiriş çiziği
    if total > vis {
        let track_y = top + pad;
        let track_h = vis * lh;
        let thumb_h = (track_h * vis / total).max(px);
        let pos = (track_h - thumb_h) * first / max;
        let bx = (win.x + win.w).saturating_sub(pad / 2 + 4);
        gfx::blend_rect(bx, track_y, 3, track_h, 0xffffff, 28);
        gfx::blend_rect(bx, track_y + pos, 3, thumb_h, 0xc8d2e0, 170);
    }
}

/// Soat oynasi: analog siferblat + raqamli vaqt va sana.
fn draw_clock(win: &Window) {
    use crate::image::vector::{cos, sin};
    let t = time::now();
    draw_clock_tabs(win);
    if win.tab != 0 {
        draw_clock_extra(win);
        return;
    }
    let top = clock_tabs(win)[0].1 + body() * 2;
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

// ---------------- Mavzu: asosiy rang va fon rasmlari ----------------

const ACCENTS: [Color; 6] = [0x2f88ff, 0x2fbf5f, 0xbf5af2, 0xff9f0a, 0xff375f, 0x14b8c4];
const ACCENT_NAMES: [&str; 6] = ["Kök", "Yaşil", "Binafşa", "Töq sariq", "Pushti", "Firuza"];
const WALL_NAMES: [&str; 4] = ["Zamin manzarasi", "Tun", "Aurora", "Şafaq"];

const WALL_CUSTOM: usize = 4;

/// Fonlar soni: 4 ta ichki + (tanlangan bölsa) foydalanuvçi rasmi.
fn wall_count() -> usize {
    if crate::config::UI_WALL_PATH.lock().is_empty() { 4 } else { 5 }
}
fn wall_name(i: usize) -> &'static str {
    if i == WALL_CUSTOM { "Sizning rasmingiz" } else { WALL_NAMES[i % WALL_NAMES.len()] }
}
/// Hozir tanlangan fon (yaroqsiz bölsa — 0).
fn sel_wall() -> usize {
    let v = crate::config::UI_WALL.load(Ordering::Relaxed) as usize;
    if v < wall_count() { v } else { 0 }
}

/// Rasmni maydonni toʻliq qoplaydigan qilib kesadi va masştablaydi ("cover").
fn cover(img: &crate::image::Image, w: usize, h: usize) -> crate::image::Image {
    let (iw, ih) = (img.w.max(1), img.h.max(1));
    // manbadagi kesiş töğriburçagi
    let (cw, ch) = if iw * h > ih * w { (ih * w / h, ih) } else { (iw, iw * h / w) };
    let (ox, oy) = ((iw - cw.min(iw)) / 2, (ih - ch.min(ih)) / 2);
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = oy + y * ch / h;
        for x in 0..w {
            px.push(img.px[sy.min(ih - 1) * iw + (ox + x * cw / w).min(iw - 1)]);
        }
    }
    crate::image::Image { w, h, px }
}

/// Fonni yasaydi: 0 — foto, 1..3 — protsedural, 4 — foydalanuvçi rasmi.
fn build_wallpaper(idx: usize, w: usize, h: usize, photo: &Option<crate::image::Image>) -> Option<crate::image::Image> {
    match idx {
        0 => photo.as_ref().map(|p| if p.w == w && p.h == h { p.clone() } else { cover(p, w, h) }),
        WALL_CUSTOM => {
            let path = crate::config::UI_WALL_PATH.lock().clone();
            let name = path.rsplit('/').next().unwrap_or("");
            let img = fs::read_all(&path).ok().and_then(|d| crate::image::decode_file(name, &d));
            match img {
                Some(im) => Some(cover(&im, w, h)),
                None => photo.as_ref().map(|p| cover(p, w, h)),
            }
        }
        i => Some(make_wallpaper(i, w, h)),
    }
}

/// Tanlangan asosiy rang (tanlov fonlari, belgilar).
fn accent() -> Color {
    ACCENTS[(crate::config::UI_ACCENT.load(Ordering::Relaxed) as usize) % ACCENTS.len()]
}

/// Foto-fon eskizi (ish stoli fonni dekodlaganda yasaladi).
static PHOTO_THUMB: crate::sync::SpinLock<Option<crate::image::Image>> = crate::sync::SpinLock::new(None);

fn mix(a: Color, b: Color, t: u32) -> Color {
    // t: 0..=256
    let ch = |sh: u32| (((a >> sh) & 0xff) * (256 - t) + ((b >> sh) & 0xff) * t) / 256;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Protsedural fon rasmi (1 — Tün, 2 — Aurora, 3 — Şafaq) istalgan ölçamda.
fn make_wallpaper(idx: usize, w: usize, h: usize) -> crate::image::Image {
    use crate::image::vector::sin;
    let mut px = alloc::vec![0u32; w * h];
    let hash = |x: usize, y: usize| -> u32 {
        let mut v = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663);
        v ^= v >> 13;
        v = v.wrapping_mul(0x5bd1e995);
        v ^ (v >> 15)
    };
    let vgrad = |stops: &[(u32, Color)], t: u32| -> Color {
        // t: 0..=1000
        for k in 1..stops.len() {
            if t <= stops[k].0 {
                let (t0, c0) = stops[k - 1];
                let (t1, c1) = stops[k];
                return mix(c0, c1, ((t - t0) * 256 / (t1 - t0).max(1)).min(256));
            }
        }
        stops[stops.len() - 1].1
    };
    match idx {
        1 => {
            // Tun: chuqur kök osmon, yulduzlar, oy
            let stops = [(0, 0x0d1b3d), (600, 0x081127), (1000, 0x03060e)];
            for y in 0..h {
                let c = vgrad(&stops, (y * 1000 / h.max(1)) as u32);
                for x in 0..w {
                    let hv = hash(x, y);
                    px[y * w + x] = if hv % 1400 == 0 { mix(c, 0xffffff, 120 + (hv >> 24) % 136) } else { c };
                }
            }
            let (mx, my, mr) = (w * 4 / 5, h / 4, (h / 14).max(3));
            for y in my.saturating_sub(mr * 3)..(my + mr * 3).min(h) {
                for x in mx.saturating_sub(mr * 3)..(mx + mr * 3).min(w) {
                    let (dx, dy) = (x as isize - mx as isize, y as isize - my as isize);
                    let d2 = (dx * dx + dy * dy) as usize;
                    let (cx2, cy2) = (dx + mr as isize / 2, dy - mr as isize / 3);
                    let cut = ((cx2 * cx2 + cy2 * cy2) as usize) < mr * mr;
                    if d2 < mr * mr && !cut {
                        px[y * w + x] = 0xf3eed2;
                    } else if d2 < mr * mr * 9 {
                        let t = (256 - (d2 * 256 / (mr * mr * 9)) as u32) / 6;
                        px[y * w + x] = mix(px[y * w + x], 0x8fa6e8, t);
                    }
                }
            }
        }
        2 => {
            // Aurora: toʻq fon va yashil-firuza yogʻdu tasmalari
            let stops = [(0, 0x061a24), (1000, 0x020608)];
            for y in 0..h {
                let c = vgrad(&stops, (y * 1000 / h.max(1)) as u32);
                for x in 0..w {
                    px[y * w + x] = c;
                }
            }
            for band in 0..3 {
                let (col, base, amp, wid) = match band {
                    0 => (0x3cf08a, 0.36, 0.08, 0.06),
                    1 => (0x18c9c4, 0.46, 0.06, 0.05),
                    _ => (0x8a5cff, 0.26, 0.05, 0.04),
                };
                for x in 0..w {
                    let fx = x as f32 / w as f32;
                    let cy = h as f32 * (base + amp * sin(fx * 7.0 + band as f32 * 1.7) + 0.02 * sin(fx * 23.0));
                    let sig = h as f32 * wid;
                    let y0 = (cy - sig * 3.0).max(0.0) as usize;
                    let y1 = ((cy + sig * 1.2) as usize).min(h);
                    for y in y0..y1 {
                        let d = (y as f32 - cy) / sig;
                        let k = if d < 0.0 { 1.0 / (1.0 + d * d * 0.35) } else { 1.0 / (1.0 + d * d * 4.0) };
                        let t = (k * 200.0) as u32;
                        px[y * w + x] = mix(px[y * w + x], col, t.min(256));
                    }
                }
            }
            for y in 0..h {
                for x in 0..w {
                    if hash(x, y) % 2500 == 0 && y < h * 2 / 3 {
                        px[y * w + x] = mix(px[y * w + x], 0xffffff, 160);
                    }
                }
            }
        }
        _ => {
            // Şafaq: quyosh botişi, quyoş va ikki qavat togʻ
            let stops = [(0, 0x241a52), (380, 0x7d3a7a), (620, 0xff7a59), (760, 0xffc46b), (1000, 0xffc46b)];
            for y in 0..h {
                let c = vgrad(&stops, (y * 1000 / h.max(1)) as u32);
                for x in 0..w {
                    px[y * w + x] = c;
                }
            }
            let (sx, sy, sr) = (w / 2, h * 66 / 100, (h / 9).max(3));
            for y in sy.saturating_sub(sr)..(sy + sr).min(h) {
                for x in sx.saturating_sub(sr)..(sx + sr).min(w) {
                    let (dx, dy) = (x as isize - sx as isize, y as isize - sy as isize);
                    if ((dx * dx + dy * dy) as usize) < sr * sr {
                        px[y * w + x] = mix(px[y * w + x], 0xfff1c9, 220);
                    }
                }
            }
            for (layer, col) in [(0, 0x5a2e6b), (1, 0x2a1838)] {
                for x in 0..w {
                    let fx = x as f32 / w as f32;
                    let top = if layer == 0 {
                        h as f32 * (0.62 + 0.07 * sin(fx * 9.0 + 1.0) + 0.03 * sin(fx * 27.0))
                    } else {
                        h as f32 * (0.74 + 0.06 * sin(fx * 6.0 + 2.5) + 0.025 * sin(fx * 31.0))
                    };
                    for y in (top.max(0.0) as usize)..h {
                        px[y * w + x] = col;
                    }
                }
            }
        }
    }
    crate::image::Image { w, h, px }
}

const SETTINGS_TABS: [(&str, Color); 6] = [
    ("Umumiy", 0x8a93a6),
    ("Ekran", 0x2f88ff),
    ("Övoz", 0xff375f),
    ("Tarmoq", 0x34c759),
    ("Foydalanuvçilar", 0xff9f0a),
    ("Xavfsizlik", 0xbf5af2),
];

/// Sozlamalar geometriyasi: (yon panel eni, mazmun x, mazmun y, qator balandligi).
fn settings_geo(win: &Window) -> (usize, usize, usize, usize) {
    let u = body();
    let sbw = u * 9;
    (sbw, win.x + sbw + u, win.y + title_h() + u * 2 / 3, u * 7 / 4)
}

/// Ekran bölimidagi fon eskizi joyi (2×2 grid).
fn wall_thumb_rect(win: &Window, i: usize) -> (usize, usize, usize, usize) {
    let u = body();
    let (_, x, y, _) = settings_geo(win);
    let rw = (win.x + win.w).saturating_sub(x + u);
    let tw = (rw - u * 2) / 3;
    let th = tw * 9 / 16;
    let y0 = y + u * 2 + small() + u / 2;
    (x + (i % 3) * (tw + u), y0 + (i / 3) * (th + u * 3 / 2), tw, th)
}

/// Asosiy rang namunasi markazi va radiusi.
fn accent_swatch(win: &Window, i: usize) -> (usize, usize, usize) {
    let u = body();
    let (_, x, _, _) = settings_geo(win);
    let (_, ty, _, th) = wall_thumb_rect(win, 3);
    let y = ty + th + u * 2 + small() + u / 2;
    (x + u * 3 / 4 + i * (u * 2), y, u * 3 / 4)
}

/// Kalit–qiymat qatorlari jadvali.
fn kv_table(x: usize, y: usize, rw: usize, rows: &[(&str, String)]) {
    let u = body();
    let rh = u * 3 / 2;
    fill_round_rect_a(x, y, rw, rh * rows.len(), u / 2, 0xffffff, 12);
    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = y + i * rh;
        if i > 0 {
            fill_rect(x + u / 2, ry, rw - u, 1, 0x2a3346);
        }
        let ty = ry + (rh - small()) / 2;
        gfx::draw_text(x + u / 2, ty, k, 0xdbe3ee, small() * 9 / 10);
        let v = fit_text(v, rw / 2, small() * 9 / 10);
        let vw = gfx::text_width(&v, small() * 9 / 10);
        gfx::draw_text((x + rw).saturating_sub(vw + u / 2), ty, &v, 0x8a9ab0, small() * 9 / 10);
    }
}

fn ip_str(ip: [u8; 4]) -> String {
    alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Sozlamalar oynasi: çapda bölimlar, öngda tanlangan bölim.
fn draw_settings(win: &Window) {
    let u = body();
    let top = win.y + title_h();
    let (sbw, x, y0, ih) = settings_geo(win);
    let r = radius();
    let bottom = win.y + win.h;
    fill_rect(win.x, top, sbw, (bottom - top).saturating_sub(r), 0x151b27);
    gfx::fill_round_rect(win.x, bottom.saturating_sub(2 * r), sbw, 2 * r, r, 0x151b27);
    fill_rect(win.x + r, bottom.saturating_sub(2 * r), sbw - r, 2 * r, 0x151b27);
    fill_rect(win.x + sbw, top, 1, bottom - top, 0x2a3346);
    for (i, (name, col)) in SETTINGS_TABS.iter().enumerate() {
        let iy = top + u / 2 + i * ih;
        if i == win.tab {
            fill_round_rect_a(win.x + u / 3, iy, sbw - u * 2 / 3, ih - 2, u / 3, accent(), 90);
        }
        let isz = small();
        gfx::fill_round_rect(win.x + u * 2 / 3, iy + (ih - isz) / 2, isz, isz, isz / 4, *col);
        let lbl = fit_text(name, sbw - u * 3, small() * 9 / 10);
        gfx::draw_text(win.x + u * 2 / 3 + isz + u / 2, iy + (ih - small()) / 2, &lbl, 0xdbe3ee, small() * 9 / 10);
    }
    let rw = (win.x + win.w).saturating_sub(x + u);
    gfx::draw_text(x, y0, SETTINGS_TABS[win.tab].0, OQ, u * 6 / 5);
    let y = y0 + u * 2;
    match win.tab {
        1 => {
            // Ekran: fon rasmi + asosiy rang
            gfx::draw_text(x, y, "Fon rasmi", 0x9fb0c3, small() * 9 / 10);
            let cur = sel_wall();
            for i in 0..win.thumbs.len() {
                let (tx, ty, tw, th) = wall_thumb_rect(win, i);
                if let Some(Some(t)) = win.thumbs.get(i) {
                    gfx::blit_scaled(tx, ty, tw, th, t.w, t.h, &t.px);
                } else {
                    fill_rect(tx, ty, tw, th, 0x222a3a);
                }
                if i == cur {
                    gfx::round_outline(tx.saturating_sub(3), ty.saturating_sub(3), tw + 6, th + 6, u / 3, 3, accent());
                } else {
                    gfx::round_outline(tx, ty, tw, th, 2, 1, 0x3a4458);
                }
                gfx::draw_text_centered(tx + tw / 2, ty + th + u / 4, wall_name(i), if i == cur { OQ } else { 0xaebccf }, small() * 4 / 5);
            }
            let (_, sy, sr) = accent_swatch(win, 0);
            gfx::draw_text(x, sy - sr - small() - u / 3, "Asosiy rang", 0x9fb0c3, small() * 9 / 10);
            let ca = crate::config::UI_ACCENT.load(Ordering::Relaxed) as usize % ACCENTS.len();
            for (i, c) in ACCENTS.iter().enumerate() {
                let (cx2, cy2, rr) = accent_swatch(win, i);
                if i == ca {
                    gfx::fill_circle(cx2, cy2, rr + 4, OQ);
                }
                gfx::fill_circle(cx2, cy2, rr, *c);
            }
            let (lx, ly, _) = accent_swatch(win, ACCENTS.len());
            gfx::draw_text(lx, ly.saturating_sub(small() / 2), ACCENT_NAMES[ca], 0xdbe3ee, small() * 9 / 10);
        }
        2 => {
            let present = crate::audio::sound::present();
            kv_table(x, y, rw, &[
                ("Qurilma", String::from(if present { "virtio-sound (faol)" } else { "Topilmadi" })),
                ("Çiqiş", String::from("PCM S16LE, 2 kanal")),
                ("Dekoderlar", String::from("WAV · AIFF · FLAC · MP3 · OGG")),
                ("Kodeklar", String::from("PCM · mu-law · A-law · ADPCM")),
            ]);
            if !present {
                gfx::draw_text(x, y + u * 7, "QEMU: -device virtio-sound-device bilan övoz eşitiladi", 0x6f7f95, small() * 4 / 5);
            }
        }
        3 => {
            let up = crate::net::present();
            let m = crate::net::our_mac();
            let mac = alloc::format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", m[0], m[1], m[2], m[3], m[4], m[5]);
            let cfg = crate::net::is_configured();
            kv_table(x, y, rw, &[
                ("Qurilma", String::from(if up { "virtio-net" } else { "Topilmadi" })),
                ("Holat", String::from(if cfg { "Ulangan (DHCP)" } else if up { "Sozlanmagan" } else { "—" })),
                ("IP manzil", if cfg { ip_str(crate::net::our_ip()) } else { String::from("—") }),
                ("Şlyuz", if cfg { ip_str(crate::net::gateway()) } else { String::from("—") }),
                ("DNS", if cfg { ip_str(crate::net::dns_server()) } else { String::from("—") }),
                ("MAC", if up { mac } else { String::from("—") }),
            ]);
        }
        4 => {
            let me = accounts::current().map(|a| a.username).unwrap_or_default();
            let list = accounts::all();
            let rh = u * 5 / 2;
            fill_round_rect_a(x, y, rw, rh * list.len().max(1), u / 2, 0xffffff, 12);
            for (i, a) in list.iter().enumerate() {
                let ry = y + i * rh;
                if i > 0 {
                    fill_rect(x + u / 2, ry, rw - u, 1, 0x2a3346);
                }
                let ar = u * 3 / 4;
                gfx::fill_circle(x + u + ar / 2, ry + rh / 2, ar, 0x1f8a46);
                let ini = a.full_name.chars().next().unwrap_or('?').to_uppercase().collect::<String>();
                gfx::draw_text_centered(x + u + ar / 2, ry + rh / 2 - small() / 2, &ini, OQ, small());
                let tx = x + u * 3;
                gfx::draw_text(tx, ry + u / 3, &a.full_name, OQ, small());
                gfx::draw_text(tx, ry + u / 3 + small() + 2, &alloc::format!("@{} · uid {}", a.username, a.uid), 0x8a9ab0, small() * 4 / 5);
                let mut badge = String::new();
                if a.admin {
                    badge.push_str("Administrator");
                }
                if a.username == me {
                    if !badge.is_empty() {
                        badge.push_str(" · ");
                    }
                    badge.push_str("Siz");
                }
                let bw = gfx::text_width(&badge, small() * 4 / 5);
                gfx::draw_text((x + rw).saturating_sub(bw + u / 2), ry + (rh - small()) / 2, &badge, accent(), small() * 4 / 5);
            }
        }
        5 => {
            let items = [
                ("W^X", "Kod — faqat öqiş, maʼlumot — bajarilmaydi"),
                ("PAN", "Yadro foydalanuvçi xotirasiga faqat nusxa orqali"),
                ("KASLR", "Yadro steklari har yuklanişda tasodifiy joyda"),
                ("Stek qörğoni", "Töliş aniqlanadi — faqat aybdor oqim töxtaydi"),
                ("Disk şifrlaş", "ZaminFS: ChaCha20-Poly1305"),
                ("Parollar", "PBKDF2-SHA256, kalit örab saqlanadi"),
                ("TLS", "X.509: muddat va host nomi tekşiruvi"),
            ];
            let rh = u * 2;
            fill_round_rect_a(x, y, rw, rh * items.len(), u / 2, 0xffffff, 12);
            for (i, (k, v)) in items.iter().enumerate() {
                let ry = y + i * rh;
                if i > 0 {
                    fill_rect(x + u / 2, ry, rw - u, 1, 0x2a3346);
                }
                gfx::fill_circle(x + u, ry + rh / 2, u / 3, 0x2fbf5f);
                gfx::draw_text(x + u * 2, ry + u / 5, k, OQ, small() * 9 / 10);
                gfx::draw_text(x + u * 2, ry + u / 5 + small(), &fit_text(v, rw - u * 3, small() * 4 / 5), 0x8a9ab0, small() * 4 / 5);
            }
        }
        _ => {
            let (sw, sh) = gfx::size();
            let (free_p, total_p) = crate::memory::frame::stats();
            let user = accounts::current().map(|a| a.username).unwrap_or_default();
            kv_table(x, y, rw, &[
                ("Tizim", String::from("Zamin OS")),
                ("Versiya", String::from(env!("CARGO_PKG_VERSION"))),
                ("Foydalanuvçi", user),
                ("Protsessor", String::from("AArch64 · 4 yadro")),
                ("Xotira", alloc::format!("{} / {} MB", (total_p - free_p) * 4 / 1024, total_p * 4 / 1024)),
                ("Ekran", alloc::format!("{} x {}", sw, sh)),
                ("Ish vaqti", { let s = crate::timer::uptime_ms() / 1000; alloc::format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) }),
            ]);
        }
    }
}

/// Sozlamalar oynasiga bosiş (bölim, fon, rang). Bosildi — true.
fn settings_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    let u = body();
    let (sbw, _, _, ih) = settings_geo(win);
    let top = win.y + title_h();
    if cx >= win.x && cx < win.x + sbw && cy >= top + u / 2 {
        let i = (cy - top - u / 2) / ih;
        if i < SETTINGS_TABS.len() {
            win.tab = i;
            return true;
        }
    }
    if win.tab == 1 {
        for i in 0..win.thumbs.len() {
            let (tx, ty, tw, th) = wall_thumb_rect(win, i);
            if cx >= tx && cx < tx + tw && cy >= ty && cy < ty + th {
                crate::config::UI_WALL.store(i as u8, Ordering::Relaxed);
                let _ = crate::config::save_current();
                notify("Sozlamalar", &alloc::format!("Fon: {}", wall_name(i)), 0x8a93a6);
                return true;
            }
        }
        for i in 0..ACCENTS.len() {
            let (sx, sy, sr) = accent_swatch(win, i);
            let (dx, dy) = (cx as isize - sx as isize, cy as isize - sy as isize);
            if ((dx * dx + dy * dy) as usize) <= (sr + 3) * (sr + 3) {
                crate::config::UI_ACCENT.store(i as u8, Ordering::Relaxed);
                let _ = crate::config::save_current();
                notify("Sozlamalar", &alloc::format!("Asosiy rang: {}", ACCENT_NAMES[i]), ACCENTS[i]);
                return true;
            }
        }
    }
    false
}

/// Brauzer oynasi: manzil qatori + boşlang'iç sahifa.


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
        if l.kind == Kind::Trash && trash_count() > 0 {
            // Tola savat: ustida qogʻozlar
            gfx::fill_circle(ix + isz * 2 / 5, iy + isz * 3 / 10, isz / 10, 0xf4f6fa);
            gfx::fill_circle(ix + isz * 3 / 5, iy + isz / 4, isz / 11, 0xdfe6f0);
            gfx::fill_circle(ix + isz / 2, iy + isz / 5, isz / 12, 0xffffff);
        }
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
        Kind::Calc => {
            fill_round_rect(x + s / 4, y + s / 5, s / 2, s / 6, s / 24, 0x3a2400);
            let q = s / 9;
            let g = s / 18;
            let gx = c - (q * 3 + g * 2) / 2;
            for row in 0..2 {
                for col in 0..3 {
                    let cc = if row == 1 && col == 2 { 0x3a2400 } else { OQ };
                    gfx::fill_round_rect(gx + col * (q + g), y + s * 9 / 20 + row * (q + g), q, q, q / 3, cc);
                }
            }
        }
        Kind::Monitor => {
            fill_round_rect(x + s / 6, y + s / 4, s * 2 / 3, s / 2, s / 12, 0x0b2a18);
            let t = (s as f32 / 26.0).max(1.0);
            let (bx, by) = ((x + s / 6) as f32, (y + s / 2) as f32);
            let w = (s * 2 / 3) as f32;
            let pts = [(0.08, 0.0), (0.3, 0.0), (0.4, -0.16), (0.52, 0.14), (0.62, -0.06), (0.7, 0.0), (0.92, 0.0)];
            for k in 0..pts.len() - 1 {
                let (a, b) = (pts[k], pts[k + 1]);
                seg(bx + a.0 * w, by + a.1 * s as f32, bx + b.0 * w, by + b.1 * s as f32, t, 0x5cf08e);
            }
        }
        Kind::Info => {
            gfx::draw_text_centered(c, y + s / 6, "i", OQ, s * 3 / 5);
        }
        Kind::Paint => {
            // palitra
            let r = s * 3 / 10;
            gfx::fill_circle(c, y + m, r, OQ);
            gfx::fill_circle(c + r / 2, y + m + r / 3, r / 4, color);
            for (k, col) in [0xff3b30u32, 0xffcc00, 0x007aff, 0x34c759].iter().enumerate() {
                let a = -2.6 + k as f32 * 0.75;
                let px = c as f32 + crate::image::vector::cos(a) * r as f32 * 0.55;
                let py = (y + m) as f32 + crate::image::vector::sin(a) * r as f32 * 0.55;
                gfx::fill_circle(px as usize, py as usize, (r / 6).max(2), *col);
            }
        }
        Kind::Puzzle => {
            let q = s * 9 / 40;
            let g = s / 20;
            let x0 = c - q - g / 2;
            let y0 = y + s / 2 - q - g / 2;
            for k in 0..4 {
                if k == 3 {
                    continue;
                }
                let (tx, ty) = (x0 + (k % 2) * (q + g), y0 + (k / 2) * (q + g));
                gfx::fill_round_rect(tx, ty, q, q, q / 4, OQ);
                gfx::draw_text_centered(tx + q / 2, ty + q / 8, ["1", "2", "3"][k], color, q * 3 / 4);
            }
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
    if let Some(c) = altgr_char(code, shift) {
        return Some(c);
    }
    let c = match code {
        2..=11 => {
            let row = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
            let sh = ['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'];
            return Some(if shift { sh } else { row }[(code - 2) as usize]);
        }
        12 => return Some(if shift { '_' } else { '-' }),
        13 => return Some(if shift { '+' } else { '=' }),
        26 => return Some(if shift { '{' } else { '[' }),
        27 => return Some(if shift { '}' } else { ']' }),
        39 => return Some(if shift { ':' } else { ';' }),
        40 => return Some(if shift { '"' } else { '\'' }),
        41 => return Some(if shift { '~' } else { '`' }),
        43 => return Some(if shift { '|' } else { '\\' }),
        51 => return Some(if shift { '<' } else { ',' }),
        52 => return Some(if shift { '>' } else { '.' }),
        53 => return Some(if shift { '?' } else { '/' }),
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't', 21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g', 35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b', 49 => 'n', 50 => 'm',
        57 => ' ',
        _ => return None,
    };
    Some(if shift { c.to_ascii_uppercase() } else { c })
}

// ---------------- Yordamçi chiziq va belgilar ----------------

/// Qalin kesma (to'rtburçak poligon).
fn seg(x0: f32, y0: f32, x1: f32, y1: f32, t: f32, c: Color) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let l = crate::image::vector::fsqrt(dx * dx + dy * dy).max(0.001);
    let (nx, ny) = (-dy / l * t, dx / l * t);
    gfx::fill_poly(&[(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)], c);
}

/// Arifmetik amal belgisi (+ − × ÷ =) — shriftda yöq belgilar uçun chiziladi.
fn op_glyph(cx: usize, cy: usize, s: usize, op: &str, c: Color) {
    let (x, y, h) = (cx as f32, cy as f32, s as f32 / 2.0);
    let t = (s as f32 / 14.0).max(1.0);
    match op {
        "+" => {
            seg(x - h, y, x + h, y, t, c);
            seg(x, y - h, x, y + h, t, c);
        }
        "-" => seg(x - h, y, x + h, y, t, c),
        "*" => {
            let d = h * 0.75;
            seg(x - d, y - d, x + d, y + d, t, c);
            seg(x - d, y + d, x + d, y - d, t, c);
        }
        "/" => {
            seg(x - h, y, x + h, y, t, c);
            let r = (t * 1.4) as usize + 1;
            gfx::fill_circle(cx, (y - h * 0.6) as usize, r, c);
            gfx::fill_circle(cx, (y + h * 0.6) as usize, r, c);
        }
        "=" => {
            seg(x - h, y - h * 0.35, x + h, y - h * 0.35, t, c);
            seg(x - h, y + h * 0.35, x + h, y + h * 0.35, t, c);
        }
        _ => gfx::draw_text_centered(cx, cy.saturating_sub(s / 2), op, c, s),
    }
}

// ---------------- Savat (haqiqiy: ~/.Savat) ----------------

const TRASH_DIR: &str = "~/.Savat";
const TRASH_INDEX: &str = "~/.Savat/.joylar";
/// Savatdagi elementlar soni (dok ikonasi uçun keş); MAX — hali hisoblanmagan.
static TRASH_N: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Savat tarkibi: (savatdagi nom, asl yöl).
fn trash_entries() -> Vec<(String, String)> {
    let text = fs::read_all(TRASH_INDEX).map(|b| String::from(String::from_utf8_lossy(&b))).unwrap_or_default();
    let mut v: Vec<(String, String)> =
        text.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (String::from(a), String::from(b))).collect();
    let present: Vec<String> =
        fs::list(TRASH_DIR).unwrap_or_default().into_iter().filter(|e| !e.name.starts_with('.')).map(|e| e.name).collect();
    v.retain(|(n, _)| present.contains(n));
    for n in present {
        if !v.iter().any(|(a, _)| *a == n) {
            let orig = path_join("~", &n);
            v.push((n, orig));
        }
    }
    v
}

fn write_trash_index(v: &[(String, String)]) {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    let mut text = String::new();
    for (a, b) in v {
        text.push_str(a);
        text.push('\t');
        text.push_str(b);
        text.push('\n');
    }
    if let Ok(id) = fs::open(TRASH_INDEX, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE) {
        let _ = fs::write(id, Some(0), text.as_bytes());
    }
    TRASH_N.store(v.len(), Ordering::Relaxed);
}

fn trash_count() -> usize {
    let n = TRASH_N.load(Ordering::Relaxed);
    if n != usize::MAX {
        return n;
    }
    let n = trash_entries().len();
    TRASH_N.store(n, Ordering::Relaxed);
    n
}

/// Band bölmagan yöl: "nom.txt" → "nom (2).txt" va h.k.
fn unique_dest(p: &str) -> String {
    if fs::node_of(p).is_err() {
        return String::from(p);
    }
    let (dir, name) = p.rsplit_once('/').unwrap_or(("", p));
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, alloc::format!(".{}", e)),
        _ => (name, String::new()),
    };
    for n in 2..1000 {
        let c = alloc::format!("{}/{} ({}){}", dir, stem, n, ext);
        if fs::node_of(&c).is_err() {
            return c;
        }
    }
    String::from(p)
}

/// Papkani içidagilari bilan birga öçiradi.
fn remove_path(p: &str) -> Result<(), fs::FsError> {
    if let Ok(list) = fs::list(p) {
        for e in list {
            remove_path(&path_join(p, &e.name))?;
        }
    }
    fs::remove(p)
}

/// Fayl yoki papkani (içidagilari bilan) nusxalaydi.
fn copy_path(src: &str, dst: &str, depth: usize) -> Result<(), String> {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    if depth > 16 {
        return Err(String::from("juda çuqur papka"));
    }
    match fs::list(src) {
        Ok(list) => {
            fs::mkdir(dst).map_err(|e| alloc::format!("{:?}", e))?;
            for e in list {
                copy_path(&path_join(src, &e.name), &path_join(dst, &e.name), depth + 1)?;
            }
            Ok(())
        }
        Err(_) => {
            let data = fs::read_all(src).map_err(|e| alloc::format!("{:?}", e))?;
            let id = fs::open(dst, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE).map_err(|e| alloc::format!("{:?}", e))?;
            fs::write(id, Some(0), &data).map_err(|e| alloc::format!("{:?}", e))?;
            Ok(())
        }
    }
}

/// Faylni savatga köçiradi (savatning özidagisi — butunlay öçiriladi).
fn to_trash(p: &str) -> Result<String, String> {
    let name = String::from(p.rsplit('/').next().unwrap_or(p));
    if p.starts_with("~/.Savat/") {
        remove_path(p).map_err(|e| alloc::format!("{}: {:?}", name, e))?;
        TRASH_N.store(usize::MAX, Ordering::Relaxed);
        let _ = fs::commit();
        return Ok(name);
    }
    let _ = fs::mkdir(TRASH_DIR);
    let dest = unique_dest(&path_join(TRASH_DIR, &name));
    fs::rename(p, &dest).map_err(|e| alloc::format!("{}: {:?}", name, e))?;
    let tname = String::from(dest.rsplit('/').next().unwrap_or(""));
    let mut v = trash_entries();
    v.retain(|(a, _)| *a != tname);
    v.push((tname, String::from(p)));
    write_trash_index(&v);
    let _ = fs::commit();
    Ok(name)
}

/// Savatdan asl joyiga qaytaradi (asl papka yöq bölsa — uy papkasiga).
fn trash_restore(tname: &str) -> Result<String, String> {
    let mut v = trash_entries();
    let orig = v.iter().find(|(a, _)| a == tname).map(|(_, b)| b.clone()).unwrap_or_else(|| path_join("~", tname));
    let parent_ok = orig.rsplit_once('/').is_some_and(|(d, _)| fs::list(if d.is_empty() { "/" } else { d }).is_ok());
    let base_name = String::from(orig.rsplit('/').next().unwrap_or(tname));
    let target = if parent_ok { orig.clone() } else { path_join("~", &base_name) };
    let dest = unique_dest(&target);
    fs::rename(&path_join(TRASH_DIR, tname), &dest).map_err(|e| alloc::format!("{:?}", e))?;
    v.retain(|(a, _)| a != tname);
    write_trash_index(&v);
    let _ = fs::commit();
    Ok(dest)
}

/// Bitta elementni savatdan butunlay öçiradi.
fn trash_purge(tname: &str) -> Result<(), String> {
    remove_path(&path_join(TRASH_DIR, tname)).map_err(|e| alloc::format!("{:?}", e))?;
    let mut v = trash_entries();
    v.retain(|(a, _)| a != tname);
    write_trash_index(&v);
    let _ = fs::commit();
    Ok(())
}

/// Savatni böşatadi. Natija — öçirilganlar soni.
fn empty_trash() -> usize {
    let v = trash_entries();
    let n = v.iter().filter(|(a, _)| remove_path(&path_join(TRASH_DIR, a)).is_ok()).count();
    let rest = trash_entries();
    write_trash_index(&rest);
    let _ = fs::commit();
    n
}

fn load_trash(win: &mut Window) {
    let v = trash_entries();
    TRASH_N.store(v.len(), Ordering::Relaxed);
    win.rows = v
        .iter()
        .map(|(n, _)| (n.clone(), FKind::of(n, fs::list(&path_join(TRASH_DIR, n)).is_ok())))
        .collect();
    win.side = v.into_iter().map(|(_, o)| o).collect();
    win.fsel = win.fsel.filter(|&s| s < win.rows.len());
    win.title = if win.rows.is_empty() {
        String::from("Savat")
    } else {
        alloc::format!("Savat — {} ta element", win.rows.len())
    };
}

/// Barça Fayllar va Savat oynalarini yangilaydi (fayl tizimi özgarganda).
fn reload_all_files(windows: &mut [Window]) {
    for w in windows.iter_mut() {
        match w.kind {
            Kind::Files => {
                let p = w.path.clone();
                let sel = w.fsel;
                load_dir(w, &p);
                w.fsel = sel.filter(|&s| s < w.rows.len());
            }
            Kind::Trash => load_trash(w),
            _ => {}
        }
    }
}

/// Savat oynasi: (ro'yxat x, y, eni, qator balandligi, sigʻadigan qatorlar).
fn trash_list_geo(win: &Window) -> (usize, usize, usize, usize, usize) {
    let u = body();
    let ly = win.y + title_h() + u * 5 / 2 + u / 4;
    let rh = u * 2;
    let max = (win.y + win.h).saturating_sub(ly + u / 2) / rh;
    (win.x + u / 2, ly, win.w.saturating_sub(u), rh, max)
}

fn trash_row_at(win: &Window, cx: usize, cy: usize) -> Option<usize> {
    let (lx, ly, lw, rh, max) = trash_list_geo(win);
    if cx < lx || cx >= lx + lw || cy < ly {
        return None;
    }
    let i = (cy - ly) / rh;
    if i < max && i < win.rows.len() { Some(i) } else { None }
}

/// Savat tugmalari: [Tiklaş, Böşatiş] — (x, y, w, h).
fn trash_btns(win: &Window) -> [(usize, usize, usize, usize); 2] {
    let u = body();
    let bar = u * 5 / 2;
    let bh = u * 3 / 2;
    let by = win.y + title_h() + (bar - bh) / 2;
    let w1 = gfx::text_width("Böşatiş", small()) + u * 2;
    let x1 = (win.x + win.w).saturating_sub(u / 2 + w1);
    let w0 = gfx::text_width("Tiklaş", small()) + u * 2;
    let x0 = x1.saturating_sub(u / 2 + w0);
    [(x0, by, w0, bh), (x1, by, w1, bh)]
}

fn in_rect(r: (usize, usize, usize, usize), cx: usize, cy: usize) -> bool {
    cx >= r.0 && cx < r.0 + r.2 && cy >= r.1 && cy < r.1 + r.3
}

/// Savat oynasiga bosiş: 0 — tegişli emas, 1 — işlandi, 2 — fayl tizimi özgardi.
fn trash_click(win: &mut Window, cx: usize, cy: usize) -> u8 {
    if win.rows.is_empty() {
        return 0;
    }
    let [rb, eb] = trash_btns(win);
    if in_rect(rb, cx, cy) {
        if let Some(name) = win.fsel.and_then(|s| win.rows.get(s)).map(|r| r.0.clone()) {
            match trash_restore(&name) {
                Ok(dest) => notify("Tiklandi", &dest, 0x2fbf5f),
                Err(e) => notify("Tiklab bölmadi", &e, 0xff453a),
            }
            win.fsel = None;
            return 2;
        }
        return 1;
    }
    if in_rect(eb, cx, cy) {
        let n = empty_trash();
        notify("Savat böşatildi", &alloc::format!("{} ta element butunlay öçirildi", n), 0xff453a);
        win.fsel = None;
        return 2;
    }
    if let Some(r) = trash_row_at(win, cx, cy) {
        win.fsel = Some(r);
        return 1;
    }
    0
}

/// Savat oynasi: böş holat yoki elementlar ro'yxati.
fn draw_trash(win: &Window) {
    let u = body();
    if win.rows.is_empty() {
        let cx = win.x + win.w / 2;
        let top = win.y + title_h();
        let s = u * 4;
        let tw = s * 3 / 5;
        let th = s * 2 / 3;
        let x = cx - tw / 2;
        let y = top + (win.h.saturating_sub(title_h() + s + u * 4)) / 2;
        gfx::fill_round_rect(x - tw / 8, y, tw * 5 / 4, s / 10, s / 30, 0x8a93a6);
        gfx::fill_round_rect(x, y + s / 8, tw, th, tw / 8, 0x5a6476);
        for k in 0..3 {
            fill_rect(x + tw / 4 + k * tw / 4 - 1, y + s / 8 + th / 6, (s / 30).max(2), th * 2 / 3, 0x3a4252);
        }
        gfx::draw_text_centered(cx, y + s / 8 + th + u, "Savat böş", 0xe7edf6, body());
        gfx::draw_text_centered(cx, y + s / 8 + th + u * 5 / 2, "Öçirilgan fayllar şu yerda turadi", 0x6f7f95, small() * 9 / 10);
        return;
    }
    // Asboblar qatori
    let top = win.y + title_h();
    let bar = u * 5 / 2;
    fill_rect(win.x + 1, top, win.w - 2, bar, 0x1f2636);
    fill_rect(win.x + 1, top + bar, win.w - 2, 1, 0x2a3346);
    let info = alloc::format!("{} ta element", win.rows.len());
    gfx::draw_text(win.x + u, top + (bar - small()) / 2, &info, 0xaebccf, small());
    let [rb, eb] = trash_btns(win);
    let can = win.fsel.is_some();
    gfx::fill_round_rect(rb.0, rb.1, rb.2, rb.3, rb.3 / 3, if can { accent() } else { 0x323b4d });
    gfx::draw_text_centered(rb.0 + rb.2 / 2, rb.1 + (rb.3 - small()) / 2, "Tiklaş", if can { OQ } else { 0x6f7f95 }, small());
    gfx::fill_round_rect(eb.0, eb.1, eb.2, eb.3, eb.3 / 3, 0x5c2427);
    gfx::draw_text_centered(eb.0 + eb.2 / 2, eb.1 + (eb.3 - small()) / 2, "Böşatiş", 0xff8a80, small());
    // Ro'yxat
    let (lx, ly, lw, rh, max) = trash_list_geo(win);
    for (i, (name, kind)) in win.rows.iter().enumerate().take(max) {
        let ry = ly + i * rh;
        if win.fsel == Some(i) {
            fill_round_rect_a(lx, ry, lw, rh - 2, u / 3, accent(), 90);
        } else if i % 2 == 1 {
            fill_round_rect_a(lx, ry, lw, rh - 2, u / 3, 0xffffff, 8);
        }
        let isz = rh * 3 / 4;
        big_icon(*kind, name, lx + u / 2, ry + (rh - isz) / 2 - 1, isz);
        let nx = lx + u / 2 + isz + u / 2;
        gfx::draw_text(nx, ry + (rh - small()) / 2 - 1, &fit_text(name, lw / 2, small()), 0xe7edf6, small());
        let orig = win.side.get(i).map(|o| o.rsplit_once('/').map(|(d, _)| d).unwrap_or("~")).unwrap_or("~");
        let o = fit_text(&alloc::format!("Asl joyi: {}", orig), lw / 2 - u, small() * 4 / 5);
        let ow = gfx::text_width(&o, small() * 4 / 5);
        gfx::draw_text((lx + lw).saturating_sub(ow + u / 2), ry + (rh - small() * 4 / 5) / 2, &o, 0x7d8ca3, small() * 4 / 5);
    }
}

// ---------------- Fayllar: tanlaş, klaviatura, bufer ----------------

/// Nomini özgartirişni yakunlaydi (yangi nom böş yoki öşa bölsa — bekor).
fn files_rename_commit(win: &mut Window) {
    let Some(new) = win.rename.take() else { return };
    let Some(old) = win.fsel.and_then(|s| win.rows.get(s)).map(|r| r.0.clone()) else { return };
    let new = String::from(new.trim());
    if new.is_empty() || new == old || new.contains('/') || new.starts_with('.') {
        return;
    }
    let from = path_join(&win.path, &old);
    let to = path_join(&win.path, &new);
    match fs::rename(&from, &to) {
        Ok(()) => {
            let _ = fs::commit();
            notify("Nomi özgartirildi", &alloc::format!("{} — {}", old, new), 0x2f88ff);
        }
        Err(e) => notify("Nomini özgartirib bölmadi", &alloc::format!("{}: {:?}", old, e), 0xff453a),
    }
    let p = win.path.clone();
    load_dir(win, &p);
    win.fsel = win.rows.iter().position(|r| r.0 == new);
}

/// Tanlangan faylni savatga tashlaydi.
fn files_trash_sel(windows: &mut Vec<Window>, wi: usize, desk: &mut Desk) {
    let w = &windows[wi];
    let Some(name) = w.fsel.and_then(|s| w.rows.get(s)).map(|r| r.0.clone()) else { return };
    let p = path_join(&w.path, &name);
    match to_trash(&p) {
        Ok(n) => notify("Savatga tashlandi", &n, 0xff9f0a),
        Err(e) => notify("Savatga tashlab bölmadi", &e, 0xff453a),
    }
    reload_all_files(windows);
    desk.icons = home_icons();
}

/// Buferga olish (nusxa yoki kesish).
fn clip_set(desk: &mut Desk, p: String, cut: bool) {
    let name = String::from(p.rsplit('/').next().unwrap_or(""));
    notify(if cut { "Kesildi" } else { "Nusxa olindi" }, &alloc::format!("{} — joylaş: Alt+V", name), 0x2f88ff);
    desk.fclip = Some((p, cut));
}

/// Buferdagini `dir` papkasiga joylaydi.
fn paste_into(windows: &mut Vec<Window>, desk: &mut Desk, dir: &str) {
    let Some((src, cut)) = desk.fclip.clone() else {
        notify("Bufer böş", "Avval nusxa oling yoki kesing", 0x8a93a6);
        return;
    };
    let name = String::from(src.rsplit('/').next().unwrap_or(""));
    let src_dir = src.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    if cut && src_dir == dir {
        desk.fclip = None;
        return;
    }
    let dst = unique_dest(&path_join(dir, &name));
    if dst.starts_with(&alloc::format!("{}/", src)) {
        notify("Joylab bölmadi", "Papkani öz içiga joylab bölmaydi", 0xff453a);
        return;
    }
    let r = if cut { fs::rename(&src, &dst).map_err(|e| alloc::format!("{:?}", e)) } else { copy_path(&src, &dst, 0) };
    match r {
        Ok(()) => {
            let _ = fs::commit();
            if cut {
                desk.fclip = None;
            }
            notify(if cut { "Köçirildi" } else { "Nusxalandi" }, dst.rsplit('/').next().unwrap_or(""), 0x2fbf5f);
        }
        Err(e) => notify("Joylab bölmadi", &alloc::format!("{}: {}", name, e), 0xff453a),
    }
    reload_all_files(windows);
    desk.icons = home_icons();
}

/// Alt+C / Alt+X / Alt+V — faol Fayllar oynasida. Işlangan bölsa true.
fn files_clip_key(windows: &mut Vec<Window>, desk: &mut Desk, code: u16) -> bool {
    let Some(w) = windows.last().filter(|w| !w.hidden && w.kind == Kind::Files) else { return false };
    match code {
        46 | 45 => {
            if let Some(name) = w.fsel.and_then(|s| w.rows.get(s)).map(|r| r.0.clone()) {
                let p = path_join(&w.path, &name);
                clip_set(desk, p, code == 45);
            }
        }
        _ => {
            let dir = w.path.clone();
            paste_into(windows, desk, &dir);
        }
    }
    true
}

/// Fayllar oynasi klaviaturasi: strelkalar, Enter, Backspace, Delete, F2.
fn files_key(windows: &mut Vec<Window>, desk: &mut Desk, code: u16, shift: bool, opened: &mut usize) {
    let wi = windows.len() - 1;
    {
        let w = &mut windows[wi];
        if let Some(r) = w.rename.as_mut() {
            match code {
                28 => {
                    files_rename_commit(w);
                    desk.icons = home_icons();
                }
                14 => {
                    r.pop();
                }
                _ => {
                    if let Some(c) = keycode_char(code, shift) {
                        if c != '/' && r.chars().count() < 60 {
                            r.push(c);
                        }
                    }
                }
            }
            return;
        }
    }
    let (n, cols, cur) = {
        let w = &windows[wi];
        (w.rows.len(), files_geo(w).cols, w.fsel)
    };
    match code {
        105 | 106 | 103 | 108 if n > 0 => {
            let c = match (cur, code) {
                (None, _) => 0,
                (Some(c), 105) => c.saturating_sub(1),
                (Some(c), 106) => (c + 1).min(n - 1),
                (Some(c), 103) => if c >= cols { c - cols } else { c },
                (Some(c), _) => if c + cols < n { c + cols } else { c },
            };
            windows[wi].fsel = Some(c);
        }
        28 => {
            if let Some(c) = cur {
                files_activate(windows, wi, c, opened);
            }
        }
        14 => {
            let w = &mut windows[wi];
            if w.path != "~" && w.path != "/" {
                let p = path_join(&w.path, "..");
                load_dir(w, &p);
            }
        }
        111 => files_trash_sel(windows, wi, desk),
        60 => {
            let w = &mut windows[wi];
            if let Some(c) = cur {
                w.rename = Some(w.rows[c].0.clone());
            }
        }
        _ => {}
    }
}

// ---------------- Kalkulyator ----------------

/// Tugmalar: (yozuv, tur) — 0: amal (kulrang), 1: arifmetika, 2: raqam, 3: "=".
const CALC_KEYS: [(&str, u8); 19] = [
    ("C", 0), ("+/-", 0), ("%", 0), ("/", 1),
    ("7", 2), ("8", 2), ("9", 2), ("*", 1),
    ("4", 2), ("5", 2), ("6", 2), ("-", 1),
    ("1", 2), ("2", 2), ("3", 2), ("+", 1),
    ("0", 2), (".", 2), ("=", 3),
];

/// Kalkulyator geometriyasi: displey (x, y, w, h) va tugma töğriburçaklari.
fn calc_geo(win: &Window) -> ((usize, usize, usize, usize), Vec<(usize, usize, usize, usize)>) {
    let u = body();
    let pad = u * 2 / 3;
    let dx = win.x + pad;
    let dy = win.y + title_h() + pad;
    let dw = win.w.saturating_sub(pad * 2);
    let dh = u * 4;
    let gy = dy + dh + pad;
    let gap = u / 2;
    let bw = dw.saturating_sub(gap * 3) / 4;
    let bh = ((win.y + win.h).saturating_sub(gy + pad)).saturating_sub(gap * 4) / 5;
    let mut r = Vec::new();
    for k in 0..CALC_KEYS.len() {
        let (row, col, w) = match k {
            16 => (4, 0, bw * 2 + gap),
            17 => (4, 2, bw),
            18 => (4, 3, bw),
            _ => (k / 4, k % 4, bw),
        };
        r.push((dx + col * (bw + gap), gy + row * (bh + gap), w, bh));
    }
    ((dx, dy, dw, dh), r)
}

fn calc_eval(e: &str) -> Option<f64> {
    calc(&alloc::format!("0+({})", e))
}

/// Tugma bosilişini bajaradi (`tab == 1` — natija körsatilmoqda).
fn calc_press(win: &mut Window, key: &str) {
    let fresh = win.tab == 1;
    let ops = "+-*/";
    match key {
        "C" => {
            win.input = String::from("0");
            win.lines.clear();
            win.tab = 0;
        }
        "=" => {
            let e = win.input.clone();
            match calc_eval(&e) {
                Some(v) if v.is_finite() => {
                    win.lines = alloc::vec![alloc::format!("{} =", e)];
                    win.input = fmt_num(v);
                    win.tab = 1;
                }
                _ => win.lines = alloc::vec![String::from("Xato")],
            }
        }
        "+/-" => {
            let s = win.input.clone();
            win.input = if let Some(inner) = s.strip_prefix("-(").and_then(|r| r.strip_suffix(')')) {
                String::from(inner)
            } else if let Some(r) = s.strip_prefix('-').filter(|r| !r.contains(|c| ops.contains(c))) {
                String::from(r)
            } else if s == "0" {
                s
            } else if s.contains(|c| ops.contains(c)) {
                alloc::format!("-({})", s)
            } else {
                alloc::format!("-{}", s)
            };
        }
        "%" => {
            if let Some(v) = calc_eval(&win.input) {
                win.input = fmt_num(v / 100.0);
                win.tab = 1;
            }
        }
        "back" => {
            win.input.pop();
            if win.input.is_empty() {
                win.input = String::from("0");
            }
            win.tab = 0;
        }
        k => {
            let is_op = ops.contains(k);
            if fresh && !is_op {
                win.input.clear();
            }
            win.tab = 0;
            if win.input == "0" && !is_op && k != "." && k != ")" {
                win.input.clear();
            }
            if is_op && win.input.chars().last().is_some_and(|l| ops.contains(l)) {
                win.input.pop();
            }
            if win.input.len() < 28 {
                win.input.push_str(k);
            }
        }
    }
}

fn calc_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    let (_, keys) = calc_geo(win);
    if let Some(k) = keys.iter().position(|&r| in_rect(r, cx, cy)) {
        calc_press(win, CALC_KEYS[k].0);
        win.lclick = (crate::timer::uptime_ms(), k); // bosilgan tugma (yorituv uçun)
        return true;
    }
    false
}

fn calc_key(win: &mut Window, code: u16, shift: bool) {
    match code {
        28 => calc_press(win, "="),
        14 => calc_press(win, "back"),
        111 => calc_press(win, "C"),
        _ => match keycode_char(code, shift) {
            Some('=') => calc_press(win, "="),
            Some('c') | Some('C') => calc_press(win, "C"),
            Some(',') => calc_press(win, "."),
            Some(c) if c.is_ascii_digit() || "+-*/.%()".contains(c) => {
                let mut b = [0u8; 4];
                let s = c.encode_utf8(&mut b);
                if c == '%' {
                    calc_press(win, "%");
                } else {
                    let s = String::from(&*s);
                    calc_press(win, &s);
                }
            }
            _ => {}
        },
    }
}

fn draw_calc(win: &Window) {
    let u = body();
    let ((dx, dy, dw, dh), keys) = calc_geo(win);
    gfx::fill_round_rect(dx, dy, dw, dh, u / 2, 0x10151f);
    if let Some(l) = win.lines.first() {
        let t = fit_text(l, dw - u, small());
        let tw = gfx::text_width(&t, small());
        let col = if l == "Xato" { 0xff6b61 } else { 0x7d8ca3 };
        gfx::draw_text(dx + dw - tw - u / 2, dy + u / 2, &t, col, small());
    }
    // Asosiy son: sigʻmasa — kiçraytiriladi
    let mut px = u * 2;
    while px > small() && gfx::text_width(&win.input, px) > dw - u {
        px -= 2;
    }
    let shown = if gfx::text_width(&win.input, px) > dw - u {
        let v: Vec<char> = win.input.chars().collect();
        let mut k = v.len();
        let mut s = String::new();
        while k > 0 {
            let t: String = v[k - 1..].iter().collect();
            if gfx::text_width(&t, px) > dw - u {
                break;
            }
            s = t;
            k -= 1;
        }
        s
    } else {
        win.input.clone()
    };
    let tw = gfx::text_width(&shown, px);
    gfx::draw_text(dx + dw - tw - u / 2, dy + dh - px - u / 2, &shown, OQ, px);
    let flash = crate::timer::uptime_ms().saturating_sub(win.lclick.0) < 150;
    for (k, &(x, y, w, h)) in keys.iter().enumerate() {
        let (label, t) = CALC_KEYS[k];
        let mut c = match t {
            0 => 0x5a6476,
            1 | 3 => 0xff9f0a,
            _ => 0x2c3446,
        };
        if flash && win.lclick.1 == k {
            c = mix(c, 0xffffff, 90);
        }
        gfx::fill_round_rect(x, y, w, h, h.min(w) / 2, c);
        let (bx, by) = (x + w / 2, y + h / 2);
        let gs = (h * 2 / 5).min(u * 3 / 2);
        if t == 1 || t == 3 {
            op_glyph(bx, by, gs, label, OQ);
        } else {
            let px = if label.len() > 1 { small() } else { u * 6 / 5 };
            gfx::draw_text_centered(bx, by.saturating_sub(px / 2 + 1), label, OQ, px);
        }
    }
}

// ---------------- Tizim monitori ----------------

/// Oxirgi 60 soniya CPU yuklanişi (%).
static CPU_HIST: crate::sync::SpinLock<Vec<u32>> = crate::sync::SpinLock::new(Vec::new());

fn state_name(s: &task::State) -> &'static str {
    match s {
        task::State::Ready => "Tayyor",
        task::State::Running => "Işlayapti",
        task::State::Sleeping(_) => "Uxlayapti",
        task::State::WaitingInput => "Kiriş kutmoqda",
        task::State::WaitingPid(_) => "Kutmoqda",
        task::State::WaitingChannel(_) => "Kanal kutmoqda",
        task::State::WaitingEvent(_) => "Hodisa kutmoqda",
        task::State::Dead(_) => "Tugagan",
    }
}

/// Monitor geometriyasi: (kartalar y, karta balandligi, jadval y, qator, Tugatiş tugmasi).
fn monitor_geo(win: &Window) -> (usize, usize, usize, usize, (usize, usize, usize, usize)) {
    let u = body();
    let pad = u * 2 / 3;
    let cy = win.y + title_h() + pad;
    let ch = u * 6;
    let ty = cy + ch + pad;
    let rh = u * 3 / 2;
    let bh = u * 3 / 2;
    let bw = gfx::text_width("Tugatiş", small()) + u * 2;
    let btn = ((win.x + win.w).saturating_sub(pad + bw), (win.y + win.h).saturating_sub(pad + bh), bw, bh);
    (cy, ch, ty, rh, btn)
}

fn monitor_tasks() -> Vec<task::TaskInfo> {
    let mut v = task::snapshot();
    v.retain(|t| !matches!(t.state, task::State::Dead(_)));
    v.sort_by_key(|t| t.pid);
    v
}

/// Tanlov kaliti: PID (boş vazifalar hammasi PID 0 — ular qator raqami bilan).
fn task_key(t: &task::TaskInfo, idx: usize) -> usize {
    if t.pid == 0 { usize::MAX - idx } else { t.pid }
}

fn task_sel<'a>(tasks: &'a [task::TaskInfo], sel: Option<usize>) -> Option<&'a task::TaskInfo> {
    let k = sel?;
    tasks.iter().enumerate().find(|(i, t)| task_key(t, *i) == k).map(|(_, t)| t)
}

fn monitor_rows_max(win: &Window) -> usize {
    let (_, _, ty, rh, btn) = monitor_geo(win);
    btn.1.saturating_sub(ty + rh + body() / 3) / rh
}

fn monitor_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    let (_, _, ty, rh, btn) = monitor_geo(win);
    if in_rect(btn, cx, cy) {
        let tasks = monitor_tasks();
        if let Some(t) = task_sel(&tasks, win.fsel) {
            if !t.user {
                notify("Tugatib bölmaydi", &alloc::format!("{} — yadro vazifasi", t.name), 0xff9f0a);
            } else if task::kill(t.pid) {
                notify("Jarayon tugatildi", &alloc::format!("{} (PID {})", t.name, t.pid), 0xff453a);
                win.fsel = None;
            } else {
                notify("Tugatib bölmadi", &alloc::format!("{}: ruxsat yöq", t.name), 0xff453a);
            }
        }
        return true;
    }
    let first = ty + rh;
    if cy >= first && cx >= win.x && cx < win.x + win.w {
        let i = (cy - first) / rh;
        if i < monitor_rows_max(win) {
            let tasks = monitor_tasks();
            if let Some(t) = tasks.get(i) {
                win.fsel = Some(task_key(t, i));
                return true;
            }
        }
    }
    false
}

fn draw_monitor(win: &Window) {
    let u = body();
    let pad = u * 2 / 3;
    let (cy, ch, ty, rh, btn) = monitor_geo(win);
    let iw = win.w.saturating_sub(pad * 2);
    let cw0 = iw * 11 / 20;
    let cw1 = iw - cw0 - pad;
    let x0 = win.x + pad;
    let x1 = x0 + cw0 + pad;
    // --- CPU kartasi ---
    gfx::fill_round_rect(x0, cy, cw0, ch, u / 2, 0x141a26);
    let hist = CPU_HIST.lock().clone();
    let cur = hist.last().copied().unwrap_or(0);
    gfx::draw_text(x0 + u * 2 / 3, cy + u / 2, "Protsessor", 0xaebccf, small());
    let pct = alloc::format!("{}%", cur);
    let pw = gfx::text_width(&pct, u * 6 / 5);
    gfx::draw_text(x0 + cw0 - pw - u * 2 / 3, cy + u / 3, &pct, 0x34c759, u * 6 / 5);
    let (gx, gy) = (x0 + u * 2 / 3, cy + u * 2);
    let (gw, gh) = (cw0.saturating_sub(u * 4 / 3), ch.saturating_sub(u * 5 / 2));
    for k in 1..4 {
        fill_rect(gx, gy + gh * k / 4, gw, 1, 0x222b3b);
    }
    if hist.len() >= 2 && gw > 0 {
        let n = 60usize;
        let off = n.saturating_sub(hist.len()); // böş boşlanişi — öngda
        for px in 0..gw {
            let pos = px * (n - 1) * 16 / gw.max(1);
            let (i, f) = (pos / 16, (pos % 16) as u32);
            if i < off {
                continue;
            }
            let a = hist[(i - off).min(hist.len() - 1)];
            let b = hist[(i + 1 - off).min(hist.len() - 1)];
            let v = (a * (16 - f) + b * f) / 16;
            let h = (v as usize * gh / 100).min(gh);
            if h > 0 {
                fill_rect(gx + px, gy + gh - h, 1, h, 0x1d5a35);
                fill_rect(gx + px, gy + gh - h, 1, 2.min(h), 0x34c759);
            }
        }
    }
    // --- Xotira kartasi ---
    gfx::fill_round_rect(x1, cy, cw1, ch, u / 2, 0x141a26);
    gfx::draw_text(x1 + u * 2 / 3, cy + u / 2, "Xotira", 0xaebccf, small());
    let (free_p, total_p) = crate::memory::frame::stats();
    let used_p = total_p.saturating_sub(free_p);
    let mpct = used_p * 100 / total_p.max(1);
    let mtxt = alloc::format!("{}%", mpct);
    let mw = gfx::text_width(&mtxt, u * 6 / 5);
    gfx::draw_text(x1 + cw1 - mw - u * 2 / 3, cy + u / 3, &mtxt, 0x2f9bff, u * 6 / 5);
    let (bx, by, bw, bh) = (x1 + u * 2 / 3, cy + u * 2 + u / 4, cw1.saturating_sub(u * 4 / 3), u / 2);
    gfx::fill_round_rect(bx, by, bw, bh, bh / 2, 0x262f40);
    gfx::fill_round_rect(bx, by, (bw * mpct / 100).max(bh), bh, bh / 2, 0x2f9bff);
    let (hu, ht) = crate::memory::heap::stats();
    let (files, dirs, bytes) = fs::stats();
    let lines = [
        alloc::format!("RAM: {} / {} MB", used_p * 4 / 1024, total_p * 4 / 1024),
        alloc::format!("Uyum: {} / {} KB", hu / 1024, ht / 1024),
        alloc::format!("Disk: {} fayl, {} papka, {} KB", files, dirs, bytes / 1024),
    ];
    for (k, l) in lines.iter().enumerate() {
        gfx::draw_text(bx, by + bh + u / 2 + k * (small() + 3), &fit_text(l, bw, small() * 4 / 5), 0x8ea0b6, small() * 4 / 5);
    }
    // --- Jarayonlar jadvali ---
    let tasks = monitor_tasks();
    let cols = [x0 + u / 2, x0 + u * 3, x0 + iw * 9 / 20, x0 + iw * 7 / 10, x0 + iw * 7 / 8];
    fill_round_rect_a(x0, ty, iw, rh, u / 3, 0xffffff, 12);
    for (k, h) in ["PID", "Nomi", "Holati", "Turi", "Yadro"].iter().enumerate() {
        gfx::draw_text(cols[k], ty + (rh - small() * 4 / 5) / 2, h, 0x8ea0b6, small() * 4 / 5);
    }
    let max = monitor_rows_max(win);
    for (i, t) in tasks.iter().take(max).enumerate() {
        let ry = ty + rh + i * rh;
        if win.fsel == Some(task_key(t, i)) {
            fill_round_rect_a(x0, ry, iw, rh, u / 3, accent(), 110);
        } else if i % 2 == 1 {
            fill_round_rect_a(x0, ry, iw, rh, u / 3, 0xffffff, 6);
        }
        let ty2 = ry + (rh - small() * 9 / 10) / 2;
        let px = small() * 9 / 10;
        let col = if t.user { 0xe7edf6 } else { 0xaab6c8 };
        gfx::draw_text(cols[0], ty2, &alloc::format!("{}", t.pid), col, px);
        gfx::draw_text(cols[1], ty2, &fit_text(&t.name, cols[2] - cols[1] - u / 2, px), col, px);
        let sc = match t.state {
            task::State::Running => 0x34c759,
            task::State::Ready => 0x64d2ff,
            _ => 0x8ea0b6,
        };
        gfx::draw_text(cols[2], ty2, state_name(&t.state), sc, px);
        gfx::draw_text(cols[3], ty2, if t.user { "Dastur" } else { "Yadro" }, col, px);
        gfx::draw_text(cols[4], ty2, &alloc::format!("{}", t.cpu), col, px);
    }
    // --- Pastki qator ---
    let info = alloc::format!("{} ta vazifa · {} ta dastur", tasks.len(), tasks.iter().filter(|t| t.user).count());
    gfx::draw_text(x0, btn.1 + (btn.3 - small()) / 2, &info, 0x8ea0b6, small());
    let can = task_sel(&tasks, win.fsel).is_some_and(|t| t.user);
    gfx::fill_round_rect(btn.0, btn.1, btn.2, btn.3, btn.3 / 3, if can { 0xd83b33 } else { 0x323b4d });
    gfx::draw_text_centered(btn.0 + btn.2 / 2, btn.1 + (btn.3 - small()) / 2, "Tugatiş", if can { OQ } else { 0x6f7f95 }, small());
}

fn monitor_key(win: &mut Window, code: u16) {
    let tasks = monitor_tasks();
    if tasks.is_empty() {
        return;
    }
    let cur = win.fsel.and_then(|k| tasks.iter().enumerate().position(|(i, t)| task_key(t, i) == k));
    let i = match (code, cur) {
        (103, Some(c)) => c.saturating_sub(1),
        (108, Some(c)) => (c + 1).min(tasks.len() - 1),
        (103 | 108, None) => 0,
        _ => return,
    };
    win.fsel = Some(task_key(&tasks[i], i));
}

// ---------------- Rasm va video köruvçi boşqaruvi ----------------

/// Köruvçi geometriyasi: rasm maydoni (x, y, w, h) va pastki qator y, balandligi.
fn viewer_geo(win: &Window) -> (usize, usize, usize, usize, usize, usize) {
    let pad = body() / 2;
    let x = win.x + pad;
    let y = win.content_top();
    let w = win.w.saturating_sub(pad * 2);
    let foot = if win.video { body() * 2 } else { small() + pad };
    let h = (win.y + win.h).saturating_sub(y + pad + foot);
    (x, y, w, h, y + h + pad / 2, foot)
}

/// Video boşqaruvi: (play markazi x, y, radius, chiziq x0, x1).
fn video_bar(win: &Window) -> (usize, usize, usize, usize, usize) {
    let u = body();
    let (x, _, w, _, fy, foot) = viewer_geo(win);
    let r = u * 2 / 3;
    let py = fy + foot / 2;
    (x + r, py, r, x + r * 2 + u, (x + w).saturating_sub(u * 5))
}

/// Oldingi/keyingi strelkalari markazlari (rasm uçun).
fn viewer_arrows(win: &Window) -> [(usize, usize, usize); 2] {
    let u = body();
    let (x, y, w, h, _, _) = viewer_geo(win);
    let r = u;
    let my = y + h / 2;
    [(x + r + u / 3, my, r), ((x + w).saturating_sub(r + u / 3), my, r)]
}

fn in_circle(c: (usize, usize, usize), px: usize, py: usize) -> bool {
    let (dx, dy) = (px as isize - c.0 as isize, py as isize - c.1 as isize);
    (dx * dx + dy * dy) as usize <= (c.2 + 3) * (c.2 + 3)
}

fn viewer_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    if win.video {
        if win.frames.is_empty() {
            return false;
        }
        let (px, py, r, x0, x1) = video_bar(win);
        if in_circle((px, py, r), cx, cy) {
            video_toggle(win);
            return true;
        }
        if cx + 4 >= x0 && cx <= x1 + 4 && cy + r >= py && cy <= py + r && x1 > x0 {
            let f = (cx.clamp(x0, x1) - x0) * (win.frames.len() - 1) / (x1 - x0);
            win.frame = f;
            win.frame_ms = crate::timer::uptime_ms();
            return true;
        }
        return false;
    }
    if win.path.is_empty() || (win.img.is_none() && win.frames.is_empty()) {
        return false;
    }
    let [l, r] = viewer_arrows(win);
    if in_circle(l, cx, cy) {
        image_step(win, -1);
        return true;
    }
    if in_circle(r, cx, cy) {
        image_step(win, 1);
        return true;
    }
    false
}

fn video_toggle(win: &mut Window) {
    win.playing = !win.playing;
    if win.playing && win.frame + 1 >= win.frames.len() {
        win.frame = 0; // oxirida — boşidan
    }
    win.frame_ms = crate::timer::uptime_ms();
}

fn image_key(win: &mut Window, code: u16) {
    if win.video {
        let n = win.frames.len();
        match code {
            57 => video_toggle(win),
            105 if n > 0 => {
                win.playing = false;
                win.frame = win.frame.saturating_sub(1);
            }
            106 if n > 0 => {
                win.playing = false;
                win.frame = (win.frame + 1).min(n - 1);
            }
            _ => {}
        }
        return;
    }
    match code {
        105 => image_step(win, -1),
        106 => image_step(win, 1),
        _ => {}
    }
}

fn draw_image(win: &Window) {
    let img = if !win.frames.is_empty() {
        &win.frames[win.frame.min(win.frames.len() - 1)].0
    } else if let Some(img) = &win.img {
        img
    } else {
        return;
    };
    let u = body();
    let (x, y, w, h, fy, foot) = viewer_geo(win);
    gfx::fill_rect(x, y, w, h, 0x11151d);
    let (ix, iy, iw, ih) = gfx::blit_fit(x, y, w, h, img.w, img.h, &img.px);
    gfx::round_outline(ix.saturating_sub(1), iy.saturating_sub(1), iw + 2, ih + 2, 1, 1, 0x3a4458);
    if win.video {
        let (px, py, r, x0, x1) = video_bar(win);
        gfx::fill_circle(px, py, r, accent());
        if win.playing {
            fill_rect(px - r / 3, py - r / 2, (r / 4).max(2), r, OQ);
            fill_rect(px + r / 8, py - r / 2, (r / 4).max(2), r, OQ);
        } else {
            let t = r * 2 / 5;
            gfx::fill_poly(
                &[((px - t / 2) as f32, (py - t) as f32), ((px + t) as f32, py as f32), ((px - t / 2) as f32, (py + t) as f32)],
                OQ,
            );
        }
        let n = win.frames.len().max(1);
        let th = (u / 4).max(3);
        gfx::fill_round_rect(x0, py - th / 2, x1.saturating_sub(x0), th, th / 2, 0x2a3346);
        let kx = x0 + if n > 1 { win.frame * (x1 - x0) / (n - 1) } else { 0 };
        gfx::fill_round_rect(x0, py - th / 2, (kx - x0).max(th), th, th / 2, accent());
        gfx::fill_circle(kx, py, th * 3 / 2, OQ);
        // vaqt: kadrlar davomiyligi yiğindisi
        let ms = |k: usize| -> u32 { win.frames.iter().take(k).map(|f| f.1.max(20)).sum() };
        let (pos, dur) = (ms(win.frame), ms(n));
        let tt = alloc::format!("{}:{:02} / {}:{:02}", pos / 60000, (pos / 1000) % 60, dur / 60000, (dur / 1000) % 60);
        gfx::draw_text(x1 + u / 2, py.saturating_sub(small() * 4 / 10), &tt, 0xaebccf, small() * 4 / 5);
        let _ = (fy, foot);
        return;
    }
    // Oldingi / keyingi (papkada bir nechta rasm bölsa)
    let sib = if win.path.is_empty() { Vec::new() } else { image_siblings(&win.path) };
    if sib.len() > 1 {
        for (k, (ax, ay, r)) in viewer_arrows(win).into_iter().enumerate() {
            fill_round_rect_a(ax - r, ay - r, r * 2, r * 2, r, 0x000000, 130);
            let d = r as f32 * 0.35;
            let (fx, fy2) = (ax as f32, ay as f32);
            let s = if k == 0 { 1.0 } else { -1.0 };
            let t = (r as f32 / 9.0).max(1.2);
            seg(fx + d * 0.5 * s, fy2 - d, fx - d * 0.5 * s, fy2, t, OQ);
            seg(fx - d * 0.5 * s, fy2, fx + d * 0.5 * s, fy2 + d, t, OQ);
        }
    }
    let pos = sib.iter().position(|p| *p == win.path);
    let info = match pos {
        Some(p) if sib.len() > 1 => alloc::format!("{}x{} · {} / {} · strelkalar bilan almaştiring", img.w, img.h, p + 1, sib.len()),
        _ => alloc::format!("{}x{} · {} piksel", img.w, img.h, img.w * img.h),
    };
    gfx::draw_text(x, fy + (foot - small()) / 2, &fit_text(&info, w, small()), 0x8aa0bc, small());
}

// ---------------- Matn muharriri (haqiqiy kursor) ----------------

fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

/// Muharrir geometriyasi: (matn y boşi, matn x, qator balandligi, körinadigan qatorlar, gutter).
fn editor_geo(win: &Window) -> (usize, usize, usize, usize, usize) {
    let px = small();
    let lh = gfx::line_height(px, Font::Mono).max(px + 3);
    let u = body();
    let (_, by, _, bh) = win.save_btn();
    let top = by + bh + u / 3;
    let ty0 = top + u / 3;
    let digits = if win.lines.len() > 999 { 4 } else { 3 };
    let gut = gfx::mono_advance(px) * digits + u / 2;
    let bar = if win.find.is_some() { find_bar_h() } else { 0 };
    let rows = ((win.y + win.h).saturating_sub(u / 2 + ty0 + bar) / lh).max(1);
    (ty0, win.x + gut + u / 2, lh, rows, gut)
}

/// Qidiruv paneli balandligi (muharrir pastida).
fn find_bar_h() -> usize {
    body() * 2
}

fn editor_fix(w: &mut Window) {
    if w.lines.is_empty() {
        w.lines.push(String::new());
    }
    w.crow = w.crow.min(w.lines.len() - 1);
    w.ccol = w.ccol.min(w.lines[w.crow].chars().count());
    let (_, _, _, rows, _) = editor_geo(w);
    if w.crow < w.scroll {
        w.scroll = w.crow;
    }
    if w.crow >= w.scroll + rows {
        w.scroll = w.crow + 1 - rows;
    }
}

fn editor_insert(w: &mut Window, s: &str) {
    if w.lines[w.crow].chars().count() >= 400 {
        return;
    }
    let ws = s.chars().all(|c| c == ' ' || c == '\t');
    editor_record(w, if s.chars().count() == 1 || ws { crate::edit::EditKind::Type { ws } } else { crate::edit::EditKind::Paste });
    let line = &mut w.lines[w.crow];
    let b = byte_at(line, w.ccol);
    line.insert_str(b, s);
    w.ccol += s.chars().count();
    w.dirty = true;
}

/// Tahrirdan oldin holatni tarixga yozadi.
fn editor_record(w: &mut Window, kind: crate::edit::EditKind) {
    let (lines, r, c) = (&w.lines, w.crow, w.ccol);
    w.hist.record(kind, lines, r, c);
}

/// Alt+Z — bekor qiliş, Alt+Shift+Z — qaytariş. Holat va kursor tiklanadi.
fn editor_undo(w: &mut Window, redo: bool) -> bool {
    let cur = crate::edit::Snapshot { lines: core::mem::take(&mut w.lines), row: w.crow, col: w.ccol };
    let back = cur.clone();
    let got = if redo { w.hist.redo(cur) } else { w.hist.undo(cur) };
    let ok = got.is_some();
    let snap = got.unwrap_or(back);
    w.lines = snap.lines;
    w.crow = snap.row;
    w.ccol = snap.col;
    if ok {
        w.dirty = true;
        w.cur = Vec::new();
    } else {
        w.cur = String::from(if redo { "Qaytariladigan amal yöq" } else { "Bekor qilinadigan amal yöq" }).into_bytes();
    }
    editor_fix(w);
    ok
}

/// Alt+K — qidiruv panelini oçadi/yopadi.
fn editor_find_toggle(w: &mut Window) {
    w.find = if w.find.is_some() { None } else { Some(String::new()) };
    w.find_at = 0;
    editor_fix(w);
}

/// Qidiruv: `step` — keyingi moslikka ötiş (oxiridan boşiga aylanadi);
/// aks holda kursordan boşlab birinçi moslikka.
fn editor_find_go(w: &mut Window, step: bool) {
    let Some(q) = w.find.as_ref() else { return };
    let m = crate::edit::find_in_lines(&w.lines, q);
    if m.is_empty() {
        w.find_at = 0;
        return;
    }
    w.find_at = if step {
        (w.find_at + 1) % m.len()
    } else {
        m.iter().position(|&p| p >= (w.crow, w.ccol)).unwrap_or(0)
    };
    let (r, c) = m[w.find_at];
    w.crow = r;
    w.ccol = c;
    w.hist.break_group();
    editor_fix(w);
}

/// Qidiruv maydoni oçiq: tugmalar unga tegişli.
fn editor_find_key(w: &mut Window, code: u16, shift: bool) {
    let Some(q) = w.find.as_mut() else { return };
    match code {
        28 => editor_find_go(w, true),
        14 => {
            q.pop();
            editor_find_go(w, false);
        }
        _ => {
            if let Some(ch) = keycode_char(code, shift) {
                if q.chars().count() < 60 {
                    q.push(ch);
                }
                editor_find_go(w, false);
            }
        }
    }
}

fn editor_key(w: &mut Window, code: u16, shift: bool) {
    editor_fix(w);
    if w.find.is_some() {
        editor_find_key(w, code, shift);
        return;
    }
    use crate::edit::EditKind;
    let (_, _, _, rows, _) = editor_geo(w);
    let (r, c) = (w.crow, w.ccol);
    let len = w.lines[r].chars().count();
    if matches!(code, 102..=109) {
        w.hist.break_group(); // kursor harakati — söz guruhi yopiladi
    }
    match code {
        28 => {
            editor_record(w, EditKind::Enter);
            let b = byte_at(&w.lines[r], c);
            let rest = w.lines[r].split_off(b);
            w.lines.insert(r + 1, rest);
            w.crow += 1;
            w.ccol = 0;
            w.dirty = true;
        }
        14 => {
            if c > 0 || r > 0 {
                editor_record(w, EditKind::Delete);
            }
            if c > 0 {
                let b = byte_at(&w.lines[r], c - 1);
                w.lines[r].remove(b);
                w.ccol -= 1;
                w.dirty = true;
            } else if r > 0 {
                let l = w.lines.remove(r);
                w.crow = r - 1;
                w.ccol = w.lines[r - 1].chars().count();
                w.lines[r - 1].push_str(&l);
                w.dirty = true;
            }
        }
        111 => {
            if c < len || r + 1 < w.lines.len() {
                editor_record(w, EditKind::Delete);
            }
            if c < len {
                let b = byte_at(&w.lines[r], c);
                w.lines[r].remove(b);
                w.dirty = true;
            } else if r + 1 < w.lines.len() {
                let n = w.lines.remove(r + 1);
                w.lines[r].push_str(&n);
                w.dirty = true;
            }
        }
        105 => {
            if c > 0 {
                w.ccol -= 1;
            } else if r > 0 {
                w.crow -= 1;
                w.ccol = usize::MAX;
            }
        }
        106 => {
            if c < len {
                w.ccol += 1;
            } else if r + 1 < w.lines.len() {
                w.crow += 1;
                w.ccol = 0;
            }
        }
        103 => w.crow = r.saturating_sub(1),
        108 => w.crow = r + 1,
        102 => w.ccol = 0,
        107 => w.ccol = usize::MAX,
        104 => w.crow = r.saturating_sub(rows),
        109 => w.crow = r + rows,
        15 => editor_insert(w, "    "),
        _ => {
            if let Some(ch) = keycode_char(code, shift) {
                let mut b = [0u8; 4];
                let s = String::from(&*ch.encode_utf8(&mut b));
                editor_insert(w, &s);
            }
        }
    }
    editor_fix(w);
}

fn editor_click(w: &mut Window, cx: usize, cy: usize) -> bool {
    let (ty0, tx, lh, rows, _) = editor_geo(w);
    if cy < ty0 || cy >= ty0 + rows * lh || cx < w.x || cx >= w.x + w.w {
        return false;
    }
    w.crow = w.scroll + (cy - ty0) / lh;
    w.hist.break_group();
    let adv = gfx::mono_advance(small()).max(1);
    w.ccol = (cx.saturating_sub(tx) + adv / 2) / adv;
    editor_fix(w);
    true
}

fn draw_editor(win: &Window) {
    let px = small();
    let u = body();
    let (bx, by, bw, bh) = win.save_btn();
    let fname = win.path.rsplit('/').next().unwrap_or("");
    let status = String::from_utf8_lossy(&win.cur);
    let name = if win.dirty { alloc::format!("{} *", fname) } else { String::from(fname) };
    let left = if status.is_empty() { name } else { alloc::format!("{}  ·  {}", name, status) };
    let matches = win.find.as_ref().map(|q| crate::edit::find_in_lines(&win.lines, q)).unwrap_or_default();
    let mtxt = match win.find.as_ref() {
        Some(q) if !q.is_empty() => {
            let k = if matches.is_empty() { 0 } else { win.find_at.min(matches.len() - 1) + 1 };
            alloc::format!("{}/{} moslik  ·  ", k, matches.len())
        }
        _ => String::new(),
    };
    let pos = alloc::format!("{}Qator {}, ustun {}", mtxt, win.crow + 1, win.ccol + 1);
    let pw = gfx::text_width(&pos, small() * 4 / 5);
    let px_pos = bx.saturating_sub(pw + u);
    gfx::draw_text(win.x + u, by + (bh.saturating_sub(small())) / 2, &fit_text(&left, px_pos.saturating_sub(win.x + u * 2), small()), 0x9fb0c3, small());
    gfx::draw_text(px_pos, by + (bh.saturating_sub(small() * 4 / 5)) / 2, &pos, 0x6f7f95, small() * 4 / 5);
    gfx::fill_round_rect(bx, by, bw, bh, bh / 2, if win.dirty { 0x30c85a } else { 0x2a6b3d });
    gfx::draw_text_centered(bx + bw / 2, by + (bh.saturating_sub(small())) / 2, "Saqla", 0x06230f, small());
    let top = by + bh + u / 3;
    fill_rect(win.x + 1, top - 1, win.w.saturating_sub(2), 1, 0x2a3346);
    let (ty0, tx, lh, rows, gut) = editor_geo(win);
    let bottom = (win.y + win.h).saturating_sub(radius());
    fill_rect(win.x + 1, top, gut, bottom.saturating_sub(top), 0x161b26);
    let digits = if win.lines.len() > 999 { 4 } else { 3 };
    for k in 0..rows {
        let li = win.scroll + k;
        let Some(line) = win.lines.get(li) else { break };
        let ty = ty0 + k * lh;
        let cur = li == win.crow;
        if cur {
            gfx::blend_rect(win.x + gut, ty.saturating_sub(1), win.w.saturating_sub(gut + 2), lh, accent(), 28);
        }
        let num = alloc::format!("{:>w$}", li + 1, w = digits);
        gfx::draw_text_mono(win.x + u / 4, ty, &num, if cur { 0x9fb0c3 } else { 0x55627a }, px);
        gfx::draw_text_mono(tx, ty, line, 0xe7edf6, px);
        // Qidiruv mosliklari — sariq fon (joriysi yorqinroq)
        if let Some(q) = win.find.as_ref() {
            let qw = gfx::text_width_mono(q, px);
            for (mi, &(mr, mc)) in matches.iter().enumerate() {
                if mr != li {
                    continue;
                }
                let pre: String = line.chars().take(mc).collect();
                let mx = tx + gfx::text_width_mono(&pre, px);
                let right = (win.x + win.w).saturating_sub(2);
                if mx >= right {
                    continue;
                }
                let col = if mi == win.find_at { 0xffd426 } else { 0xb8961c };
                fill_rect(mx, ty.saturating_sub(1), qw.min(right - mx), lh, col);
                gfx::draw_text_mono(mx, ty, q, 0x1b1400, px);
            }
        }
        if cur {
            let pre: String = line.chars().take(win.ccol).collect();
            let cw = gfx::text_width_mono(&pre, px);
            fill_rect(tx + cw, ty, (px / 10).max(2), px + 2, 0x7fb0ea);
        }
    }
    // Qidiruv paneli (pastki qator)
    if let Some(q) = win.find.as_ref() {
        let fh = find_bar_h();
        let fy = bottom.saturating_sub(fh);
        fill_rect(win.x + 1, fy, win.w.saturating_sub(2), fh, 0x161b26);
        fill_rect(win.x + 1, fy, win.w.saturating_sub(2), 1, 0x2a3346);
        let lbl = "Qidir:";
        let lx = win.x + u;
        let ly = fy + (fh.saturating_sub(small())) / 2;
        gfx::draw_text(lx, ly, lbl, 0x9fb0c3, small());
        let ix = lx + gfx::text_width(lbl, small()) + u / 2;
        let info = if q.is_empty() {
            String::from("Enter — keyingisi · Esc — yopiş")
        } else if matches.is_empty() {
            String::from("Topilmadi")
        } else {
            alloc::format!("{}/{} moslik", win.find_at.min(matches.len() - 1) + 1, matches.len())
        };
        let iw = gfx::text_width(&info, small() * 4 / 5);
        let ir = (win.x + win.w).saturating_sub(u);
        let fw = ir.saturating_sub(iw + u + ix);
        let ih = fh * 3 / 4;
        let iy = fy + (fh - ih) / 2;
        gfx::fill_round_rect(ix, iy, fw, ih, ih / 3, 0x0f131c);
        let shown = fit_text(q, fw.saturating_sub(u), small());
        gfx::draw_text_mono(ix + u / 3, iy + (ih.saturating_sub(px)) / 2, &shown, 0xe7edf6, px);
        let cw = gfx::text_width_mono(&shown, px);
        fill_rect(ix + u / 3 + cw, iy + (ih.saturating_sub(px + 2)) / 2, (px / 10).max(2), px + 2, 0xffd426);
        let icol = if !q.is_empty() && matches.is_empty() { 0xff6b5e } else { 0x6f7f95 };
        gfx::draw_text(ir.saturating_sub(iw), fy + (fh.saturating_sub(small() * 4 / 5)) / 2, &info, icol, small() * 4 / 5);
    }
}

// ---------------- Oʻzbekça harflar: AltGr (öng Alt) ----------------

static ALTGR: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// AltGr + s/c/o/g/' → ş ç ö ğ ʼ (Shift bilan — bosh harf).
fn altgr_char(code: u16, shift: bool) -> Option<char> {
    if !ALTGR.load(Ordering::Relaxed) {
        return None;
    }
    let c = match code {
        31 => 'ş',
        46 => 'ç',
        24 => 'ö',
        34 => 'ğ',
        40 => return Some('ʼ'),
        _ => return None,
    };
    Some(if shift { c.to_uppercase().next().unwrap_or(c) } else { c })
}

// ---------------- Oyna yopiştiriş (snap) ----------------

/// Siçqon ekran çetida bölsa — oyna yopişadigan maydon.
fn snap_zone(cx: usize, cy: usize) -> Option<(usize, usize, usize, usize)> {
    let (sw, _) = gfx::size();
    let (zx, zy, zw, zh) = zoom_rect();
    let m = body() / 3;
    if cy <= menubar_h() + 1 {
        Some((zx, zy, zw, zh))
    } else if cx <= 2 {
        Some((zx, zy, (zw - m) / 2, zh))
    } else if cx + 3 >= sw {
        let hw = (zw - m) / 2;
        Some((zx + zw - hw, zy, hw, zh))
    } else {
        None
    }
}

static TITLE_CLICK: crate::sync::SpinLock<(u64, usize)> = crate::sync::SpinLock::new((0, 0));

// ---------------- Fayllarni sudrab tashlaş (drag & drop) ----------------

/// Fayllar oynasida bosilgan element: (yöl, nom, tur, x, y) — siljisa sudraş boşlanadi.
static FDRAG_PENDING: crate::sync::SpinLock<Option<(String, String, FKind, usize, usize)>> = crate::sync::SpinLock::new(None);

struct FDrag {
    path: String,
    name: String,
    kind: FKind,
}

/// Tashlaş nişoni: (papka yöli yoki "" — savat, yoritiladigan töğriburçak).
fn drop_target(windows: &[Window], desk: &Desk, cx: usize, cy: usize) -> Option<(String, (usize, usize, usize, usize))> {
    let (sw, sh) = gfx::size();
    if let Some(ti) = LAUNCHERS.iter().position(|l| l.kind == Kind::Trash) {
        let (ix, iy, isz) = dock_icon_rect(sw, sh, ti);
        if in_rect((ix, iy, isz, isz), cx, cy) {
            return Some((String::new(), (ix, iy, isz, isz)));
        }
    }
    if let Some(i) = windows.iter().rposition(|w| !w.hidden && w.contains(cx, cy)) {
        let w = &windows[i];
        match w.kind {
            Kind::Trash => return Some((String::new(), (w.x, w.y, w.w, w.h))),
            Kind::Files => {
                let g = files_geo(w);
                if let Some(r) = file_row_at(w, cx, cy) {
                    if w.rows[r].1 == FKind::Dir {
                        let (row, col) = (r / g.cols, r % g.cols);
                        return Some((path_join(&w.path, &w.rows[r].0), (g.gx + col * g.cw, g.gy + row * g.ch, g.cw, g.ch)));
                    }
                }
                if let Some(p) = files_side_at(w, cx, cy) {
                    let k = (cy - g.side_y) / g.item_h;
                    return Some((p, (w.x + body() / 3, g.side_y + k * g.item_h, g.sbw - body() * 2 / 3, g.item_h)));
                }
                if cx >= g.gx && cy >= g.gy {
                    return Some((w.path.clone(), (g.tx, g.top + g.th, w.x + w.w - g.tx, (w.y + w.h).saturating_sub(g.top + g.th))));
                }
            }
            _ => {}
        }
        return None;
    }
    (0..desk.icons.len()).find_map(|k| {
        let r = desk_icon_rect(k);
        in_rect(r, cx, cy).then(|| (alloc::format!("~/{}", desk.icons[k]), r))
    })
}

fn draw_fdrag(windows: &[Window], desk: &Desk, d: &FDrag, cx: usize, cy: usize) {
    let u = body();
    if let Some((_, (x, y, w, h))) = drop_target(windows, desk, cx, cy) {
        fill_round_rect_a(x, y, w, h, u / 2, accent(), 60);
        gfx::round_outline(x, y, w, h, u / 2, 2, accent());
    }
    let s = u * 2;
    big_icon(d.kind, &d.name, cx + u / 2, cy + u / 2, s);
    let lbl = fit_text(&d.name, u * 8, small() * 9 / 10);
    let lw = gfx::text_width(&lbl, small() * 9 / 10) + u;
    fill_round_rect_a(cx + u / 2, cy + u / 2 + s + 2, lw, small() + u / 3, u / 3, accent(), 220);
    gfx::draw_text(cx + u, cy + u / 2 + s + 2 + u / 6, &lbl, OQ, small() * 9 / 10);
}

/// Sudralgan faylni tashlaydi: papkaga köçiradi (Shift — nusxalaydi) yoki savatga.
fn drop_file(windows: &mut Vec<Window>, desk: &mut Desk, d: FDrag, cx: usize, cy: usize, copy: bool) {
    let Some((dir, _)) = drop_target(windows, desk, cx, cy) else { return };
    if dir.is_empty() {
        match to_trash(&d.path) {
            Ok(n) => notify("Savatga tashlandi", &n, 0xff9f0a),
            Err(e) => notify("Savatga tashlab bölmadi", &e, 0xff453a),
        }
    } else {
        let src_dir = d.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        if dir == d.path || (src_dir == dir && !copy) {
            return;
        }
        if dir.starts_with(&alloc::format!("{}/", d.path)) {
            notify("Köçirib bölmadi", "Papkani öz içiga köçirib bölmaydi", 0xff453a);
            return;
        }
        let dst = unique_dest(&path_join(&dir, &d.name));
        let r = if copy { copy_path(&d.path, &dst, 0) } else { fs::rename(&d.path, &dst).map_err(|e| alloc::format!("{:?}", e)) };
        let to = dir.rsplit('/').next().unwrap_or("");
        let to = if dir == "~" { "Uy" } else { to };
        match r {
            Ok(()) => {
                let _ = fs::commit();
                notify(if copy { "Nusxalandi" } else { "Köçirildi" }, &alloc::format!("{} — {} papkasiga", d.name, to), 0x2fbf5f);
            }
            Err(e) => notify("Köçirib bölmadi", &alloc::format!("{}: {}", d.name, e), 0xff453a),
        }
    }
    reload_all_files(windows);
    desk.icons = home_icons();
}

// ---------------- Ekran rasmi (PNG) ----------------

static SHOT_REQ: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Ekranni (kursorsiz) PNG qilib ~/Rasmlar ga saqlaydi.
fn take_screenshot(windows: &[Window], wp: &Option<crate::image::Image>, desk: &Desk) {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    let (sw, sh) = gfx::size();
    draw(windows, sw + 64, sh + 64, wp, desk);
    let px = gfx::read_pixels();
    let t0 = crate::timer::uptime_ms();
    let png = crate::image::png_enc::encode(sw, sh, &px);
    let ms = crate::timer::uptime_ms().saturating_sub(t0);
    let t = time::now();
    let name = alloc::format!("ekran-{:04}{:02}{:02}-{:02}{:02}{:02}.png", t.year, t.month, t.day, t.hour, t.minute, t.second);
    let dir = if fs::list("~/Rasmlar").is_ok() { "~/Rasmlar" } else { "~" };
    let path = unique_dest(&path_join(dir, &name));
    let r = fs::open(&path, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE).and_then(|id| fs::write(id, Some(0), &png));
    match r {
        Ok(_) => {
            let _ = fs::commit();
            notify(
                "Ekran rasmi saqlandi",
                &alloc::format!("{} · {} KB · {} ms", path.rsplit('/').next().unwrap_or(""), png.len() / 1024, ms),
                0x2fbf5f,
            );
        }
        Err(e) => notify("Ekran rasmini saqlab bölmadi", &alloc::format!("{:?}", e), 0xff453a),
    }
}

// ---------------- Boşqotirma (15 ta raqamli plitka) ----------------

fn puzzle_new(win: &mut Window) {
    win.tiles = (1..16).chain(core::iter::once(0)).collect();
    let mut blank = 15usize;
    let mut last = usize::MAX;
    for _ in 0..400 {
        let (r, c) = (blank / 4, blank % 4);
        let mut nb: Vec<usize> = Vec::new();
        if r > 0 { nb.push(blank - 4); }
        if r < 3 { nb.push(blank + 4); }
        if c > 0 { nb.push(blank - 1); }
        if c < 3 { nb.push(blank + 1); }
        nb.retain(|&n| n != last);
        let n = nb[(crate::random::u64() % nb.len() as u64) as usize];
        win.tiles.swap(blank, n);
        last = blank;
        blank = n;
    }
    win.play_pos = 0;
    win.play_t = crate::timer::uptime_ms();
    win.tab = 0;
}

fn puzzle_solved(t: &[u8]) -> bool {
    t.iter().enumerate().all(|(i, &v)| if i == 15 { v == 0 } else { v as usize == i + 1 })
}

/// Taxta geometriyasi: (x, y, ölçam, katak) va "Yangi öyin" tugmasi.
fn puzzle_geo(win: &Window) -> ((usize, usize, usize, usize), (usize, usize, usize, usize)) {
    let u = body();
    let head = u * 5 / 2;
    let top = win.y + title_h() + head;
    let avail_w = win.w.saturating_sub(u * 2);
    let avail_h = (win.y + win.h).saturating_sub(top + u);
    let bs = avail_w.min(avail_h);
    let bx = win.x + (win.w - bs) / 2;
    let bw = gfx::text_width("Yangi öyin", small()) + u * 2;
    let bh = u * 3 / 2;
    let btn = ((win.x + win.w).saturating_sub(u + bw), win.y + title_h() + (head - bh) / 2, bw, bh);
    ((bx, top, bs, bs / 4), btn)
}

/// Plitkani (yoki qator/ustundagi bir nechtasini) böş joyga suradi.
fn puzzle_move(win: &mut Window, idx: usize) -> bool {
    let Some(blank) = win.tiles.iter().position(|&v| v == 0) else { return false };
    let (br, bc, r, c) = (blank / 4, blank % 4, idx / 4, idx % 4);
    if idx == blank || (br != r && bc != c) {
        return false;
    }
    let step: isize = if br == r { if c > bc { 1 } else { -1 } } else if r > br { 4 } else { -4 };
    let mut b = blank as isize;
    while b != idx as isize {
        let n = b + step;
        win.tiles.swap(b as usize, n as usize);
        b = n;
    }
    win.play_pos += 1;
    if puzzle_solved(&win.tiles) {
        win.tab = 1;
        win.frame_ms = crate::timer::uptime_ms();
        let s = win.frame_ms.saturating_sub(win.play_t) / 1000;
        notify("Barakalla!", &alloc::format!("{} ta yuriş · {}:{:02}", win.play_pos, s / 60, s % 60), 0x2fbf5f);
    }
    true
}

fn puzzle_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    let ((bx, by, bs, cs), btn) = puzzle_geo(win);
    if in_rect(btn, cx, cy) {
        puzzle_new(win);
        return true;
    }
    if win.tab == 1 || !in_rect((bx, by, bs, bs), cx, cy) || cs == 0 {
        return in_rect((bx, by, bs, bs), cx, cy);
    }
    let idx = ((cy - by) / cs).min(3) * 4 + ((cx - bx) / cs).min(3);
    puzzle_move(win, idx);
    true
}

fn puzzle_key(win: &mut Window, code: u16) {
    if win.tab == 1 {
        return;
    }
    let Some(blank) = win.tiles.iter().position(|&v| v == 0) else { return };
    let (r, c) = (blank / 4, blank % 4);
    // Strelka plitkani shu tomonga suradi (böş joy teskari tomonga ketadi)
    let idx = match code {
        105 if c < 3 => blank + 1,
        106 if c > 0 => blank - 1,
        103 if r < 3 => blank + 4,
        108 if r > 0 => blank - 4,
        _ => return,
    };
    puzzle_move(win, idx);
}

fn draw_puzzle(win: &Window) {
    let u = body();
    let ((bx, by, bs, cs), btn) = puzzle_geo(win);
    let hy = win.y + title_h();
    let head = u * 5 / 2;
    let end = if win.tab == 1 { win.frame_ms } else { crate::timer::uptime_ms() };
    let s = end.saturating_sub(win.play_t) / 1000;
    let info = alloc::format!("Yurişlar: {}   Vaqt: {}:{:02}", win.play_pos, s / 60, s % 60);
    gfx::draw_text(win.x + u, hy + (head - small()) / 2, &info, 0xdbe3ee, small());
    gfx::fill_round_rect(btn.0, btn.1, btn.2, btn.3, btn.3 / 3, accent());
    gfx::draw_text_centered(btn.0 + btn.2 / 2, btn.1 + (btn.3 - small()) / 2, "Yangi öyin", OQ, small());
    gfx::fill_round_rect(bx, by, bs, bs, u / 2, 0x111622);
    let gap = (cs / 14).max(2);
    for (i, &v) in win.tiles.iter().enumerate() {
        if v == 0 {
            continue;
        }
        let (x, y) = (bx + (i % 4) * cs + gap, by + (i / 4) * cs + gap);
        let ts = cs - gap * 2;
        let ok = v as usize == i + 1;
        let base = if ok { mix(accent(), 0x34c759, 110) } else { accent() };
        gradient_round_rect(x, y, ts, ts, ts / 5, mix(base, 0xffffff, 50), mix(base, 0x000000, 40));
        let txt = alloc::format!("{}", v);
        let px = ts * 2 / 5;
        gfx::draw_text_centered(x + ts / 2, y + (ts - px) / 2 - px / 10, &txt, OQ, px);
    }
    if win.tab == 1 {
        fill_round_rect_a(bx, by, bs, bs, u / 2, 0x000000, 150);
        gfx::draw_text_centered(bx + bs / 2, by + bs / 2 - u * 2, "Barakalla!", 0x5cf08e, u * 2);
        gfx::draw_text_centered(bx + bs / 2, by + bs / 2 + u / 2, &alloc::format!("{} ta yurişda yeçildi", win.play_pos), OQ, small());
    }
}

// ---------------- Brauzer (HTTP/HTTPS + HTML + CSS + JS) ----------------

use crate::html::{Doc, Kind as HK, NO_EL};

struct Web {
    url: String,
    doc: Doc,
    scroll: usize,
    hist: Vec<String>,
    status: String,
    editing: bool,
    edit: String,
    loading: Option<u64>,         // kutilayotgan yuklaş vazifasi
    images: Vec<(String, crate::image::Image)>, // (src, rasm)
    js: Option<crate::js::Engine>,
    console: bool,                // F12 — JS konsoli
    t0: u64,
    focus: Option<usize>,         // fokusdagi forma maydoni
    fetches: Vec<(u64, u32)>,     // (yuklaş vazifasi, JS fetch raqami)
    select_open: Option<usize>,   // oçiq <select> ro'yxati
    refresh: Option<(u64, String)>, // <meta refresh>: (vaqt, manzil)
    anchor: Option<String>,       // yuklanganda aylantiriladigan #yakor
    scripts: Vec<String>,         // sahifa skriptlari (@media qayta tahlilida qayta bajariladi)
    media_key: (usize, usize),    // @media oxirgi baholangan sahifa ölçami (ekran px)
    hover_rules: bool,            // sahifada :hover qoidalari bormi
}

/// WItem.img: shu qiymatdan boşlab — <canvas> buferi (doc.canvases[i - CANVAS_IMG]).
const CANVAS_IMG: usize = 1 << 30;

struct WItem {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    text: String,
    col: Color,
    px: usize,
    mono: bool,
    link: Option<String>,
    el: u16,
    button: bool,
    underline: bool,
    img: Option<usize>,
    input: u16,
    fx: u16,   // html::FX_* effektlari
    bold: bool,
    det: u16,  // <summary> belgisi: bosilsa — <details> oçiladi/yopiladi
}

/// Fonda yuklaş navbati: (raqam, manzil, olindimi, natija).
struct WebJob {
    id: u64,
    url: String,
    taken: bool,
    result: Option<WebRes>,
    post: Option<String>,                  // forma tanasi (POST)
    raw: Option<(String, Option<String>)>, // JS fetch: (usul, tana)
}

struct WebRes {
    doc: Doc,
    url: String,
    status: String,
    images: Vec<(String, crate::image::Image)>,
    scripts: Vec<String>,
    raw: Option<Result<(u16, String, String), String>>, // JS fetch natijasi
}

/// Körilgan sahifalar tarixi: (manzil, sarlavha, vaqt).
static WEB_HIST: crate::sync::SpinLock<Vec<(String, String, String)>> = crate::sync::SpinLock::new(Vec::new());

/// zamin://tarix — tarix sahifasi (dinamik).
fn history_page() -> String {
    let h = WEB_HIST.lock();
    let mut s = String::from("<title>Tarix</title><style>.vaqt{color:#8e8e93;font-size:13px}.k{border:1px solid #3a4458;border-radius:8px;padding:10px}</style><h1>Tarix</h1>");
    if h.is_empty() {
        s.push_str("<p>Hali hech qanday sahifa oçilmagan.</p>");
    } else {
        s.push_str(&alloc::format!("<p>Oxirgi {} ta sahifa (yangisi tepada):</p><div class=\"k\"><ol>", h.len()));
        for (u, t, tm) in h.iter().rev() {
            let esc = |x: &str| x.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;");
            s.push_str(&alloc::format!("<li><a href=\"{}\">{}</a> <span class=\"vaqt\">{} · {}</span></li>", esc(u), esc(t), tm, esc(u)));
        }
        s.push_str("</ol></div>");
    }
    s.push_str("<p><a href=\"zamin://bosh\">Bosh sahifa</a></p>");
    s
}

static WEB_Q: crate::sync::SpinLock<Vec<WebJob>> = crate::sync::SpinLock::new(Vec::new());
static WEB_NEXT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
static WEB_WORKER: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn builtin_page(url: &str) -> Option<&'static str> {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    Some(match url.trim_end_matches('/') {
        "zamin://html" => r##"<!DOCTYPE html><html lang="uz"><head><meta charset="utf-8"><title>To‘liq HTML sinovi</title></head><body>
<h1 id="bosh">HTML elementlari</h1>
<p><a href="#jadval">→ Jadvalga ötiş</a> · <a href="#forma">→ Formaga</a> · <a href="#media">→ Mediaga</a></p>
<h2>Matn teglari</h2>
<p><b>qalin</b>, <strong>kuçli</strong>, <i>kursiv</i>, <em>urğu</em>, <u>tagi çizilgan</u>, <s>öçirilgan</s>, <del>del</del> → <ins>ins</ins>,
<mark>belgilangan</mark>, <small>kiçik</small>, <big>katta</big>, H<sub>2</sub>O, E = mc<sup>2</sup>, <kbd>Ctrl</kbd>+<kbd>C</kbd>,
<code>kod()</code>, <abbr title="HyperText Markup Language">HTML</abbr>, <q>iqtibos</q>, <cite>asar nomi</cite>, <var>x</var>, <samp>natija</samp>.</p>
<p>Belgilar: &copy; &reg; &trade; &euro; &pound; &yen; &sect; &para; &deg; &plusmn; &times; &divide; &frac12; &frac34; &ne; &le; &ge; &infin; &sum; &radic; &larr; &rarr; &uarr; &darr; &hArr; &hearts; &spades; &alpha;&beta;&gamma;&delta; &Omega; &laquo;Salom&raquo; &bdquo;matn&ldquo; &hellip; &#8364; &#x2713;</p>
<p>Lotin: Ñandú, Ölçam, Gödel, Straße, façade, Øre, Łódź · Kirill: Ўзбекистон, Қорақалпоқ, Ғалаба, Ҳаёт · Yunon: Ελληνικά</p>
<h3>Sarlavha 3</h3><h4>Sarlavha 4</h4><h5>Sarlavha 5</h5><h6>Sarlavha 6</h6>
<h2>Ro'yxatlar</h2>
<ol type="A"><li>Birinçi</li><li>Ikkinçi<ul><li>ichki</li><li>ichki<ul><li>yanada ichki</li></ul></li></ul></li><li>Uçinçi</li></ol>
<ol type="i" start="4"><li>tört</li><li>besh</li><li value="10">ön</li></ol>
<ol reversed><li>uç</li><li>ikki</li><li>bir</li></ol>
<dl><dt>HTML</dt><dd>Gipermatn belgilaş tili</dd><dt>CSS</dt><dd>Kaskadli uslublar jadvali</dd></dl>
<details><summary>Batafsil (bosing — oçiladi)</summary><p>Bu yaşirin matn <b>details/summary</b> bilan oçildi.</p></details>
<details open><summary>Oçiq details</summary><p>Boşidan oçiq.</p></details>
<figure><svg width="220" height="90" viewBox="0 0 220 90" xmlns="http://www.w3.org/2000/svg"><rect x="5" y="5" width="210" height="80" rx="14" fill="#1e88e5"/><circle cx="45" cy="45" r="26" fill="#ffd166"/><text x="85" y="55" font-size="26" fill="#ffffff">SVG</text></svg>
<figcaption>Ichki &lt;svg&gt; — öz SVG dekoderimiz bilan</figcaption></figure>
<p><img alt="data: URI rasm" width="48" height="48" src="data:image/svg+xml;utf8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'%3E%3Ccircle cx='5' cy='5' r='4' fill='%2334c759'/%3E%3C/svg%3E"> ← data: URI (SVG)</p>
<h2 id="jadval">Jadval</h2>
<table border="1">
<caption>Zamin OS komponentlari</caption>
<thead><tr><th>Qism</th><th>Til</th><th>Holat</th></tr></thead>
<tbody><tr><td>Yadro</td><td>Rust</td><td>tayyor</td></tr>
<tr><td>Brauzer</td><td>Rust</td><td>tayyor</td></tr>
<tr><td colspan="2">Ikki ustunli katak (colspan)</td><td>ok</td></tr></tbody>
</table>
<table border="1">
<caption>Haftalik jadval (rowspan)</caption>
<tr><th>Kun</th><th>Vaqt</th><th>Dars</th></tr>
<tr><td rowspan="2">Duşanba</td><td>09:00</td><td>Rust</td></tr>
<tr><td>11:00</td><td>Tarmoq</td></tr>
<tr><td>Seşanba</td><td rowspan="2">14:00</td><td>Grafika</td></tr>
<tr><td>Çorşanba</td><td>Fayl tizimi</td></tr>
</table>
<h2 id="forma">Forma maydonlari</h2>
<fieldset><legend>Anketa</legend>
<p><label for="ism">Ism:</label> <input id="ism" name="ism" placeholder="Ismingiz"> <input type="email" placeholder="nom@pochta.uz"></p>
<p>Jins: <label><input type="radio" name="j" value="e" checked> Erkak</label> <label><input type="radio" name="j" value="a"> Ayol</label></p>
<p>Rang: <input type="color" value="#ff9500"> Ovoz: <input type="range" min="0" max="10" value="7"> <input type="number" placeholder="yosh"></p>
<p>Yuklanish: <progress value="70" max="100"></progress> Daraja: <meter value="0.4"></meter></p>
<p><select><optgroup label="Osiyo"><option>Toshkent</option><option>Seul</option></optgroup><optgroup label="Yevropa"><option>Berlin</option></optgroup></select> <input type="file"></p>
</fieldset>
<h2 id="media">Media</h2>
<video src="/videolar/zamin.avi" width="320" height="120" poster="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='320' height='120'%3E%3Crect width='320' height='120' fill='%2313213a'/%3E%3Ccircle cx='160' cy='60' r='34' fill='%23ff9500'/%3E%3Cpath d='M148 42 L180 60 L148 78 Z' fill='%23ffffff'/%3E%3C/svg%3E" controls>Brauzer videoni qöllamaydi</video>
<audio controls><source src="/musiqa/kuy.mp3" type="audio/mpeg"><source src="zamin://namuna.wav" type="audio/wav">audio yöq</audio>
<iframe src="https://example.com/"></iframe>
<h2 id="iframe">Ichki hujjat (iframe)</h2>
<iframe width="80%" srcdoc="<style>body{background:#fff8e1;color:#4e342e} h3{color:#d84315}</style><h3>srcdoc hujjati</h3><p>Bu matn <b>alohida</b> hujjatda — öz CSS i bilan.</p><ul><li>bir</li><li>ikki</li></ul>"></iframe>
<iframe src="zamin://haqida" width="90%"></iframe>
<h2 id="canvas">Canvas 2D</h2>
<canvas id="kanvas" width="320" height="120">canvas yöq</canvas>
<script>
const kv = document.getElementById('kanvas');
const g = kv.getContext('2d');
g.fillStyle = '#13213a'; g.fillRect(0, 0, 320, 120);
g.strokeStyle = '#64d2ff'; g.lineWidth = 3; g.strokeRect(6, 6, 308, 108);
g.fillStyle = '#ff9500'; g.fillRect(20, 60, 30, 40); g.fillRect(60, 40, 30, 60); g.fillRect(100, 25, 30, 75);
g.beginPath(); g.moveTo(150, 95); g.lineTo(190, 50); g.lineTo(225, 70); g.lineTo(270, 25); g.strokeStyle = '#34c759'; g.lineWidth = 4; g.stroke();
g.fillStyle = 'rgba(255, 214, 38, 0.8)'; g.beginPath(); g.arc(285, 85, 18, 0, Math.PI * 2); g.fill();
g.fillStyle = '#ffffff'; g.font = '16px sans-serif'; g.fillText('Zamin canvas', 150, 112);
</script>
<hr><address>Muallif: Abbosjon Dev · zamin.app</address>
<p><a href="#bosh">↑ Yuqoriga</a></p>
</body></html>"##,
        "zamin://veb" => r##"<title>Töliqroq veb</title>
<style>
body { background: #0f1724; color: #dbe3ee; }
h1 { text-align: center; color: #ffffff; }
.qator { display: flex; gap: 12px; }
.karta { background: #13213a; border: 1px solid #3a7bd5; border-radius: 10px; padding: 10px; }
.karta > h3 { color: #64d2ff; }
.panjara { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }
.yacheyka { background: #1d2a40; border-radius: 8px; padding: 8px; text-align: center; }
table { border: 1px solid #3a4458; }
th { color: #ffd166; }
.yashirin { display: none; }
.faol { color: #2fbf5f; font-weight: bold; }
#natija { color: #ffd166; }
</style>
<h1>Flex, grid, jadval va zamonaviy JS</h1>
<div class="qator">
  <div class="karta"><h3>Flex 1</h3><p>display:flex — ustunlar yonma-yon.</p></div>
  <div class="karta"><h3>Flex 2</h3><p>gap: 12px, teng kenglik.</p></div>
  <div class="karta"><h3>Flex 3</h3><p id="sanoq">querySelectorAll…</p></div>
</div>
<h2>Grid (2 ustun)</h2>
<div class="panjara">
  <div class="yacheyka">A</div><div class="yacheyka">B</div><div class="yacheyka">C</div><div class="yacheyka">D</div>
</div>
<h2>Jadval</h2>
<table>
<tr><th>Til</th><th>Imkoniyat</th><th>Holat</th></tr>
<tr><td>HTML</td><td>formalar, jadval, select</td><td>tayyor</td></tr>
<tr><td>CSS</td><td>flex, grid, quti modeli</td><td>tayyor</td></tr>
<tr><td>JS</td><td>async/await, DOM, localStorage</td><td>tayyor</td></tr>
</table>
<h2>Tanlov va JS</h2>
<p>Shahar: <select id="shahar"><option value="tsh">Toshkent</option><option value="sam" selected>Samarqand</option><option value="bux">Buxoro</option></select>
<button id="kor">Körsat</button> <button id="almash">Yaşir / körsat</button></p>
<p id="tanlov" class="faol">—</p>
<p id="sir" class="yashirin">Bu matn classList.toggle bilan körinadi!</p>
<p><button id="kel">async/await fetch</button> <button id="qosh">Element qöşiş</button></p>
<p id="natija">—</p>
<ul id="royxat"><li>Boşlanğiç element</li></ul>
<p id="xotira">—</p>
<script>
document.getElementById("sanoq").textContent = "Kartalar soni: " + document.querySelectorAll(".karta").length + ", yacheykalar: " + document.querySelectorAll(".yacheyka").length;
document.getElementById("kor").onclick = () => {
  const s = document.getElementById("shahar");
  document.getElementById("tanlov").innerHTML = "Tanlandi: <b>" + s.value + "</b>";
};
document.getElementById("almash").addEventListener("click", () => {
  document.getElementById("sir").classList.toggle("yashirin");
});
async function yukla() {
  const r = await fetch("zamin://malumot.json");
  const d = await r.json();
  return d.tizim + " " + d.versiya + " — " + d.imkoniyatlar.length + " ta imkoniyat";
}
document.getElementById("kel").onclick = async () => {
  document.getElementById("natija").textContent = "Kutilmoqda…";
  const t = await yukla();
  document.getElementById("natija").textContent = "await natijasi: " + t;
};
let n = 0;
document.getElementById("qosh").onclick = () => {
  n++;
  const li = document.createElement("li");
  li.textContent = "Yangi element #" + n;
  document.getElementById("royxat").appendChild(li);
};
const k = Number(localStorage.getItem("tashrif") || "0") + 1;
localStorage.setItem("tashrif", String(k));
document.cookie = "til=uz; path=/";
document.getElementById("xotira").textContent = "localStorage: bu sahifa " + k + "-marta oçildi · cookie: " + document.cookie;
</script>"##,
        "zamin://malumot.json" => r##"{"tizim": "Zamin OS", "versiya": "0.0.27", "til": "Rust", "imkoniyatlar": ["JavaScript", "CSS", "fetch", "Promise", "formalar"], "yadrolar": 4, "ochiq": true}"##,
        "zamin://forma" => r##"<title>Formalar va fetch()</title>
<style>
body { background: #0f1724; color: #dbe3ee; }
h1 { color: #ffffff; text-align: center; }
.karta { max-width: 560px; margin: 14px auto; padding: 14px; border: 2px solid #3a7bd5; border-radius: 12px; background: #13213a; }
.yashil { border: 1px solid #2fbf5f; border-radius: 10px; padding: 10px; background: #0f2a1a; max-width: 560px; margin: 14px auto; }
.izoh { color: #8ea0b6; font-size: 13px; }
#javob { color: #ffd166; font-weight: bold; }
</style>
<h1>Formalar, fetch() va quti modeli</h1>
<div class="karta">
<h3>1. Ro'yxatdan ötiş (GET)</h3>
<form action="zamin://natija" method="get">
<p>Ism: <input name="ism" placeholder="Ismingiz" size="18"></p>
<p>Parol: <input type="password" name="parol" size="12"></p>
<p><input type="checkbox" name="obuna" checked> Yangiliklarga obuna</p>
<p><input type="submit" value="Yuboriş"></p>
</form>
<p class="izoh">Natija sahifasi qiymatlarni <code>location.search</code> va <code>URLSearchParams</code> bilan öqiydi.</p>
</div>
<div class="yashil">
<h3>2. fetch() + JSON.parse</h3>
<p><button id="olish">Maʼlumot olish</button></p>
<p id="javob">—</p>
</div>
<div class="karta">
<h3>3. JS submit hodisasi (preventDefault)</h3>
<form id="hisob">
<p><input id="a" name="a" value="12" size="6"> + <input id="b" name="b" value="30" size="6"> <button>Hisobla</button></p>
</form>
<p id="yigindi" class="izoh">Natija shu yerda.</p>
</div>
<script>
document.getElementById("olish").addEventListener("click", () => {
  document.getElementById("javob").textContent = "Yuklanmoqda…";
  fetch("zamin://malumot.json")
    .then(r => r.json())
    .then(d => {
      document.getElementById("javob").textContent =
        `${d.tizim} ${d.versiya} (${d.til}): ${d.imkoniyatlar.join(", ")} · yadrolar: ${d.yadrolar}`;
    })
    .catch(e => { document.getElementById("javob").textContent = "Xato: " + e; });
});
document.getElementById("hisob").addEventListener("submit", ev => {
  ev.preventDefault();
  const a = Number(document.getElementById("a").value);
  const b = Number(document.getElementById("b").value);
  document.getElementById("yigindi").textContent = `${a} + ${b} = ${a + b}`;
});
</script>"##,
        "zamin://natija" => r##"<title>Forma natijasi</title>
<style>.karta { max-width: 520px; margin: 20px auto; padding: 16px; border: 2px solid #2fbf5f; border-radius: 12px; background: #0f2a1a; }</style>
<div class="karta"><h2>Forma qabul qilindi</h2>
<p id="ism">…</p><p id="parol">…</p><p id="obuna">…</p>
<p>Sörov satri: <code id="qs">…</code></p></div>
<p><a href="zamin://forma">Formaga qaytiş</a></p>
<script>
const p = new URLSearchParams(location.search);
document.getElementById("ism").textContent = "Salom, " + (p.get("ism") || "mehmon") + "!";
document.getElementById("parol").textContent = "Parol uzunligi: " + (p.get("parol") || "").length + " belgi";
document.getElementById("obuna").textContent = p.has("obuna") ? "Obuna: ha" : "Obuna: yöq";
document.getElementById("qs").textContent = location.search;
</script>"##,
        "zamin://joylasuv" => r##"<title>CSS joylaşuv: position va float</title>
<style>
.karta{position:relative;background:#e8eefc;color:#1a2233;padding:12px;border:1px solid #9bb0e0;border-radius:8px}
.nishon{position:absolute;top:8px;right:8px;background:#d93025;color:#fff;padding:2px 8px;border-radius:10px}
.chap{float:left;width:140px;background:#fde7c4;color:#3a2a10;padding:8px;margin:0 8px 0 0}
.ong{float:right;width:120px;background:#d6f5dd;color:#103a1a;padding:8px}
.siljish{position:relative;left:24px;top:6px;color:#1e6fd9}
</style>
<h1>position va float</h1>
<div class="karta"><h3>Karta (position: relative)</h3><p>Öng yuqori burçakdagi nişon — position: absolute; top/right: 8px.</p><span class="nishon">Yangi</span></div>
<p class="siljish">Bu paragraf position: relative bilan 24px öngga va 6px pastga siljitilgan.</p>
<div class="chap"><b>float: left</b><p>Çap ustun</p></div>
<div class="ong"><b>float: right</b><p>Öng ustun</p></div>
<p>Bu matn ikki float element orasidan oqadi. Zamin Brauzer endi CSS position (relative, absolute, fixed) va float (left, right) xossalarini, shuningdek clear xossasini tuşunadi. Matn float lar yonida toraygan sohada joylaşadi.</p>
<p style="clear:both;background:#eee;color:#222;padding:6px">clear: both — bu blok float lar ostidan boşlanadi.</p>
"##,
        "zamin://matn" => r##"<title>Matn tekislaşi va dir=rtl</title>
<style>
.q{background:#eef2f8;color:#1a2233;padding:8px;border-radius:6px}
.j{text-align:justify}
.e{text-align:end}
</style>
<h1>text-align va dir</h1>
<p class="q" style="text-align:left">text-align: left — çapga tekislangan matn.</p>
<p class="q" style="text-align:center">text-align: center — markazda.</p>
<p class="q" style="text-align:right">text-align: right — öngga tekislangan matn.</p>
<p class="q j">text-align: justify — bu paragrafdagi sözlar orasidagi bo'şliqlar kengaytiriladi, şunda har bir qator (oxirgisidan taşqari) ikkala çetga ham teng taqaladi. Zamin Brauzer buni noldan hisoblaydi.</p>
<h2>dir="rtl"</h2>
<div dir="rtl">
<p class="q">dir="rtl" — matn öngdan boşlanadi (text-align: start).</p>
<p class="q e">text-align: end — rtl da çapga.</p>
<ul><li>Birinçi band</li><li>Ikkinçi band</li></ul>
<blockquote>Iqtibos çizig'i öng tomonda.</blockquote>
<div style="display:flex;gap:8px"><p class="q">1</p><p class="q">2</p><p class="q">3</p></div>
</div>
<h2>height, line-height va text-transform</h2>
<style>.m{margin:6px 0}</style>
<div style="min-height:90px;background:#dfe9fb;color:#1a2233;padding:6px;border-radius:6px">min-height: 90px — qisqa kontent, quti kamida 90px.</div>
<div style="min-height:60px"></div>
<p class="q">Yuqorida — böş div (min-height: 60px): bu paragraf pastga surildi.</p>
<div style="height:30px;background:#fde7c8;color:#1a2233">height: 30px — keyingi blok 30px dan keyin boşlanadi,<br>uzun kontent esa taşib çiqadi (bu qator qutidan taşqarida).</div>
<div style="height:24px"></div>
<p class="q m" style="line-height:1">line-height: 1 — zich qatorlar. Zamin Brauzer qator balandligini noldan hisoblaydi, şuning uçun bu paragraf ikki-uç qatorga bölinadi va qatorlar bir-biriga yaqin turadi.</p>
<p class="q m" style="line-height:2">line-height: 2 — keng qatorlar. Zamin Brauzer qator balandligini noldan hisoblaydi, şuning uçun bu paragraf ikki-uç qatorga bölinadi va qatorlar orasi ikki baravar.</p>
<p class="q m" style="text-transform:uppercase">uppercase: şahar öy, ğalla, çiroq, oʻzbek — шаҳар ўзбек</p>
<p class="q m" style="text-transform:lowercase">LOWERCASE: ŞAHAR ÖY, ĞALLA, ÇIROQ — ШАҲАР ЎЗБЕК</p>
<p class="q m" style="text-transform:capitalize">capitalize: oʻzbek tili, gʻaroyib çiroq, maʼno</p>
"##,
        "zamin://bezak" => r##"<title>CSS bezak: gradient va soya</title>
<style>
body{background:#f3f5f9;color:#1a2233}
.k{padding:14px;border-radius:14px;color:#ffffff;box-shadow:0 6px 16px rgba(0,0,0,.30)}
.v{background:linear-gradient(to bottom,#4f8cff,#1e3a8a)}
.g{background:linear-gradient(to right,#ff5f6d,#ffc371)}
.d{background-image:linear-gradient(45deg,#11998e,#38ef7d 60%,#d4fc79)}
.u{background:linear-gradient(90deg,#8e2de2,#4a00e0,#00c6ff);border-radius:4px;margin:20px 0}
.s{background:#ffffff;color:#1a2233;border-radius:10px;box-shadow:4px 4px 0 #1e3a8a}
.y{background:#ffffff;color:#1a2233;border-radius:18px;box-shadow:0 0 20px 2px rgba(79,140,255,.6)}
.x{background:linear-gradient(#ff0000;color:#1a2233;box-shadow:bad}
</style>
<h1>Gradient va box-shadow</h1>
<p>linear-gradient (to bottom / to right / burçak), border-radius va box-shadow — noldan çiziladi.</p>
<div style="display:flex;gap:14px">
<div class="k v"><b>to bottom</b><p>Vertikal gradient</p></div>
<div class="k g"><b>to right</b><p>Gorizontal gradient</p></div>
<div class="k d"><b>45deg</b><p>Uç töxtaş, diagonal</p></div>
</div>
<div class="k u"><b>90deg, 3 rang</b> — kenglik böyiça binafşadan havoranggaça.</div>
<h2>box-shadow</h2>
<div style="display:flex;gap:18px">
<div class="s"><b>Qattiq soya</b><p>4px 4px 0</p></div>
<div class="y"><b>Yorug'lik</b><p>0 0 20px 2px</p></div>
<div class="x"><b>Yaroqsiz CSS</b><p>e'tiborsiz qoldirildi</p></div>
</div>
"##,
        "zamin://bosh" => r##"<title>Bosh sahifa</title>
<style>
.hero{background:#1e3a8a;color:#ffffff;text-align:center}
.tip{color:#9fb0c3}
.ok{color:#34c759;font-weight:bold}
</style>
<div class="hero"><h1>Zamin Brauzer</h1><p>HTML · CSS · JavaScript · HTTP/HTTPS — hammasi noldan</p></div>
<p>Bu brauzer sahifalarni <b>fonda</b> yuklaydi (ekran qotmaydi), <b>CSS</b> uslublarini qöllaydi,
<b>JavaScript</b>ni öz interpretatorimizda bajaradi va <b>rasmlarni</b> körsatadi.</p>
<p id="vaqt" class="ok">Soat: …</p>
<p><button id="tugma">Bosing: 0</button></p>
<h2>Tez havolalar</h2>
<ul>
<li><a href="zamin://sinov">JavaScript va CSS sinovi</a></li>
<li><a href="~/Hujjatlar/zamin.html">Zamin OS qöllanmasi</a> — mahalliy HTML fayl</li>
<li><a href="zamin://forma">Formalar va fetch() sinovi</a></li>
<li><a href="zamin://veb">Flex, grid, jadval, async/await sinovi</a></li>
<li><a href="zamin://html">Barça HTML elementlari sinovi</a></li>
<li><a href="zamin://joylasuv">CSS position va float sinovi</a></li>
<li><a href="zamin://ramka">iframe: ichki hujjatlar sinovi</a></li>
<li><a href="zamin://matn">text-align, dir=rtl, height, line-height va text-transform sinovi</a></li>
<li><a href="zamin://bezak">CSS gradient va box-shadow sinovi</a></li>
<li><a href="zamin://tarix">Tarix</a> — körilgan sahifalar</li>
<li><a href="zamin://haqida">Brauzer haqida</a></li>
<li><a href="https://example.com/">example.com</a> (HTTPS, haqiqiy internet)</li>
</ul>
<p class="tip">Klaviatura: strelkalar, PgUp/PgDn, Space — aylantiriş; Backspace — orqaga; F5 — yangilaş; F12 — JS konsoli.</p>
<script>
let n = 0;
const t = document.getElementById("tugma");
t.addEventListener("click", function () {
  n++;
  t.textContent = "Bosing: " + n;
  if (n % 5 == 0) { t.style.color = "#ff9f0a"; }
});
function soat() {
  const d = new Date();
  const p = x => String(x).padStart(2, "0");
  document.getElementById("vaqt").textContent = `Soat: ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
soat();
setInterval(soat, 1000);
console.log("Bosh sahifa tayyor");
</script>"##,
        "zamin://sinov" => r##"<title>JS va CSS sinovi</title>
<style>
body{background:#f5f5f7;color:#1d1d1f}
h1{color:#0a58ca}
.karta{background:#ffffff}
.qizil{color:#d70015}
#natija{font-size:20px;font-weight:bold;color:#248a3d}
.markaz{text-align:center}
.yashirin{display:none}
</style>
<h1>JavaScript va CSS sinovi</h1>
<p class="karta">Bu sahifa <b>och fon</b> (CSS <code>body{background}</code>) bilan. <span class="qizil">Qizil matn</span> — klass orqali.</p>
<p class="yashirin">Bu xatboşi körinmasligi kerak (display:none).</p>
<p class="markaz">Markazlaşgan matn (text-align:center)</p>
<h2>Fibonaçi (JS)</h2>
<p id="fib">…</p>
<h2>Massiv amallari</h2>
<p id="arr">…</p>
<h2>Hisoblagiç</h2>
<p id="natija">0</p>
<p><button onclick="qosh(1)">+1</button> <button onclick="qosh(10)">+10</button> <button onclick="nol()">Nol</button></p>
<h2>document.write</h2>
<script>
function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
let s = [];
for (let i = 0; i < 15; i++) s.push(fib(i));
document.getElementById("fib").textContent = s.join(", ");
const a = [5, 3, 8, 1, 9, 2];
const sorted = [...a].sort((x, y) => x - y);
document.getElementById("arr").textContent =
  "Saralangan: " + a.sort((x, y) => x - y).join(" ") + " · yiğindi: " + a.reduce((p, x) => p + x, 0) +
  " · juftlari: " + a.filter(x => x % 2 == 0).join(",") + " · max: " + Math.max(...a);
let jami = 0;
function qosh(k) { jami += k; document.getElementById("natija").textContent = jami; }
function nol() { jami = 0; document.getElementById("natija").textContent = "0"; }
document.write("<p>Bu qator <b>document.write</b> bilan qöşildi. Sana: " + new Date().toLocaleDateString() + "</p>");
console.log("Sinov sahifasi:", s.length, "ta Fibonaçi soni");
</script>
<p><a href="zamin://bosh">Bosh sahifa</a></p>"##,
        "zamin://ramka" => r##"<title>iframe sinovi</title>
<h1>Ichki hujjatlar (iframe)</h1>
<p>Quyidagi ramkalar — alohida hujjatlar: öz CSS i, öz foni.</p>
<iframe width="80%" srcdoc="<style>body{background:#fff8e1;color:#4e342e} h3{color:#d84315}</style><h3>srcdoc hujjati</h3><p>Bu matn <b>alohida</b> hujjatda — öz CSS i bilan.</p><ul><li>bir</li><li>ikki</li></ul>"></iframe>
<p>Bir xil manbali <code>src</code> (zamin://haqida):</p>
<iframe src="zamin://haqida" width="90%"></iframe>
<p>Boşqa manbali — havola: <iframe src="https://example.com/"></iframe></p>
<p><a href="zamin://bosh">Bosh sahifa</a></p>"##,
        "zamin://haqida" => r##"<title>Brauzer haqida</title>
<h1>Brauzer haqida</h1>
<p>Hamma qatlam noldan yozilgan:</p>
<ol>
<li><b>Tarmoq</b>: virtio-net, ARP, IPv4, TCP, UDP, DHCP, DNS</li>
<li><b>Xavfsizlik</b>: TLS 1.3 — X25519, ChaCha20-Poly1305, SHA-256, X.509</li>
<li><b>HTTP/1.1</b>: yönaltiriş, chunked, Content-Length</li>
<li><b>HTML</b>: bloklar, ro'yxatlar, havolalar, rasmlar, tugmalar, belgilar</li>
<li><b>CSS</b>: teg/klass/id/avlod tanlagiçlari, rang, fon, shrift, tekislaş, display</li>
<li><b>JavaScript</b>: funksiyalar, yopilmalar, massiv/obyekt, shablonlar, DOM, taymerlar, hodisalar</li>
<li><b>Rasmlar</b>: PNG, JPEG, GIF, BMP, SVG va boşqalar — öz dekoderlarimiz</li>
</ol>
<blockquote>Yuklaş fonda — alohida yadro vazifasida; ekran qotmaydi.</blockquote>
<p><a href="zamin://bosh">Bosh sahifaga qaytiş</a></p>"##,
        _ => return None,
    })
}

fn error_doc(title: &str, msg: &str) -> Doc {
    let src = alloc::format!(
        "<title>{}</title><h1>{}</h1><p>{}</p><p><a href=\"zamin://bosh\">Bosh sahifa</a></p>",
        title,
        title,
        crate::html::decode_entities(msg).replace('<', "&lt;")
    );
    crate::html::parse(&src)
}

/// Resursni oladi: (baytlar, turi, yakuniy manzil).
fn web_bytes(url: &str, post: Option<&str>) -> Result<(Vec<u8>, String, String), String> {
    if url.trim_end_matches('/') == "zamin://tarix" {
        return Ok((history_page().into_bytes(), String::from("text/html"), String::from(url)));
    }
    if let Some(src) = builtin_page(url) {
        return Ok((Vec::from(src.as_bytes()), String::from("text/html"), String::from(url)));
    }
    if url == "zamin://namuna.wav" {
        return Ok((Vec::from(crate::audio::SAMPLE_WAV), String::from("audio/wav"), String::from(url)));
    }
    if !url.contains("://") {
        let p = url.strip_prefix("file:").unwrap_or(url);
        let d = fs::read_all(p).map_err(|e| alloc::format!("{}: {:?}", p, e))?;
        let low = p.to_ascii_lowercase();
        let ct = if low.ends_with(".html") || low.ends_with(".htm") {
            "text/html"
        } else if low.ends_with(".css") {
            "text/css"
        } else if low.ends_with(".js") {
            "text/javascript"
        } else if FKind::of(p.rsplit('/').next().unwrap_or(p), false) == FKind::Image {
            "image/*"
        } else {
            "text/plain"
        };
        return Ok((d, String::from(ct), String::from(p)));
    }
    let r = match post {
        Some(b) => crate::net::http::send("POST", url, Some(("application/x-www-form-urlencoded", b.as_bytes())))?,
        None => crate::net::http::get(url)?,
    };
    if r.status >= 400 {
        return Err(alloc::format!("HTTP {} — {}", r.status, r.url));
    }
    let ct = String::from(r.header("content-type").unwrap_or("text/html")).to_ascii_lowercase();
    Ok((r.body, ct, r.url))
}

/// Base64 (data: URI uçun).
fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut n) = (0u32, 0);
    for c in s.bytes() {
        if let Some(v) = val(c) {
            acc = (acc << 6) | v;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((acc >> n) as u8);
            }
        }
    }
    out
}

/// Katta rasmni kiçraytiradi (xotirani tejaş).
fn shrink(img: crate::image::Image, max: usize) -> crate::image::Image {
    if img.w <= max && img.h <= max { img } else { make_thumb(&img, max) }
}

/// Yuklaş vazifasi (fonda): sahifa, tashqi CSS, skriptlar, rasmlar.
/// JS fetch: (holat kodi, tana, yakuniy manzil) — xato kodlarida ham.
fn web_raw(method: &str, url: &str, body: Option<&str>) -> Result<(u16, String, String), String> {
    if !url.contains("://") || url.starts_with("zamin:") {
        let (d, _, u) = web_bytes(url, None)?;
        return Ok((200, String::from_utf8_lossy(&d).into_owned(), u));
    }
    let ct = match body {
        Some(b) if b.trim_start().starts_with('{') || b.trim_start().starts_with('[') => "application/json",
        _ => "text/plain;charset=UTF-8",
    };
    let r = crate::net::http::send(method, url, body.map(|b| (ct, b.as_bytes())))?;
    Ok((r.status, String::from_utf8_lossy(&r.body).into_owned(), r.url))
}

/// Manba (origin): "sxema://xost[:port]"; ichki (zamin:) va mahalliy fayllar — öz guruhi.
fn web_origin(url: &str) -> String {
    match url.split_once("://") {
        Some((sch, rest)) if !sch.eq_ignore_ascii_case("zamin") => {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            alloc::format!("{}://{}", sch.to_ascii_lowercase(), host.to_ascii_lowercase())
        }
        Some(_) => String::from("zamin:"),
        None => String::from("file:"),
    }
}

/// Bir xil manbali `<iframe src>` larni (4 tagaça, `depth` daraja ichma-ich) srcdoc ga aylantiradi.
/// Boşqa manbali iframe lar havola-kartoçka bölib qoladi.
fn inline_frames(text: &str, base: &str, depth: u8) -> String {
    if depth == 0 {
        return String::from(text);
    }
    let me = web_origin(base);
    crate::html::inline_iframes(text, 4, |src| {
        let full = crate::net::http::join(base, src);
        if web_origin(&full) != me || full.trim_end_matches('/') == base.trim_end_matches('/') {
            return None; // boşqa manba yoki özini özi
        }
        let (d, ct, fu) = web_bytes(&full, None).ok()?;
        if !ct.contains("html") || d.len() > 256 * 1024 || web_origin(&fu) != me {
            return None;
        }
        let inner = crate::html::decode_bytes(&d, &ct);
        Some(inline_frames(&inner, &fu, depth - 1))
    })
}

fn web_load(url: &str, post: Option<&str>) -> WebRes {
    let t0 = crate::timer::uptime_ms();
    let (bytes, ct, final_url) = match web_bytes(url, post) {
        Ok(x) => x,
        Err(e) => {
            return WebRes { doc: error_doc("Sahifani oçib bölmadi", &e), url: String::from(url), status: String::from("Xato"), images: Vec::new(), scripts: Vec::new(), raw: None };
        }
    };
    let name = final_url.rsplit('/').next().unwrap_or("fayl").split('?').next().unwrap_or("fayl");
    let mut images = Vec::new();
    let mut scripts = Vec::new();
    let doc = if ct.contains("html") {
        let text = crate::html::decode_bytes(&bytes, &ct);
        // Bir xil manbali <iframe src> — ichki hujjat yuklanib srcdoc sifatida qöşiladi
        let text = inline_frames(&text, &final_url, 2);
        // Tashqi CSS
        let mut css = String::new();
        for href in crate::html::stylesheet_links(&text).into_iter().take(4) {
            if let Ok((d, _, _)) = web_bytes(&crate::net::http::join(&final_url, &href), None) {
                if d.len() < 512 * 1024 {
                    css.push_str(&String::from_utf8_lossy(&d));
                    css.push('\n');
                }
            }
        }
        let doc = crate::html::parse_with(&text, &css);
        // Skriptlar (tartib saqlanadi)
        for s in doc.scripts.iter().take(12) {
            match s {
                crate::html::Script::Inline(c) => scripts.push(c.clone()),
                crate::html::Script::Src(u) => {
                    if let Ok((d, _, _)) = web_bytes(&crate::net::http::join(&final_url, u), None) {
                        if d.len() < 1024 * 1024 {
                            scripts.push(String::from_utf8_lossy(&d).into_owned());
                        }
                    }
                }
            }
        }
        // Rasmlar
        let mut seen: Vec<String> = Vec::new();
        for b in doc.blocks.iter().filter(|b| b.kind == HK::Img && !b.src.is_empty()) {
            if seen.contains(&b.src) || seen.len() >= 24 {
                continue;
            }
            seen.push(b.src.clone());
            // Ichki SVG va data: URI — tarmoqsiz
            if let Some(n) = b.src.strip_prefix("svg:").and_then(|n| n.parse::<usize>().ok()) {
                if let Some(img) = doc.svgs.get(n).and_then(|m| crate::image::decode_file("ichki.svg", m.as_bytes())) {
                    images.push((b.src.clone(), shrink(img, 1200)));
                }
                continue;
            }
            if let Some(rest) = b.src.strip_prefix("data:") {
                let (meta, data) = rest.split_once(',').unwrap_or((rest, ""));
                let bytes = if meta.ends_with(";base64") { base64_decode(data) } else { crate::js::url_decode(data).into_bytes() };
                let ext = if meta.contains("svg") { "a.svg" } else if meta.contains("png") { "a.png" } else if meta.contains("gif") { "a.gif" } else if meta.contains("jpeg") || meta.contains("jpg") { "a.jpg" } else { "a.bin" };
                if let Some(img) = crate::image::decode_file(ext, &bytes).or_else(|| crate::image::decode(&bytes)) {
                    images.push((b.src.clone(), shrink(img, 1200)));
                }
                continue;
            }
            let full = crate::net::http::join(&final_url, &b.src);
            if let Ok((d, _, fu)) = web_bytes(&full, None) {
                if d.len() < 4 * 1024 * 1024 {
                    let nm = fu.rsplit('/').next().unwrap_or("rasm").split('?').next().unwrap_or("rasm");
                    if let Some(img) = crate::image::decode_file(nm, &d).or_else(|| crate::image::decode(&d)) {
                        images.push((b.src.clone(), shrink(img, 1200)));
                    }
                }
            }
        }
        doc
    } else if ct.starts_with("image/") {
        match crate::image::decode_file(name, &bytes).or_else(|| crate::image::decode(&bytes)) {
            Some(img) => {
                let src = alloc::format!("<title>{}</title><img src=\"{}\">", name, final_url);
                images.push((final_url.clone(), shrink(img, 1600)));
                crate::html::parse(&src)
            }
            None => error_doc("Rasmni oçib bölmadi", name),
        }
    } else if ct.starts_with("text/") || ct.contains("json") || ct.contains("javascript") || ct.contains("xml") {
        crate::html::plain(&String::from_utf8_lossy(&bytes))
    } else {
        // Boşqa fayl — Yuklamalar papkasiga
        use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
        let dir = if fs::list("~/Yuklamalar").is_ok() { "~/Yuklamalar" } else { "~" };
        let path = unique_dest(&path_join(dir, if name.is_empty() { "yuklama" } else { name }));
        match fs::open(&path, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE).and_then(|id| fs::write(id, Some(0), &bytes)) {
            Ok(_) => {
                let _ = fs::commit();
                notify("Yuklab olindi", &alloc::format!("{} · {} KB", path, bytes.len() / 1024), 0x2fbf5f);
                crate::html::parse(&alloc::format!(
                    "<title>Yuklab olindi</title><h1>Fayl yuklab olindi</h1><p><b>{}</b></p><p>Turi: {} · {} KB</p><p>Saqlandi: <code>{}</code></p>",
                    name, ct, bytes.len() / 1024, path
                ))
            }
            Err(e) => error_doc("Saqlab bölmadi", &alloc::format!("{:?}", e)),
        }
    };
    let ms = crate::timer::uptime_ms().saturating_sub(t0);
    let status = if final_url.contains("://") && !final_url.starts_with("zamin:") {
        alloc::format!(
            "{} KB · {} ms{}{}",
            bytes.len() / 1024,
            ms,
            if final_url.starts_with("https") { " · TLS 1.3" } else { "" },
            if images.is_empty() { String::new() } else { alloc::format!(" · {} rasm", images.len()) }
        )
    } else if final_url.starts_with("zamin:") {
        String::from("Ichki sahifa")
    } else {
        alloc::format!("Fayl · {} bayt", bytes.len())
    };
    WebRes { doc, url: final_url, status, images, scripts, raw: None }
}

extern "C" fn web_worker(_: usize) {
    loop {
        let job = {
            let mut q = WEB_Q.lock();
            q.iter_mut().find(|j| !j.taken).map(|j| {
                j.taken = true;
                (j.id, j.url.clone(), j.post.clone(), j.raw.clone())
            })
        };
        match job {
            Some((id, url, post, raw)) => {
                let res = match raw {
                    Some((m, b)) => WebRes {
                        doc: Doc::empty(),
                        url: url.clone(),
                        status: String::new(),
                        images: Vec::new(),
                        scripts: Vec::new(),
                        raw: Some(web_raw(&m, &url, b.as_deref())),
                    },
                    None => web_load(&url, post.as_deref()),
                };
                let mut q = WEB_Q.lock();
                if let Some(j) = q.iter_mut().find(|j| j.id == id) {
                    j.result = Some(res);
                }
            }
            None => task::sleep_ms(25),
        }
    }
}

fn web_new() -> Box<Web> {
    Box::new(Web {
        url: String::new(),
        doc: Doc::empty(),
        scroll: 0,
        hist: Vec::new(),
        status: String::new(),
        editing: false,
        edit: String::new(),
        loading: None,
        images: Vec::new(),
        js: None,
        console: false,
        t0: 0,
        focus: None,
        fetches: Vec::new(),
        select_open: None,
        refresh: None,
        anchor: None,
        scripts: Vec::new(),
        media_key: (0, 0),
        hover_rules: false,
    })
}

/// Natijani oynaga qöllaydi va skriptlarni bajaradi.
fn web_apply(win: &mut Window, res: WebRes) {
    let w = win.web.get_or_insert_with(web_new);
    apply_web(w, res);
    web_title(win);
}

fn apply_web(w: &mut Web, res: WebRes) {
    apply_web_ex(w, res, false);
}

/// `quiet` — @media qayta tahlili: tarix va ogohlantiriş (alert) xabarlari takrorlanmaydi.
fn apply_web_ex(w: &mut Web, res: WebRes, quiet: bool) {
    w.doc = res.doc;
    w.media_key = (0, 0);
    w.hover_rules = w.doc.rules.iter().any(|r| crate::css::hover_kind(r) != (false, false));
    w.scripts = res.scripts.clone();
    w.url = res.url;
    w.images = res.images;
    w.scroll = 0;
    w.loading = None;
    w.focus = None;
    w.fetches.clear();
    w.select_open = None;
    w.anchor = w.url.split_once('#').map(|(_, f)| String::from(f)).filter(|f| !f.is_empty());
    w.refresh = w.doc.refresh.clone().map(|(secs, u)| {
        let target = if u.is_empty() { w.url.clone() } else { crate::net::http::join(&w.url, &u) };
        (crate::timer::uptime_ms() + secs as u64 * 1000, target)
    });
    let mut eng = crate::js::Engine::new();
    eng.url = w.url.clone();
    eng.fetch_hook = Some(web_raw); // await fetch(...) — joyida
    let now = crate::timer::uptime_ms();
    if !quiet && !w.url.starts_with("zamin://tarix") {
        let t = time::now();
        let title = if w.doc.title.is_empty() { w.url.clone() } else { w.doc.title.clone() };
        let mut h = WEB_HIST.lock();
        h.retain(|(u, _, _)| *u != w.url);
        h.push((w.url.clone(), title, alloc::format!("{:02}:{:02}", t.hour, t.minute)));
        if h.len() > 50 {
            h.remove(0);
        }
    }
    for s in &res.scripts {
        let _ = eng.run(s, &mut w.doc, now);
    }
    for a in eng.alerts.drain(..) {
        if !quiet {
            notify("Sahifa xabari", &a, 0x2f88ff);
        }
    }
    let js_info = if res.scripts.is_empty() {
        String::new()
    } else if eng.errors > 0 {
        alloc::format!(" · JS: {} xato (F12)", eng.errors)
    } else {
        alloc::format!(" · JS: {} skript", res.scripts.len())
    };
    w.status = alloc::format!("{}{}", res.status, js_info);
    w.js = Some(eng);
}

/// Sahifa ölçami özgargan bölsa: kö'riş maydoni (CSS px) yangilanadi, @media qoidalari holati
/// özgarsa — hujjat qayta tahlil qilinadi (skriptlar qayta bajariladi, aylantiriş saqlanadi).
fn web_media(win: &mut Window) -> bool {
    let (_, _, _, _, (_, _, pw, ph)) = web_geo(win);
    let Some(web) = win.web.as_mut() else { return false };
    let key = (pw, ph);
    if web.media_key == key {
        return false;
    }
    web.media_key = key;
    let k = (small() * 11 / 10) as f32 / 16.0;
    crate::css::set_viewport(pw.saturating_sub(body()) as f32 / k, ph as f32 / k);
    if web.doc.media_on.is_empty() || crate::css::media_state(&web.doc.rules) == web.doc.media_on {
        return false;
    }
    let doc = crate::html::parse_with(&web.doc.src, &web.doc.extra_css);
    let (scroll, status, refresh, focus) = (web.scroll, web.status.clone(), web.refresh.clone(), web.focus);
    let res = WebRes { doc, url: web.url.clone(), status: String::new(), images: core::mem::take(&mut web.images), scripts: core::mem::take(&mut web.scripts), raw: None };
    apply_web_ex(web, res, true);
    web.scroll = scroll;
    web.status = status;
    web.refresh = refresh;
    web.anchor = None;
    web.focus = focus.filter(|&f| f < web.doc.inputs.len());
    web.media_key = key;
    true
}

/// :hover — siçqonça ostidagi sahifa elementi (va ajdodlari) uçun uslub qayta qöllanadi.
fn web_hover(win: &mut Window, cx: usize, cy: usize) -> bool {
    let (_, _, _, _, (px0, py0, pw, ph)) = web_geo(win);
    let Some(web) = win.web.as_mut() else { return false };
    if !web.hover_rules {
        return false;
    }
    let inside = cx >= px0 && cx < px0 + pw && cy >= py0 && cy < py0 + ph && web.select_open.is_none();
    let el = if inside {
        let (items, _, total) = web_layout(web, pw.saturating_sub(body()));
        let sc = web.scroll.min(total.saturating_sub(ph));
        let (x, y) = (cx - px0, cy - py0 + sc);
        items.iter().rev().find(|i| i.el != NO_EL && x >= i.x && x < i.x + i.w && y >= i.y && y < i.y + i.h).map(|i| i.el).unwrap_or(NO_EL)
    } else {
        NO_EL
    };
    web.doc.set_hover(el)
}

fn web_title(win: &mut Window) {
    if let Some(w) = &win.web {
        let title = if w.doc.title.is_empty() { w.url.clone() } else { w.doc.title.clone() };
        win.title = alloc::format!("{} — Zamin Brauzer", title);
    }
}

// --- Tablar ---
fn tab_count(win: &Window) -> usize {
    win.web_tabs.len() + 1
}

/// k-tabga ötiş (faol tab boşqalari orasiga qaytadi).
fn tab_switch(win: &mut Window, k: usize) {
    if k == win.tab || k >= tab_count(win) {
        return;
    }
    let Some(cur) = win.web.take() else { return };
    let mut all = core::mem::take(&mut win.web_tabs);
    all.insert(win.tab.min(all.len()), cur);
    win.web = Some(all.remove(k));
    win.web_tabs = all;
    win.tab = k;
    web_title(win);
}

fn tab_new(win: &mut Window, url: &str) {
    if let Some(cur) = win.web.take() {
        let at = win.tab.min(win.web_tabs.len());
        win.web_tabs.insert(at, cur);
    }
    win.tab = win.web_tabs.len();
    web_go(win, url, false);
}

/// Faol tabni yopadi. Oxirgi tab bölsa — false (oyna yopilsin).
fn tab_close(win: &mut Window) -> bool {
    if win.web_tabs.is_empty() {
        return false;
    }
    let k = win.tab.min(win.web_tabs.len() - 1);
    win.web = Some(win.web_tabs.remove(k));
    win.tab = k;
    web_title(win);
    true
}

/// Tablar paneli: (y, balandlik, [(x, w)], "+" x).
fn tab_geo(win: &Window) -> (usize, usize, Vec<(usize, usize)>, usize) {
    let u = body();
    let y = win.y + title_h();
    let h = u * 3 / 2;
    let n = tab_count(win);
    let avail = win.w.saturating_sub(u * 3);
    let tw = (avail / n).min(u * 11).max(u * 3);
    let tabs = (0..n).map(|k| (win.x + u / 3 + k * tw, tw)).collect();
    (y, h, tabs, win.x + u / 3 + n * tw + u / 4)
}

fn draw_tabs(win: &Window) {
    let u = body();
    let (y, h, tabs, plus) = tab_geo(win);
    fill_rect(win.x + 1, y, win.w.saturating_sub(2), h, 0x1a2030);
    for (k, &(x, w)) in tabs.iter().enumerate() {
        let web = if k == win.tab { win.web.as_deref() } else { win.web_tabs.get(if k < win.tab { k } else { k - 1 }).map(|b| &**b) };
        let Some(web) = web else { continue };
        let active = k == win.tab;
        if active {
            gfx::fill_round_rect(x, y + 3, w - 2, h - 3, u / 3, WIN_BODY);
            fill_rect(x, y + h - u / 3, w - 2, u / 3, WIN_BODY);
        } else {
            fill_rect(x + w - 2, y + u / 3, 1, h - u * 2 / 3, 0x34405a);
        }
        let t = if web.doc.title.is_empty() { web.url.clone() } else { web.doc.title.clone() };
        let px = small() * 4 / 5;
        let lbl = fit_text(&t, w.saturating_sub(u * 2), px);
        let tx = x + u / 2;
        if web.loading.is_some() {
            gfx::fill_circle(tx + 3, y + h / 2 + 1, 3, accent());
        }
        gfx::draw_text(tx + if web.loading.is_some() { 9 } else { 0 }, y + (h - px) / 2 + 1, &lbl, if active { 0xeef3fa } else { 0x8696ac }, px);
        // yopiş belgisi
        let (cx, cy) = ((x + w - u * 3 / 4) as f32, (y + h / 2 + 1) as f32);
        let d = u as f32 / 5.0;
        seg(cx - d, cy - d, cx + d, cy + d, 1.1, 0x8696ac);
        seg(cx - d, cy + d, cx + d, cy - d, 1.1, 0x8696ac);
    }
    let (pc, pm) = ((plus + u / 2) as f32, (y + h / 2 + 1) as f32);
    let d = u as f32 / 3.5;
    seg(pc - d, pm, pc + d, pm, 1.3, 0xaebccf);
    seg(pc, pm - d, pc, pm + d, 1.3, 0xaebccf);
}

/// Tablar paneliga bosiş. Natija: Some(true) — işlandi, Some(false) — oxirgi tab yopildi (oynani yopiş).
fn tabs_click(win: &mut Window, cx: usize, cy: usize) -> Option<bool> {
    let u = body();
    let (y, h, tabs, plus) = tab_geo(win);
    if cy < y || cy >= y + h {
        return None;
    }
    if cx >= plus && cx < plus + u {
        tab_new(win, "zamin://bosh");
        return Some(true);
    }
    for (k, &(x, w)) in tabs.iter().enumerate() {
        if cx >= x && cx < x + w {
            if cx + u + u / 4 >= x + w {
                // × — yopiş
                tab_switch(win, k);
                return Some(tab_close(win));
            }
            tab_switch(win, k);
            return Some(true);
        }
    }
    Some(true)
}

fn web_go(win: &mut Window, url: &str, push: bool) {
    web_go_ex(win, url, push, None);
}

/// Yuklaş: `post` — forma tanasi (POST usuli).
fn web_go_ex(win: &mut Window, url: &str, push: bool, post: Option<String>) {
    let mut url = String::from(url.trim());
    if !url.contains("://") && !url.starts_with('~') && !url.starts_with('/') && !url.starts_with("file:") && !url.starts_with("zamin:") {
        url = alloc::format!("http://{}", url);
    }
    let w = win.web.get_or_insert_with(web_new);
    if push && !w.url.is_empty() && w.url != url {
        w.hist.push(w.url.clone());
    }
    w.editing = false;
    if post.is_none() && (builtin_page(&url).is_some() || url.starts_with("zamin:")) {
        let res = web_load(&url, None);
        web_apply(win, res);
        return;
    }
    // Fonda yuklaymiz — ekran qotmaydi
    if !WEB_WORKER.swap(true, Ordering::Relaxed) {
        let _ = task::spawn_kernel("brauzer", web_worker, 0);
    }
    let id = WEB_NEXT.fetch_add(1, Ordering::Relaxed);
    {
        let mut q = WEB_Q.lock();
        q.retain(|j| !(j.result.is_some() && j.id < id.saturating_sub(8)));
        q.push(WebJob { id, url: url.clone(), taken: false, result: None, post, raw: None });
    }
    w.loading = Some(id);
    w.t0 = crate::timer::uptime_ms();
    w.url = url.clone();
    w.status = alloc::format!("Yuklanmoqda… {}", url);
}

/// GUI sikli: tayyor yuklaşlar va JS taymerlari. Özgariş bölsa — true.
fn web_poll(windows: &mut [Window]) -> bool {
    let mut changed = false;
    let now = crate::timer::uptime_ms();
    for win in windows.iter_mut().filter(|w| w.kind == Kind::Browser) {
        // Nofaol tablarda tugagan yuklaşlar
        for t in win.web_tabs.iter_mut() {
            if let Some(id) = t.loading {
                let res = {
                    let mut q = WEB_Q.lock();
                    q.iter().position(|j| j.id == id && j.result.is_some()).map(|k| q.remove(k).result.unwrap())
                };
                if let Some(r) = res {
                    let mut tmp = core::mem::replace(t, web_new());
                    apply_web(&mut tmp, r);
                    *t = tmp;
                    changed = true;
                }
            }
        }
        // JS fetch(): yangi sörovlarni navbatga, tayyorlarini promise larga
        if let Some(w) = win.web.as_mut().filter(|w| w.loading.is_none()) {
            if let Some(eng) = w.js.as_mut() {
                for (fid, method, url, body) in core::mem::take(&mut eng.fetch_out) {
                    if !WEB_WORKER.swap(true, Ordering::Relaxed) {
                        let _ = task::spawn_kernel("brauzer", web_worker, 0);
                    }
                    let id = WEB_NEXT.fetch_add(1, Ordering::Relaxed);
                    let full = crate::net::http::join(&w.url, &url);
                    WEB_Q.lock().push(WebJob { id, url: full, taken: false, result: None, post: None, raw: Some((method, body)) });
                    w.fetches.push((id, fid));
                }
                let mut k = 0;
                while k < w.fetches.len() {
                    let (jid, fid) = w.fetches[k];
                    let done = {
                        let mut q = WEB_Q.lock();
                        q.iter().position(|j| j.id == jid && j.result.is_some()).map(|p| q.remove(p).result.unwrap())
                    };
                    match done {
                        Some(r) => {
                            w.fetches.remove(k);
                            eng.fetch_done(fid, r.raw.unwrap_or(Err(String::from("javob yöq"))), &mut w.doc);
                            for a in eng.alerts.drain(..) {
                                notify("Sahifa xabari", &a, 0x2f88ff);
                            }
                            changed = true;
                        }
                        None => k += 1,
                    }
                }
            }
        }
        // #yakor va <meta refresh>
        if let Some(f) = win.web.as_mut().and_then(|w| w.anchor.take()) {
            web_anchor(win, &f);
            changed = true;
        }
        if let Some((at, u)) = win.web.as_ref().and_then(|w| w.refresh.clone()) {
            if now >= at {
                if let Some(w) = win.web.as_mut() {
                    w.refresh = None;
                }
                web_go(win, &u, true);
                changed = true;
            }
        }
        let Some(id) = win.web.as_ref().and_then(|w| w.loading) else {
            if let Some(w) = win.web.as_mut() {
                if let Some(eng) = w.js.as_mut() {
                    if eng.tick(&mut w.doc, now) {
                        changed = true;
                        for a in eng.alerts.drain(..) {
                            notify("Sahifa xabari", &a, 0x2f88ff);
                        }
                    }
                }
            }
            continue;
        };
        let res = {
            let mut q = WEB_Q.lock();
            q.iter().position(|j| j.id == id && j.result.is_some()).map(|k| q.remove(k).result.unwrap())
        };
        match res {
            Some(r) => {
                web_apply(win, r);
                changed = true;
            }
            None => {
                // aylanuvçi körsatkiç uçun qayta chizamiz
                if now.wrapping_sub(win.play_t) >= 120 {
                    win.play_t = now;
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Brauzer geometriyasi: (asboblar y, balandligi, tugmalar x[3], manzil (x,y,w,h), sahifa (x,y,w,h)).
fn web_geo(win: &Window) -> (usize, usize, [usize; 3], (usize, usize, usize, usize), (usize, usize, usize, usize)) {
    let u = body();
    let top = win.y + title_h() + u * 3 / 2; // tablar paneli ostida
    let bh = u * 3 / 2;
    let by = top + u / 3;
    let b0 = win.x + u / 2;
    let step = bh + u / 4;
    let btns = [b0, b0 + step, b0 + step * 2];
    let ax = b0 + step * 3 + u / 4;
    let addr = (ax, by, (win.x + win.w).saturating_sub(ax + u / 2), bh);
    let ptop = by + bh + u / 3 + 1;
    let foot = small() + u / 2;
    let page = (win.x + 1, ptop, win.w.saturating_sub(2), (win.y + win.h).saturating_sub(ptop + foot));
    (by, bh, btns, addr, page)
}

fn luma(c: Color) -> u32 {
    (((c >> 16) & 0xff) * 3 + ((c >> 8) & 0xff) * 6 + (c & 0xff)) / 10
}

/// Sahifa ranglari: (fon, matn, havola, kod, sarlavha).
fn web_palette(doc: &Doc) -> (Color, Color, Color, Color, Color) {
    let bg = doc.bg.unwrap_or(0x161b26);
    let dark = luma(bg) < 128;
    let text = doc.color.unwrap_or(if dark { 0xd3dbe6 } else { 0x1d1d1f });
    if dark { (bg, text, 0x5aa9ff, 0xffc777, doc.color.unwrap_or(OQ)) } else { (bg, text, 0x0b57d0, 0xa3501a, doc.color.unwrap_or(0x000000)) }
}

/// Bezak: töğriburçak (to'ldirilgan va/yoki çegarali, yumaloq burçakli).
struct Deco {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    fill: Option<Color>,
    radius: usize,
    bw: usize,
    bc: Color,
    grad: Option<crate::css::Grad>,
    // box-shadow (ekran px): dx, dy, blur, spread, rang, alfa
    shadow: Option<(i32, i32, usize, i32, Color, u8)>,
}

fn dfill(x: usize, y: usize, w: usize, h: usize, c: Color) -> Deco {
    Deco { x, y, w, h, fill: Some(c), radius: 0, bw: 0, bc: 0, grad: None, shadow: None }
}

/// Sahifa joylaşuvi: elementlar, bezaklar va toʻliq balandlik.
/// Quti modeli: konteyner (fon / çegara / padding / kenglik) — qöşni bloklar bitta qutiga birlaşadi.
/// Flex / grid / jadval: konteyner bolalari yonma-yon ustunlar bölib joylaşadi (rekursiv).
fn web_layout(web: &Web, width: usize) -> (Vec<WItem>, Vec<Deco>, usize) {
    let u = body();
    let mut items: Vec<WItem> = Vec::new();
    let mut deco: Vec<Deco> = Vec::new();
    let vis: Vec<&crate::html::Block> = web
        .doc
        .blocks
        .iter()
        .filter(|b| !b.hidden && !(b.kind == HK::P && b.spans.iter().all(|s| s.text.is_empty() && s.input == NO_EL)))
        .collect();
    let mut anc = alloc::vec![(0u32, u, u / 2, width.saturating_sub(u * 2))];
    let y = lay(web, &vis, u, width.saturating_sub(u * 2), u / 2, 0, &mut items, &mut deco, &mut anc);
    // absolute elementlar ham sahifa balandligiga kiradi
    let y = items.iter().map(|i| i.y + i.h).fold(y, usize::max);
    (items, deco, y + u)
}

/// Sinov uçun: hujjatni `width` kenglikda joylaştiradi — (element, y, h) ro'yxati va umumiy balandlik.
/// Üçinçi qiymat — CSS piksel → ekran piksel kö'paytuvçisi.
pub fn layout_probe(doc: Doc, width: usize) -> (Vec<(u16, usize, usize, String)>, usize, f32) {
    let mut w = web_new();
    w.doc = doc;
    let (items, _, total) = web_layout(&w, width);
    let k = (small() * 11 / 10) as f32 / 16.0;
    (items.into_iter().map(|i| (i.el, i.y, i.h, i.text)).collect(), total, k)
}

/// position / float: oqimdan taşqari blokning (nusxa) quti maydonlarini tozalaydi — kenglikni lay() beradi.
fn out_of_flow(run: &[&crate::html::Block]) -> Vec<crate::html::Block> {
    run.iter()
        .map(|b| {
            let mut c = (*b).clone();
            if c.bgk != 0 && c.bgk < c.oser {
                // quti tasqaridagi ajdodniki — bu yerda çizilmaydi
                (c.bg, c.bgk, c.pad, c.bw, c.bc, c.radius) = (None, 0, 0.0, 0.0, None, 0.0);
                (c.grad, c.shadow) = (None, None);
            }
            if c.bgk == c.oser || c.bgk == 0 {
                (c.maxw, c.center) = (None, false);
            }
            c.oser = 0;
            c.okind = 0;
            c
        })
        .collect()
}

/// x=0 dan joylangan elementlarning tabiiy kengligi (shrink-to-fit): matn öng çeti + çap içki çekiniş (padding).
fn items_right(items: &[WItem]) -> usize {
    let r = items.iter().map(|i| i.x + i.w).max().unwrap_or(0);
    let l = items.iter().map(|i| i.x).min().unwrap_or(0);
    r + l
}

/// Bloklar ro'yxatini (x, kenglik) sohaga joylaydi; `skip` — joriy ustun içidagi flex (qayta guruhlamaslik uçun).
/// `anc` — joylaşgan (position) elementlar langarlari: (raqam, x, y, kenglik); [0] — sahifa.
#[allow(clippy::too_many_arguments)]
fn lay(web: &Web, vis: &[&crate::html::Block], page_pad0: usize, full0: usize, y0: usize, skip: u32, items: &mut Vec<WItem>, deco: &mut Vec<Deco>, anc: &mut Vec<(u32, usize, usize, usize)>) -> usize {
    let doc = &web.doc;
    let u = body();
    let k = (small() * 11 / 10) as f32 / 16.0;
    let pxs = |size: f32| ((size * k) as usize).clamp(8, 90);
    let lpx = |v: f32| (v * k) as usize;
    let (page_bg, text_col, link_col, code_col, head_col) = web_palette(doc);
    let dark = luma(page_bg) < 128;
    let mut y = y0;
    let mut open_box: Option<(u32, usize, usize)> = None;
    // rowspan: pastki qatorlarga çoziladigan katak qutilari (deco raqami, qolgan qatorlar)
    let mut rs_pend: Vec<(usize, u8)> = Vec::new();
    // float: (egallangan kenglik, pastki çeti) — çap va öng
    let mut lf = (0usize, 0usize);
    let mut rf = (0usize, 0usize);
    // position: relative — siljitiladigan diapazonlar (items boşi, deco boşi, dx, dy) va quti fonlari
    let spx = |v: f32| (v * k) as isize;
    let mut shifts: Vec<(usize, usize, usize, usize, isize, isize)> = Vec::new();
    let mut box_shifts: Vec<(usize, isize, isize)> = Vec::new();
    let mut pend: Option<(usize, usize, isize, isize)> = None;
    // height / min-height: joriy guruh (element seriyasi, kontent boşi y)
    let mut hgrp: Option<(u32, usize)> = None;
    let mut bi = 0;
    while bi < vis.len() {
        if let Some((i0, d0, dx, dy)) = pend.take() {
            shifts.push((i0, items.len(), d0, deco.len(), dx, dy));
        }
        let b = vis[bi];
        // clear: float lardan pastga
        if b.clear & 1 != 0 && lf.1 > y {
            y = lf.1;
        }
        if b.clear & 2 != 0 && rf.1 > y {
            y = rf.1;
        }
        if y >= lf.1 {
            lf.0 = 0;
        }
        if y >= rf.1 {
            rf.0 = 0;
        }
        // float bilan torayişgan soha
        let (page_pad, full) = if lf.0 + rf.0 + u * 4 < full0 { (page_pad0 + lf.0, full0 - lf.0 - rf.0) } else { (page_pad0, full0) };
        // --- position: absolute / fixed va float: oqimdan taşqari ---
        if b.oser != 0 && b.okind != 0 {
            let mut j = bi;
            while j < vis.len() && vis[j].oser == b.oser {
                j += 1;
            }
            let run = out_of_flow(&vis[bi..j]);
            let refs: Vec<&crate::html::Block> = run.iter().collect();
            let gap = u / 2;
            let ins = b.oins;
            if b.okind >= 3 {
                // langar: eng yaqin joylaşgan ajdod (yoki sahifa)
                let a = if b.okind == 4 { anc[0] } else { anc.iter().rev().find(|a| a.0 == b.pk && b.pk != 0).copied().unwrap_or(anc[0]) };
                let (l, r) = (ins[3].map(spx), ins[1].map(spx));
                let given = match b.ow {
                    Some((v, true)) => Some(a.3 * (v as usize).min(100) / 100),
                    Some((v, false)) => Some(lpx(v)),
                    None => None,
                };
                let mut w = match (given, l, r) {
                    (Some(w), _, _) => w,
                    (None, Some(l), Some(r)) => (a.3 as isize - l - r).max(u as isize * 2) as usize,
                    (None, Some(l), None) => (a.3 as isize - l).max(u as isize * 4) as usize,
                    (None, None, Some(r)) => (a.3 as isize - r).max(u as isize * 4) as usize,
                    _ => full,
                }
                .max(u * 2);
                if given.is_none() && !(l.is_some() && r.is_some()) {
                    // shrink-to-fit: avval ölçab olamiz
                    let (mut ti, mut td, mut ta): (Vec<WItem>, Vec<Deco>, _) = (Vec::new(), Vec::new(), anc.clone());
                    lay(web, &refs, 0, w, 0, skip, &mut ti, &mut td, &mut ta);
                    w = items_right(&ti).clamp(u, w);
                }
                let x = match (l, r) {
                    (Some(l), _) => (a.1 as isize + l).max(0) as usize,
                    (None, Some(r)) => (a.1 as isize + a.3 as isize - r - w as isize).max(0) as usize,
                    _ => page_pad,
                };
                let yy = match ins[0] {
                    Some(t) => (a.2 as isize + spx(t)).max(0) as usize,
                    None => y,
                };
                let (i0, d0) = (items.len(), deco.len());
                lay(web, &refs, x, w, yy, skip, items, deco, anc);
                if l.is_none() && r.is_some() {
                    // quti minimal kenglikdan kengroq çiqqan bölsa — öng çet joyida qolsin
                    let real = items[i0..].iter().map(|i| i.x + i.w).chain(deco[d0..].iter().map(|d| d.x + d.w)).max().unwrap_or(x + w);
                    let over = real.saturating_sub(x + w);
                    if over > 0 {
                        for it in items[i0..].iter_mut() {
                            it.x = it.x.saturating_sub(over);
                        }
                        for d in deco[d0..].iter_mut() {
                            d.x = d.x.saturating_sub(over);
                        }
                    }
                }
            } else {
                // float: çap yoki öng çetga, matn yonidan oqadi
                let mut w = match b.ow {
                    Some((v, true)) => full0 * (v as usize).min(100) / 100,
                    Some((v, false)) => lpx(v).min(full0),
                    None => {
                        let (mut ti, mut td, mut ta): (Vec<WItem>, Vec<Deco>, _) = (Vec::new(), Vec::new(), anc.clone());
                        lay(web, &refs, 0, full0 / 2, 0, skip, &mut ti, &mut td, &mut ta);
                        items_right(&ti).clamp(u * 2, full0 / 2)
                    }
                }
                .max(u * 2);
                if lf.0 + rf.0 + w > full0 && (lf.0 > 0 || rf.0 > 0) {
                    // sig'madi — float lar ostiga
                    y = y.max(if b.okind == 1 { lf.1 } else { rf.1 }).max(lf.1.min(rf.1));
                    if y >= lf.1 {
                        lf.0 = 0;
                    }
                    if y >= rf.1 {
                        rf.0 = 0;
                    }
                }
                w = w.min(full0.saturating_sub(lf.0 + rf.0)).max(u * 2);
                let x = if b.okind == 1 { page_pad0 + lf.0 } else { page_pad0 + full0 - rf.0 - w };
                let bottom = lay(web, &refs, x, w, y, skip, items, deco, anc);
                let f = if b.okind == 1 { &mut lf } else { &mut rf };
                if f.1 <= y {
                    f.1 = 0;
                }
                f.0 += w + gap;
                f.1 = f.1.max(bottom + u / 4);
            }
            open_box = None;
            bi = j;
            continue;
        }
        // langar (position: relative va boşqalar) — joylaşgan elementning boşi
        if b.aser != 0 && !anc.iter().any(|a| a.0 == b.aser) && (b.flex == 0 || b.flex == skip || b.aser <= b.flex) {
            anc.push((b.aser, page_pad, y, full));
        }
        // --- Flex / grid / jadval: ketma-ket bloklarni ustunlarga ajratamiz ---
        if b.flex != 0 && b.flex != skip && (b.fmode == 1 || b.fmode == 3) {
            let mut j = bi;
            while j < vis.len() && vis[j].flex == b.flex {
                j += 1;
            }
            let run = &vis[bi..j];
            let mut cols: Vec<(u32, Vec<&crate::html::Block>)> = Vec::new();
            for x in run {
                match cols.iter_mut().find(|c| c.0 == x.fcol) {
                    Some(c) => c.1.push(x),
                    None => cols.push((x.fcol, alloc::vec![*x])),
                }
            }
            let per_row = if b.fmode == 3 { (b.gcols.max(1) as usize).min(cols.len().max(1)) } else { cols.len().clamp(1, 8) };
            let gap = if b.fgap > 0.0 { lpx(b.fgap) } else { u / 2 };
            y += u / 4;
            for row in cols.chunks(per_row) {
                let n = row.len();
                let avail = full.saturating_sub(gap * (n.saturating_sub(1)));
                // Kengliklar: belgilanganlari (px / %) — qolgani teng (yoki flex-grow ulushida)
                let fixed: Vec<Option<usize>> = row
                    .iter()
                    .map(|c| match c.1[0].fw {
                        Some((v, true)) if b.fmode != 3 => Some(full * (v as usize).min(100) / 100),
                        Some((v, false)) if b.fmode != 3 => Some(lpx(v).min(avail)),
                        _ => None,
                    })
                    .collect();
                let used: usize = fixed.iter().flatten().sum();
                let grows: Vec<f32> = row.iter().zip(fixed.iter()).map(|(c, f)| if f.is_some() { 0.0 } else if c.1[0].fgrow > 0.0 { c.1[0].fgrow } else { 1.0 }).collect();
                let gsum: f32 = grows.iter().sum::<f32>().max(0.001);
                let rest = avail.saturating_sub(used);
                let widths: Vec<usize> = fixed.iter().zip(grows.iter()).map(|(f, g)| f.unwrap_or((rest as f32 * g / gsum) as usize).max(u * 2)).collect();
                let total: usize = widths.iter().sum::<usize>() + gap * (n.saturating_sub(1));
                let extra = full.saturating_sub(total);
                let (mut x, step_extra) = match b.fjust {
                    1 => (page_pad + extra / 2, 0),
                    3 => (page_pad + extra, 0),
                    2 if n > 1 => (page_pad, extra / (n - 1)),
                    _ => (page_pad, 0),
                };
                let mut bottom = y;
                let mut rs_new: Vec<(usize, u8)> = Vec::new();
                for (c, w) in row.iter().zip(widths.iter()) {
                    let d0 = deco.len();
                    // dir="rtl": ustunlar (flex / grid / jadval kataklari) öngdan çapga
                    let cx = if b.rtl { (page_pad * 2 + full).saturating_sub(x + w) } else { x };
                    let yy = lay(web, &c.1, cx, *w, y, b.flex, items, deco, anc);
                    bottom = bottom.max(yy);
                    if c.1[0].rspan > 1 && deco.len() > d0 {
                        rs_new.push((d0, c.1[0].rspan - 1));
                    }
                    x += w + gap + step_extra;
                }
                // Yuqoridan kelayotgan rowspan kataklari shu qator pastigaça çoziladi
                for (di, left) in rs_pend.iter_mut() {
                    if let Some(d) = deco.get_mut(*di) {
                        d.h = d.h.max(bottom.saturating_sub(d.y));
                    }
                    *left -= 1;
                }
                rs_pend.retain(|p| p.1 > 0);
                rs_pend.extend(rs_new);
                y = bottom + u / 4;
            }
            open_box = None;
            bi = j;
            continue;
        }
        rs_pend.clear();
        bi += 1;
        let bi = bi - 1;
        let base_px = pxs(b.size);
        // --- Konteyner (quti) geometriyasi ---
        let (bx, bwid, inset) = if b.bgk != 0 {
            let cw = match b.maxw {
                Some((v, true)) => full * (v as usize).min(100) / 100,
                Some((v, false)) => lpx(v).min(full),
                None => full,
            }
            .max(u * 4);
            let cx = page_pad + if b.center { (full - cw) / 2 } else { 0 };
            let bwp = if b.bw > 0.0 { lpx(b.bw).max(1) } else { 0 };
            let padp = lpx(b.pad).max(if b.bg.is_some() || bwp > 0 { u / 2 } else { 0 });
            let inset = padp + bwp;
            if open_box.map(|o| o.0) != Some(b.bgk) {
                y += lpx(b.bmt);
                let shadow = b.shadow.map(|s| (spx(s.dx) as i32, spx(s.dy) as i32, lpx(s.blur), spx(s.spread) as i32, s.color, s.alpha));
                deco.push(Deco { x: cx, y, w: cw, h: 0, fill: b.bg, radius: lpx(b.radius), bw: bwp, bc: b.bc.unwrap_or(0x808080), grad: b.grad, shadow });
                open_box = Some((b.bgk, deco.len() - 1, inset));
                if b.bdx != 0.0 || b.bdy != 0.0 {
                    box_shifts.push((deco.len() - 1, spx(b.bdx), spx(b.bdy)));
                }
                y += inset;
            }
            (cx + inset, cw.saturating_sub(inset * 2), inset)
        } else {
            open_box = None;
            (page_pad, full, 0)
        };
        let _ = inset;
        if b.rdx != 0.0 || b.rdy != 0.0 {
            pend = Some((items.len(), deco.len(), spx(b.rdx), spx(b.rdy)));
        }
        let pad = bx;
        let maxw = bwid;
        let (indent, gap_top, gap_bot) = match b.kind {
            HK::H(1) => (0, u, u / 2),
            HK::H(_) => (0, u * 2 / 3, u / 4),
            HK::Li(d, _) => (d as usize * u * 3 / 2, u / 8, u / 8),
            HK::Pre => (u / 2, u / 2, u / 2),
            HK::Quote => (u * 3 / 2, u / 2, u / 2),
            HK::Hr => (0, u / 2, u / 2),
            HK::Img => (0, u / 3, u / 3),
            HK::P => (0, u / 4, u / 3),
            HK::Indent(d) => (d as usize * u * 2, u / 8, u / 4),
        };
        let bcol = b.color.unwrap_or(match b.kind {
            HK::H(_) => head_col,
            HK::Quote => mix(text_col, page_bg, 70),
            _ => text_col,
        });
        // birinçi blok qutida — yuqori bo'şliqsiz
        let first_in_box = b.bgk != 0 && (bi == 0 || vis[bi - 1].bgk != b.bgk);
        y += if first_in_box { 0 } else { gap_top } + if b.bgk == 0 { lpx(b.mt) } else { 0 };
        let y_block = y;
        let last_in_box = b.bgk != 0 && vis.get(bi + 1).is_none_or(|n| n.bgk != b.bgk);
        if b.hser != 0 && hgrp.map(|g| g.0) != Some(b.hser) {
            hgrp = Some((b.hser, y_block));
        }
        let last_in_h = b.hser != 0 && vis.get(bi + 1).is_none_or(|n| n.hser != b.hser);
        let close_box = |y: &mut usize, deco: &mut Vec<Deco>, open_box: &mut Option<(u32, usize, usize)>| {
            // height: keyingi blok belgilangan balandlikdan keyin (kontent taşib çiqişi mumkin);
            // min-height: kamida şunça
            if let Some((_, y0)) = hgrp.filter(|_| last_in_h) {
                let t = y0 + lpx(b.hmin);
                *y = if b.hfix { t } else { (*y).max(t) };
            }
            if let Some((_, di, ins)) = *open_box {
                if last_in_box {
                    *y += ins;
                    deco[di].h = *y - deco[di].y;
                    *y += lpx(b.bmb);
                    *open_box = None;
                } else {
                    deco[di].h = *y - deco[di].y;
                }
            }
        };
        if b.kind == HK::Hr {
            deco.push(dfill(pad, y, maxw, 1, if dark { 0x34405a } else { 0xc7ccd6 }));
            y += 1 + gap_bot;
            close_box(&mut y, deco, &mut open_box);
            continue;
        }
        if b.kind == HK::Img {
            let (aw, ah) = (b.iw as f32, b.ih as f32);
            // <canvas>: öz piksel buferi (JS çizadi), şaffof joylarda sahifa foni
            if let Some(n) = b.src.strip_prefix("canvas:").and_then(|n| n.parse::<usize>().ok()).filter(|&n| n < doc.canvases.len()) {
                let cv = &doc.canvases[n];
                let nw = lpx(cv.w as f32).max(1);
                let w = nw.min(maxw).max(1);
                let h = (lpx(cv.h as f32) * w / nw).max(1);
                let x = pad + match b.align {
                    1 => (maxw - w) / 2,
                    2 => maxw - w,
                    _ => 0,
                };
                items.push(WItem {
                    x, y, w, h, text: String::new(), col: text_col, px: small(), mono: false, link: b.spans.first().and_then(|s| s.link.clone()),
                    el: b.el, button: false, underline: false, img: Some(CANVAS_IMG + n), input: NO_EL, fx: 0, bold: false, det: NO_EL,
                });
                y += h + if last_in_box { 0 } else { gap_bot };
                close_box(&mut y, deco, &mut open_box);
                continue;
            }
            let found = web.images.iter().position(|(s, _)| *s == b.src);
            let (w, h) = match found {
                Some(i) => {
                    let im = &web.images[i].1;
                    // width/height atributlari (CSS px) bölsa — şunday, aks holda tabiiy ölçam
                    let (nw, nh) = match (b.iw, b.ih) {
                        (0, 0) => (im.w, im.h),
                        (_, 0) => (lpx(aw), lpx(aw) * im.h / im.w.max(1)),
                        (0, _) => (lpx(ah) * im.w / im.h.max(1), lpx(ah)),
                        _ => (lpx(aw), lpx(ah)),
                    };
                    let w = nw.min(maxw).max(1);
                    (w, nh * w / nw.max(1))
                }
                None if b.iw > 0 => (lpx(aw).min(maxw), if b.ih > 0 { lpx(ah) } else { u * 3 }),
                None => (maxw.min(u * 12), u * 3),
            };
            let x = pad + match b.align {
                1 => (maxw - w) / 2,
                2 => maxw - w,
                _ => 0,
            };
            let alt = b.spans.first().map(|s| s.text.clone()).unwrap_or_default();
            let link = b.spans.first().and_then(|s| s.link.clone());
            if found.is_none() {
                deco.push(dfill(x, y, w, h, if dark { 0x232a38 } else { 0xe4e7ec }));
            }
            items.push(WItem {
                x, y, w, h,
                text: if found.is_some() { String::new() } else { alloc::format!("[rasm] {}", alt) },
                col: mix(text_col, page_bg, 110),
                px: small(), mono: false, link, el: b.el, button: false, underline: false, img: found, input: NO_EL, fx: 0, bold: false, det: NO_EL,
            });
            y += h + if last_in_box { 0 } else { gap_bot };
            close_box(&mut y, deco, &mut open_box);
            continue;
        }
        // dir="rtl": chekiniş (ro'yxat, iqtibos) öng tomonda, belgilar — öngda
        let rtl = b.rtl;
        let x0 = pad + if rtl { 0 } else { indent };
        let xr = pad + maxw - if rtl { indent } else { 0 };
        if b.summary {
            // ▶ / ▼ — bosilsa <details> oçiladi
            let open = doc.details.get(b.det as usize).copied().unwrap_or(false);
            let t = String::from(if open { "▼" } else { "▶" });
            let tw = gfx::text_width(&t, base_px);
            items.push(WItem { x: if rtl { xr.saturating_sub(tw) } else { x0 }, y, w: tw + u / 2, h: base_px, text: t, col: mix(bcol, page_bg, 60), px: base_px, mono: false, link: None, el: NO_EL, button: false, underline: false, img: None, input: NO_EL, fx: 0, bold: false, det: b.det });
        }
        let x0 = if b.summary && !rtl { x0 + base_px + u / 3 } else { x0 };
        let xr = if b.summary && rtl { xr.saturating_sub(base_px + u / 3) } else { xr };
        // ro'yxat belgisi: ltr — matndan çapda, rtl — öngda
        let mark_x = |tw: usize| if rtl { xr + u / 3 } else { x0.saturating_sub(tw + u / 3) };
        if let HK::Li(d, n) = b.kind {
            if n == 0 || b.mark.is_empty() && n == 0 {
                // ichma-ich ro'yxatlar: ● ○ ■
                let t = String::from(match (d.max(1) - 1) % 3 {
                    0 => "•",
                    1 => "◦",
                    _ => "▪",
                });
                let tw = gfx::text_width(&t, base_px);
                items.push(WItem { x: mark_x(tw), y, w: tw, h: base_px, text: t, col: mix(bcol, page_bg, 60), px: base_px, mono: false, link: None, el: NO_EL, button: false, underline: false, img: None, input: NO_EL, fx: 0, bold: false, det: NO_EL });
            } else {
                let t = if b.mark.is_empty() { alloc::format!("{}.", n) } else { b.mark.clone() };
                let tw = gfx::text_width(&t, base_px);
                items.push(WItem { x: mark_x(tw), y, w: tw, h: base_px, text: t, col: mix(bcol, page_bg, 70), px: base_px, mono: false, link: None, el: NO_EL, button: false, underline: false, img: None, input: NO_EL, fx: 0, bold: false, det: NO_EL });
            }
        }
        if b.kind == HK::Pre {
            let all: String = b.spans.iter().map(|s| s.text.as_str()).collect();
            let lhm = if b.lh > 0.0 { ((base_px as f32 * b.lh) as usize).max(1) } else { gfx::line_height(base_px, Font::Mono).max(base_px + 3) };
            let lines: Vec<&str> = all.lines().collect();
            let h = lines.len() * lhm + u;
            deco.push(dfill(pad, y, maxw, h, if b.bgk != 0 && b.bg.is_some() { mix(b.bg.unwrap(), 0x000000, 20) } else if dark { 0x10151f } else { 0xeef0f3 }));
            let mut yy = y + u / 2;
            for l in lines {
                let w = gfx::text_width_mono(l, base_px);
                let off = match b.align {
                    1 => (xr - x0).saturating_sub(w) / 2,
                    2 => (xr - x0).saturating_sub(w),
                    _ => 0,
                };
                items.push(WItem { x: x0 + off, y: yy, w, h: base_px, text: String::from(l), col: b.color.unwrap_or(if dark { 0xd7e3c8 } else { 0x24292f }), px: base_px, mono: true, link: None, el: b.el, button: false, underline: false, img: None, input: NO_EL, fx: 0, bold: false, det: NO_EL });
                yy += lhm;
            }
            y += h + if last_in_box { 0 } else { gap_bot };
            close_box(&mut y, deco, &mut open_box);
            continue;
        }
        // Qatorlarga bölib joylaş (tekislaş bilan)
        let mut x = x0;
        let mut line_start = items.len();
        // line-height: kö'paytuvçi (0 — normal = 1.4); matn qator içida markazlanadi (yarim oraliq)
        let lhf = b.lh;
        let norm = |px: usize| if lhf > 0.0 { ((px as f32 * lhf) as usize).max(1) } else { px * 14 / 10 };
        let lead = if lhf > 0.0 { norm(base_px).saturating_sub(base_px) / 2 } else { 0 };
        let mut line_h = norm(base_px);
        let line_w = xr - x0;
        // Qatorni tekislaş: 1 — markaz, 2 — öng, 3 — justify (oxirgi qatordan taşqari).
        // Qator oxiridagi böşliq hisobga olinmaydi (öngga tekislangan matn çetga taqaladi).
        let finish = |items: &mut Vec<WItem>, start: usize, x: usize, last: bool| {
            if b.align == 0 || start >= items.len() {
                return;
            }
            let tail = items.last().map(|it| {
                if it.img.is_none() && it.input == NO_EL && it.text.ends_with(' ') {
                    if it.mono { gfx::text_width_mono(" ", it.px) } else { gfx::text_width(" ", it.px) }
                } else {
                    0
                }
            }).unwrap_or(0);
            let used = (x - x0).saturating_sub(tail);
            let free = line_w.saturating_sub(used);
            if b.align == 3 {
                if last {
                    if rtl {
                        for it in items[start..].iter_mut() {
                            it.x += free;
                        }
                    }
                    return;
                }
                // bo'şliqlar (söz oxiridagi ' ') orasida teng taqsimlaymiz
                let n = items.len() - start;
                let gaps = items[start..n + start - 1].iter().filter(|it| it.text.ends_with(' ')).count();
                if gaps == 0 {
                    return;
                }
                let mut k = 0;
                for it in items[start..].iter_mut() {
                    it.x += free * k / gaps;
                    if it.text.ends_with(' ') {
                        k += 1;
                    }
                }
                return;
            }
            let off = if b.align == 1 { free / 2 } else { free };
            for it in items[start..].iter_mut() {
                it.x += off;
            }
        };
        for s in &b.spans {
            let spx = pxs(s.size);
            if s.br {
                finish(items, line_start, x, true);
                x = x0;
                y += line_h;
                line_start = items.len();
                line_h = norm(base_px);
                continue;
            }
            // Forma maydoni (matn, parol, belgilaş, yuboriş)
            if s.input != NO_EL && !s.button {
                let inp = &doc.inputs[s.input as usize];
                let (ww, hh) = match inp.kind {
                    2 => (spx + u / 2, spx),
                    7 => (spx * 2, spx * 6 / 5),
                    8 => ((u * 8).min(maxw), spx),
                    9 => ((u * 8).min(maxw), spx * 2 / 3),
                    1 => (gfx::text_width(&inp.value, spx) + u * 3 / 2, spx * 7 / 5),
                    _ => ((inp.size.clamp(4, 60) as usize * spx * 11 / 20).min(maxw), spx * 8 / 5),
                };
                if x + ww > xr && x > x0 {
                    finish(items, line_start, x, false);
                    x = x0;
                    y += line_h;
                    line_start = items.len();
                }
                line_h = line_h.max(hh + u / 2);
                items.push(WItem { x, y, w: ww, h: hh, text: String::new(), col: bcol, px: spx, mono: false, link: None, el: s.el, button: false, underline: false, img: None, input: s.input, fx: 0, bold: false, det: NO_EL });
                x += ww + u / 3;
                continue;
            }
            let c = s.color.unwrap_or(if s.link.is_some() { link_col } else if s.code { code_col } else { bcol });
            let mut rest = s.text.as_str();
            while !rest.is_empty() {
                let cut = if s.button { rest.len() } else { rest.find(' ').map(|i| i + 1).unwrap_or(rest.len()) };
                let word = &rest[..cut];
                rest = &rest[cut..];
                let ww = if s.code { gfx::text_width_mono(word, spx) } else { gfx::text_width(word, spx) }
                    + if s.button || s.fx & crate::html::FX_KBD != 0 { u / 2 } else { 0 }
                    + if s.bold { 1 } else { 0 };
                if x + ww > xr && x > x0 {
                    finish(items, line_start, x, false);
                    x = x0;
                    y += line_h;
                    line_start = items.len();
                    line_h = norm(base_px);
                    if word == " " {
                        continue;
                    }
                }
                line_h = line_h.max(norm(spx));
                // Tayanç çiziq (baseline) böyiça tekislaş: kiçik matn pastga suriladi
                let off = base_px.saturating_sub(spx) * 4 / 5;
                let yo = lead + if s.fx & crate::html::FX_SUP != 0 { y.saturating_sub(spx / 6) } else if s.fx & crate::html::FX_SUB != 0 { y + base_px * 2 / 5 } else { y + off };
                items.push(WItem { x, y: yo, w: ww, h: spx, text: String::from(word), col: c, px: spx, mono: s.code, link: s.link.clone(), el: s.el, button: s.button, underline: s.link.is_some() && s.underline, img: None, input: s.input, fx: s.fx, bold: s.bold, det: if b.summary { b.det } else { NO_EL } });
                x += ww;
            }
        }
        finish(items, line_start, x, true);
        y += line_h;
        if b.kind == HK::Quote {
            deco.push(dfill(if rtl { (pad + maxw).saturating_sub(u / 2 + 3) } else { pad + u / 2 }, y_block, 3, y - y_block, accent()));
        }
        if b.kind == HK::H(1) && b.bgk == 0 {
            deco.push(dfill(pad, y + u / 6, maxw, 1, if dark { 0x2a3346 } else { 0xd0d5dd }));
            y += u / 3;
        }
        if !last_in_box {
            y += gap_bot;
        }
        if b.bgk == 0 {
            y += lpx(b.mb);
        }
        close_box(&mut y, deco, &mut open_box);
    }
    if let Some((i0, d0, dx, dy)) = pend.take() {
        shifts.push((i0, items.len(), d0, deco.len(), dx, dy));
    }
    // position: relative — joylaşuvdan keyin siljitamiz (oqimdagi joyi saqlanadi)
    let mv = |v: usize, d: isize| (v as isize + d).max(0) as usize;
    for (i0, i1, d0, d1, dx, dy) in shifts {
        for it in items[i0..i1].iter_mut() {
            (it.x, it.y) = (mv(it.x, dx), mv(it.y, dy));
        }
        for d in deco[d0..d1].iter_mut() {
            (d.x, d.y) = (mv(d.x, dx), mv(d.y, dy));
        }
    }
    for (di, dx, dy) in box_shifts {
        if let Some(d) = deco.get_mut(di) {
            (d.x, d.y) = (mv(d.x, dx), mv(d.y, dy));
        }
    }
    // float lar konteyner içida qoladi
    y.max(lf.1).max(rf.1)
}

fn draw_browser(win: &Window) {
    let u = body();
    draw_tabs(win);
    let Some(web) = &win.web else { return };
    let (by, bh, btns, (ax, ay, aw, ah), (px0, py0, pw, ph)) = web_geo(win);
    for (k, bx) in btns.iter().enumerate() {
        let (c, m) = ((*bx + bh / 2) as f32, (by + bh / 2) as f32);
        let on = k != 0 || !web.hist.is_empty();
        fill_round_rect_a(*bx, by, bh, bh, bh / 3, 0xffffff, if on { 18 } else { 8 });
        let col = if on { 0xdbe3ee } else { 0x4a5568 };
        let d = u as f32 / 3.5;
        let t = (u as f32 / 12.0).max(1.2);
        match k {
            0 => {
                seg(c + d * 0.4, m - d, c - d * 0.5, m, t, col);
                seg(c - d * 0.5, m, c + d * 0.4, m + d, t, col);
            }
            1 => {
                use crate::image::vector::{cos, sin};
                if web.loading.is_some() {
                    // yuklanmoqda: X (töxtatiş belgisi)
                    seg(c - d * 0.7, m - d * 0.7, c + d * 0.7, m + d * 0.7, t, col);
                    seg(c - d * 0.7, m + d * 0.7, c + d * 0.7, m - d * 0.7, t, col);
                } else {
                    for i in 0..18 {
                        let a0 = 0.6 + i as f32 * 0.29;
                        let a1 = a0 + 0.29;
                        seg(c + cos(a0) * d, m + sin(a0) * d, c + cos(a1) * d, m + sin(a1) * d, t, col);
                    }
                    seg(c + cos(0.6) * d, m + sin(0.6) * d, c + cos(0.6) * d + d * 0.5, m + sin(0.6) * d - d * 0.1, t, col);
                }
            }
            _ => {
                gfx::fill_poly(&[(c, m - d), (c + d, m), (c - d, m)], col);
                fill_rect((c - d * 0.65) as usize, m as usize, (d * 1.3) as usize, (d * 0.85) as usize, col);
            }
        }
    }
    gfx::fill_round_rect(ax, ay, aw, ah, ah / 2, 0x10151f);
    gfx::round_outline(ax, ay, aw, ah, ah / 2, if web.editing { 2 } else { 1 }, if web.editing { accent() } else { 0x2a3346 });
    let secure = web.url.starts_with("https://");
    let local = !web.url.contains("://") || web.url.starts_with("zamin:");
    gfx::fill_circle(ax + u * 3 / 4, ay + ah / 2, (u / 5).max(2), if secure { 0x34c759 } else if local { 0x64d2ff } else { 0xff9f0a });
    let shown = if web.editing { &web.edit } else { &web.url };
    let maxw = aw.saturating_sub(u * 2);
    let t = if web.editing {
        let v: Vec<char> = shown.chars().collect();
        let mut k = v.len();
        while k > 0 && gfx::text_width(&v[k - 1..].iter().collect::<String>(), small()) < maxw {
            k -= 1;
        }
        v[k..].iter().collect::<String>()
    } else {
        fit_text(shown, maxw, small())
    };
    let ex = gfx::draw_text(ax + u * 3 / 2, ay + (ah - small()) / 2, &t, 0xdbe3ee, small());
    if web.editing {
        fill_rect(ex + 1, ay + 4, 2, ah - 8, 0x63a8ff);
    }
    // Yuklaş çizigʻi
    fill_rect(win.x + 1, py0 - 1, win.w.saturating_sub(2), 1, 0x2a3346);
    if web.loading.is_some() {
        let el = crate::timer::uptime_ms().saturating_sub(web.t0);
        let span = win.w / 4;
        let pos = (el as usize / 4) % (win.w + span);
        let x0 = (win.x + pos).saturating_sub(span);
        let x1 = (win.x + pos).min(win.x + win.w);
        if x1 > x0.max(win.x) {
            fill_rect(x0.max(win.x), py0 - 2, x1 - x0.max(win.x), 3, accent());
        }
    }
    let (page_bg, ..) = web_palette(&web.doc);
    fill_rect(px0, py0, pw, ph, page_bg);
    gfx::set_clip(px0, py0, pw, ph);
    let (items, deco, total) = web_layout(web, pw.saturating_sub(u));
    let sc = web.scroll.min(total.saturating_sub(ph));
    for d in deco {
        if d.h > 0 && d.y + d.h >= sc && d.y < sc + ph {
            let top = d.y.max(sc);
            let (dx, dy, dh) = (px0 + d.x, py0 + top - sc, d.y + d.h - top);
            // Yuqoridan qisman aylantirilgan quti: faqat yuqori burçaklar kesiladi,
            // pastkilari yumaloq qoladi.
            let rb = d.radius.min(d.w / 2).min(dh / 2);
            let rt = if d.y >= sc { rb } else { 0 };
            if let Some((sx, sy, blur, spread, c, a)) = d.shadow {
                // soya — butun quti böyiça (yuqorisi aylantirilgan bölsa ham), qirqiş sohasi çeklaydi
                let ey = py0 as i32 + d.y as i32 - sc as i32;
                gfx::box_shadow(dx as i32, ey, d.w, d.h, d.radius.min(d.w / 2).min(d.h / 2), sx, sy, blur, spread, c, a as u32);
            }
            if let Some(g) = d.grad.filter(|g| g.n >= 2) {
                gfx::gradient_round_rect(dx, dy, d.w, dh, rt, rb, g.angle, &g.stops[..g.n as usize], top - d.y);
            } else if let Some(c) = d.fill {
                if rb > 0 {
                    gfx::fill_round_rect_tb(dx, dy, d.w, dh, rt, rb, c);
                } else {
                    fill_rect(dx, dy, d.w, dh, c);
                }
            }
            if d.bw > 0 {
                if d.y >= sc {
                    gfx::round_outline(dx, dy, d.w, dh, rb, d.bw, d.bc);
                } else {
                    // yuqori chegara körinmaydi: uni qirqiş sohasidan tepaga çiqaramiz
                    gfx::round_outline_tb(dx, dy.saturating_sub(d.bw), d.w, dh + d.bw, 0, rb, d.bw, d.bc);
                }
            }
        }
    }
    for it in &items {
        if it.y + it.h < sc || it.y > sc + ph {
            continue;
        }
        let (x, y) = (px0 + it.x, (py0 + it.y).saturating_sub(sc));
        if let Some(i) = it.img.filter(|&i| i >= CANVAS_IMG) {
            if let Some(cv) = web.doc.canvases.get(i - CANVAS_IMG) {
                if it.y >= sc {
                    gfx::blit_alpha(x, y, it.w, it.h, cv.w, cv.h, &cv.px);
                } else {
                    let skip = sc - it.y;
                    let sy = (skip * cv.h / it.h.max(1)).min(cv.h.saturating_sub(1));
                    let dh = it.h.saturating_sub(skip);
                    if dh > 0 {
                        gfx::blit_alpha(x, py0, it.w, dh, cv.w, cv.h - sy, &cv.px[sy * cv.w..]);
                    }
                }
            }
            continue;
        }
        if let Some(i) = it.img {
            let im = &web.images[i].1;
            if it.y >= sc {
                gfx::blit_scaled(x, y, it.w, it.h, im.w, im.h, &im.px);
            } else {
                // Yuqoridan qisman körinadi: manba qatorlarining pastki qismini çizamiz
                let skip = sc - it.y; // ekranda yaşiringan qator (piksel)
                let sy = (skip * im.h / it.h.max(1)).min(im.h.saturating_sub(1));
                let rest_h = im.h - sy;
                let dh = it.h.saturating_sub(skip);
                if dh > 0 {
                    gfx::blit_scaled(x, py0, it.w, dh, im.w, rest_h, &im.px[sy * im.w..]);
                }
            }
            continue;
        }
        if it.input != NO_EL && !it.button {
            let inp = &web.doc.inputs[it.input as usize];
            let focused = web.focus == Some(it.input as usize);
            let dark = luma(page_bg) < 128;
            let (fbg, fbd, ftx) = if dark { (0x10151f, 0x3a4458, 0xe7edf6) } else { (0xffffff, 0xb8bec9, 0x1d1d1f) };
            match inp.kind {
                2 if inp.radio => {
                    let r = it.px / 2;
                    let (cx2, cy2) = (x + r, y + r);
                    gfx::fill_circle(cx2, cy2, r, if inp.checked { accent() } else { fbd });
                    gfx::fill_circle(cx2, cy2, r.saturating_sub(1), if inp.checked { accent() } else { fbg });
                    if inp.checked {
                        gfx::fill_circle(cx2, cy2, r * 2 / 5, OQ);
                    }
                }
                7 => {
                    let c = crate::css::parse_color(&inp.value).unwrap_or(0);
                    gfx::fill_round_rect(x, y, it.w, it.h, u / 4, fbg);
                    gfx::round_outline(x, y, it.w, it.h, u / 4, 1, fbd);
                    gfx::fill_round_rect(x + 3, y + 3, it.w - 6, it.h - 6, u / 6, c);
                }
                8 => {
                    let v: f32 = inp.value.trim().parse().unwrap_or(inp.min);
                    let t = ((v - inp.min) / (inp.max - inp.min).max(0.0001)).clamp(0.0, 1.0);
                    let my = y + it.h / 2;
                    gfx::fill_round_rect(x, my - 2, it.w, 4, 2, fbd);
                    let kx = x + (t * (it.w.saturating_sub(it.h)) as f32) as usize + it.h / 2;
                    gfx::fill_round_rect(x, my - 2, kx - x, 4, 2, accent());
                    gfx::fill_circle(kx, my, it.h / 2, OQ);
                    gfx::fill_circle(kx, my, it.h / 2 - 2, accent());
                }
                9 => {
                    let v: f32 = inp.value.trim().parse().unwrap_or(0.0);
                    let t = ((v - inp.min) / (inp.max - inp.min).max(0.0001)).clamp(0.0, 1.0);
                    gfx::fill_round_rect(x, y, it.w, it.h, it.h / 2, fbd);
                    let fw = ((it.w as f32) * t) as usize;
                    if fw > 0 {
                        gfx::fill_round_rect(x, y, fw.max(it.h), it.h, it.h / 2, if t > 0.8 { 0x2fbf5f } else { accent() });
                    }
                }
                2 => {
                    let b = it.px;
                    gfx::fill_round_rect(x, y, b, b, b / 5, if inp.checked { accent() } else { fbg });
                    gfx::round_outline(x, y, b, b, b / 5, 1, if inp.checked { accent() } else { fbd });
                    if inp.checked {
                        let (fx, fy, q) = (x as f32, y as f32, b as f32);
                        seg(fx + q * 0.22, fy + q * 0.52, fx + q * 0.42, fy + q * 0.72, (q / 9.0).max(1.2), OQ);
                        seg(fx + q * 0.42, fy + q * 0.72, fx + q * 0.8, fy + q * 0.28, (q / 9.0).max(1.2), OQ);
                    }
                }
                6 => {
                    gfx::fill_round_rect(x, y, it.w, it.h, u / 4, fbg);
                    gfx::round_outline(x, y, it.w, it.h, u / 4, 1, if web.select_open == Some(it.input as usize) { accent() } else { fbd });
                    let label = inp.options.iter().find(|o| o.0 == inp.value).map(|o| o.1.clone()).unwrap_or_default();
                    gfx::draw_text(x + u / 3, y + (it.h - it.px) / 2, &fit_text(&label, it.w.saturating_sub(u * 3 / 2), it.px), ftx, it.px);
                    let (ax, ay) = ((x + it.w - u * 2 / 3) as f32, (y + it.h / 2) as f32);
                    let d = it.px as f32 / 4.0;
                    gfx::fill_poly(&[(ax - d, ay - d * 0.5), (ax + d, ay - d * 0.5), (ax, ay + d * 0.7)], ftx);
                }
                1 => {
                    gfx::fill_round_rect(x, y, it.w, it.h, it.h / 3, if dark { 0x3a4458 } else { 0xe8ebf0 });
                    gfx::round_outline(x, y, it.w, it.h, it.h / 3, 1, fbd);
                    gfx::draw_text_centered(x + it.w / 2, y + (it.h - it.px) / 2, &inp.value, ftx, it.px);
                }
                _ => {
                    gfx::fill_round_rect(x, y, it.w, it.h, u / 4, fbg);
                    gfx::round_outline(x, y, it.w, it.h, u / 4, if focused { 2 } else { 1 }, if focused { accent() } else { fbd });
                    let shown: String = if inp.kind == 3 { inp.value.chars().map(|_| '*').collect() } else { inp.value.clone() };
                    let tx = x + u / 3;
                    let ty = y + (it.h - it.px) / 2;
                    let avail = it.w.saturating_sub(u * 2 / 3);
                    if shown.is_empty() && !focused {
                        gfx::draw_text(tx, ty, &fit_text(&inp.placeholder, avail, it.px), mix(ftx, fbg, 140), it.px);
                    } else {
                        // oxiri körinsin
                        let v: Vec<char> = shown.chars().collect();
                        let mut k = v.len();
                        while k > 0 && gfx::text_width(&v[k - 1..].iter().collect::<String>(), it.px) <= avail {
                            k -= 1;
                        }
                        let t: String = v[k..].iter().collect();
                        let ex = gfx::draw_text(tx, ty, &t, ftx, it.px);
                        if focused {
                            fill_rect(ex + 1, y + 3, 2, it.h.saturating_sub(6), accent());
                        }
                    }
                }
            }
            continue;
        }
        let light = luma(page_bg) >= 128;
        if it.button {
            let (bf, bb) = if light { (0xe8ebf0, 0xb8bec9) } else { (0x3a4458, 0x5a6478) };
            gfx::fill_round_rect(x, y.saturating_sub(it.px / 5), it.w.saturating_sub(u / 6), it.px * 7 / 5, it.px / 3, bf);
            gfx::round_outline(x, y.saturating_sub(it.px / 5), it.w.saturating_sub(u / 6), it.px * 7 / 5, it.px / 3, 1, bb);
        }
        use crate::html::{FX_DOTTED, FX_ITALIC, FX_KBD, FX_MARK, FX_STRIKE, FX_SUB, FX_SUP, FX_UNDER};
        let tx = if it.button || it.fx & FX_KBD != 0 { x + u / 4 } else { x };
        let col = if it.button && !light && it.col == web_palette(&web.doc).1 { OQ } else { it.col };
        let px = if it.fx & (FX_SUP | FX_SUB) != 0 { it.px * 3 / 4 } else { it.px };
        let tw = it.w.saturating_sub(if it.text.ends_with(' ') { gfx::text_width(" ", px) } else { 0 });
        if it.fx & FX_MARK != 0 {
            fill_rect(x, y.saturating_sub(1), it.w, px + 4, if light { 0xfff1a8 } else { 0x6b5a00 });
        }
        if it.fx & FX_KBD != 0 {
            gfx::fill_round_rect(x, y.saturating_sub(2), tw, px + 5, 3, if light { 0xeef0f3 } else { 0x262f40 });
            gfx::round_outline(x, y.saturating_sub(2), tw, px + 5, 3, 1, if light { 0xb8bec9 } else { 0x4a5568 });
        }
        let col = if it.fx & FX_MARK != 0 && light { 0x1d1d1f } else { col };
        let draw_one = |dx: usize| {
            if it.mono {
                gfx::draw_text_mono(tx + dx, y, &it.text, col, px);
            } else {
                gfx::draw_text(tx + dx, y, &it.text, col, px);
            }
        };
        if it.fx & FX_ITALIC != 0 {
            // Kursiv: matn gorizontal tasmalarga bölinib, yuqorisi öngga suriladi (qiyalik)
            let bands = 4usize;
            let bh = (px + 4).div_ceil(bands);
            for k in 0..bands {
                let by = y + k * bh;
                let shift = (bands - 1 - k) * (px / 10).max(1) / 2;
                gfx::set_clip(px0, by.max(py0), pw, bh);
                draw_one(shift);
                if it.bold {
                    draw_one(shift + 1);
                }
            }
            gfx::set_clip(px0, py0, pw, ph);
        } else {
            draw_one(0);
            if it.bold {
                draw_one(1); // haqiqiy qalinlik: bir piksel surib qayta chiziş
            }
        }
        let lc = mix(it.col, page_bg, 90);
        if it.underline || it.fx & FX_UNDER != 0 || (it.link.is_some() && !it.button) {
            fill_rect(x, y + px + 1, tw, 1, lc);
        }
        if it.fx & FX_DOTTED != 0 {
            let mut dx = 0;
            while dx < tw {
                fill_rect(x + dx, y + px + 1, 1, 1, lc);
                dx += 3;
            }
        }
        if it.fx & FX_STRIKE != 0 {
            fill_rect(x, y + px * 11 / 20, tw, 1, col);
        }
    }
    // Oçiq <select> ro'yxati
    if let Some((x, y, w, rh, opts, cur)) = select_popup(web, &items, sc, px0, py0) {
        let h = rh * opts.len();
        gfx::shadow(x, y, w, h, 4, 8);
        gfx::fill_round_rect(x, y, w, h, 4, 0x1a2131);
        gfx::round_outline(x, y, w, h, 4, 1, 0x34405a);
        for (k, (_, l)) in opts.iter().enumerate() {
            if k == cur {
                fill_rect(x + 2, y + k * rh + 1, w - 4, rh - 2, accent());
            }
            gfx::draw_text(x + u / 2, y + k * rh + (rh - small()) / 2, &fit_text(l, w - u, small()), OQ, small());
        }
    }
    if total > ph {
        let th = (ph * ph / total).max(u);
        let ty = py0 + sc * (ph - th) / (total - ph).max(1);
        fill_round_rect_a(px0 + pw - 7, ty, 4, th, 2, if luma(page_bg) < 128 { 0xffffff } else { 0x000000 }, 60);
    }
    // JS konsoli (F12)
    if web.console {
        let chh = (ph / 3).max(u * 5);
        let cy = py0 + ph - chh;
        fill_round_rect_a(px0, cy, pw, chh, 0, 0x0b0f16, 235);
        fill_rect(px0, cy, pw, 1, accent());
        gfx::draw_text(px0 + u / 2, cy + u / 4, "Konsol (F12)", 0x8ea0b6, small() * 4 / 5);
        let log: Vec<String> = web.js.as_ref().map(|e| e.log.clone()).unwrap_or_default();
        let lh = small() + 2;
        let rows = (chh.saturating_sub(u + 4)) / lh;
        for (k, l) in log.iter().rev().take(rows).rev().enumerate() {
            let col = if l.starts_with("Xato") || l.starts_with("Sintaksis") { 0xff6b61 } else { 0xd7e3c8 };
            gfx::draw_text_mono(px0 + u / 2, cy + u + k * lh, &fit_text(l, pw - u, small() * 9 / 10), col, small() * 9 / 10);
        }
    }
    gfx::set_clip(win.x + 1, win.y + title_h(), win.w.saturating_sub(2), win.h.saturating_sub(title_h() + 1));
    gfx::draw_text(win.x + u / 2, py0 + ph + (small() + u / 2 - small() * 4 / 5) / 2, &fit_text(&web.status, win.w - u, small() * 4 / 5), 0x7d8ca3, small() * 4 / 5);
}

/// Oçiq <select> ro'yxati geometriyasi: (x, y, w, qator balandligi, variantlar, tanlangan).
fn select_popup(web: &Web, items: &[WItem], sc: usize, px0: usize, py0: usize) -> Option<(usize, usize, usize, usize, Vec<(String, String)>, usize)> {
    let k = web.select_open?;
    let it = items.iter().find(|it| it.input as usize == k)?;
    let inp = web.doc.inputs.get(k)?;
    let rh = small() + body() / 2;
    let cur = inp.options.iter().position(|o| o.0 == inp.value).unwrap_or(0);
    Some((px0 + it.x, (py0 + it.y + it.h).saturating_sub(sc), it.w.max(body() * 6), rh, inp.options.clone(), cur))
}

/// #yakor: id (yoki <a name>) li elementgaça aylantiradi.
fn web_anchor(win: &mut Window, frag: &str) {
    let Some(web) = &win.web else { return };
    let frag = crate::js::url_decode(frag);
    let Some(target) = web.doc.find_id(&frag) else { return };
    let (_, _, _, _, (_, _, pw, _)) = web_geo(win);
    let (items, _, _) = web_layout(web, pw.saturating_sub(body()));
    let y = items.iter().filter(|it| web.doc.within(it.el, target as u16)).map(|it| it.y).min();
    let max = web_max_scroll(win);
    if let (Some(y), Some(w)) = (y, win.web.as_mut()) {
        w.scroll = y.saturating_sub(body()).min(max);
    }
}

fn web_max_scroll(win: &Window) -> usize {
    let Some(web) = &win.web else { return 0 };
    let (_, _, _, _, (_, _, pw, ph)) = web_geo(win);
    let (_, _, total) = web_layout(web, pw.saturating_sub(body()));
    total.saturating_sub(ph)
}

fn web_scroll(win: &mut Window, d: isize) {
    let max = web_max_scroll(win);
    if let Some(w) = win.web.as_mut() {
        w.scroll = (w.scroll.min(max) as isize + d).clamp(0, max as isize) as usize;
    }
}

fn web_back(win: &mut Window) {
    if let Some(prev) = win.web.as_mut().and_then(|w| w.hist.pop()) {
        web_go(win, &prev, false);
    }
}

fn web_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    if let Some(r) = tabs_click(win, cx, cy) {
        if !r {
            // oxirgi tab yopildi — oynaning özi yopilsin
            win.web = None;
        }
        return true;
    }
    // Oçiq <select> ro'yxati: tanlaş
    if let Some(web) = &win.web {
        if web.select_open.is_some() {
            let (_, _, _, _, (px0, py0, pw, ph)) = web_geo(win);
            let (items, _, total) = web_layout(web, pw.saturating_sub(body()));
            let sc = web.scroll.min(total.saturating_sub(ph));
            let pop = select_popup(web, &items, sc, px0, py0);
            let k = web.select_open.unwrap();
            let w = win.web.as_mut().unwrap();
            w.select_open = None;
            if let Some((x, y, pw2, rh, opts, _)) = pop {
                if cx >= x && cx < x + pw2 && cy >= y && cy < y + rh * opts.len() {
                    let v = opts[(cy - y) / rh].0.clone();
                    w.doc.inputs[k].value = v;
                }
            }
            return true;
        }
    }
    let Some(web) = &win.web else { return false };
    let (by, bh, btns, addr, (px0, py0, pw, ph)) = web_geo(win);
    if cy >= by && cy < by + bh {
        if let Some(k) = btns.iter().position(|&bx| cx >= bx && cx < bx + bh) {
            match k {
                0 => web_back(win),
                1 => {
                    if web.loading.is_some() {
                        let w = win.web.as_mut().unwrap();
                        w.loading = None;
                        w.status = String::from("Töxtatildi");
                    } else {
                        let u = web.url.clone();
                        web_go(win, &u, false);
                    }
                }
                _ => web_go(win, "zamin://bosh", true),
            }
            return true;
        }
    }
    if in_rect(addr, cx, cy) {
        let w = win.web.as_mut().unwrap();
        w.editing = true;
        w.edit = w.url.clone();
        return true;
    }
    if in_rect((px0, py0, pw, ph), cx, cy) {
        let (items, _, total) = web_layout(web, pw.saturating_sub(body()));
        let sc = web.scroll.min(total.saturating_sub(ph));
        let (lx, ly) = (cx - px0, cy - py0 + sc);
        // Nuqta ostidagilar ichidan bosiladiganini (havola / JS) afzal köramiz
        let under: Vec<&WItem> = items.iter().filter(|it| lx >= it.x && lx < it.x + it.w && ly + it.h / 5 >= it.y && ly < it.y + it.h * 7 / 5).collect();
        let hit = under
            .iter()
            .find(|it| it.input != NO_EL || it.link.is_some() || it.el != NO_EL)
            .or(under.first())
            .map(|it| (it.el, it.link.clone(), it.input));
        let det_hit = under.iter().find(|it| it.det != NO_EL).map(|it| it.det);
        let range_t = under.iter().find(|it| it.input != NO_EL).map(|it| (lx.saturating_sub(it.x)) as f32 / it.w.max(1) as f32);
        let base = match &web.doc.base {
            Some(b) => crate::net::http::join(&web.url, b),
            None => web.url.clone(),
        };
        let w = win.web.as_mut().unwrap();
        w.editing = false;
        w.focus = None;
        // <summary> — <details> ni oçiş/yopiş
        if let Some(d) = det_hit {
            w.doc.toggle_details(d);
            return true;
        }
        // <label>: for="id" yoki içidagi maydon — öşa maydon bosilgandek
        let mut hit = hit;
        if let Some((el, link, NO_EL)) = hit.clone() {
            let mut cur = el;
            let mut n = 0;
            while cur != NO_EL && n < 16 {
                let Some(info) = w.doc.elems.get(cur as usize) else { break };
                if info.tag == "label" {
                    let target = if !info.label_for.is_empty() {
                        w.doc.find_id(&info.label_for).and_then(|e| w.doc.input_of(e))
                    } else {
                        let le = cur;
                        (0..w.doc.inputs.len()).find(|&k| w.doc.within(w.doc.inputs[k].el, le) && w.doc.inputs[k].el != NO_EL)
                    };
                    if let Some(k) = target {
                        hit = Some((w.doc.inputs[k].el, link, k as u16));
                    }
                    break;
                }
                cur = info.parent;
                n += 1;
            }
        }
        if let Some((el, _, inp)) = hit.clone().filter(|h| h.2 != NO_EL) {
            let (kind, form) = (w.doc.inputs[inp as usize].kind, w.doc.inputs[inp as usize].form);
            match kind {
                2 if w.doc.inputs[inp as usize].radio => {
                    // radio: guruhda (bir xil name va forma) faqat bittasi
                    let (name, f) = (w.doc.inputs[inp as usize].name.clone(), w.doc.inputs[inp as usize].form);
                    for (k, i) in w.doc.inputs.iter_mut().enumerate() {
                        if i.radio && i.name == name && i.form == f {
                            i.checked = k == inp as usize;
                        }
                    }
                }
                8 => {
                    let i = &mut w.doc.inputs[inp as usize];
                    let t = range_t.unwrap_or(0.5).clamp(0.0, 1.0);
                    let v = i.min + (i.max - i.min) * t;
                    i.value = alloc::format!("{}", (v + 0.5) as i64);
                }
                7 => {
                    // rang: tayyor palitra böyiça aylanadi
                    const PAL: [&str; 8] = ["#000000", "#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#007aff", "#af52de", "#ffffff"];
                    let i = &mut w.doc.inputs[inp as usize];
                    let k = PAL.iter().position(|c| c.eq_ignore_ascii_case(&i.value)).map(|k| (k + 1) % PAL.len()).unwrap_or(0);
                    i.value = String::from(PAL[k]);
                }
                9 => return true,
                0 | 3 => {
                    w.focus = Some(inp as usize);
                    return true;
                }
                6 => {
                    w.select_open = Some(inp as usize);
                    return true;
                }
                2 => {
                    let c = &mut w.doc.inputs[inp as usize].checked;
                    *c = !*c;
                }
                _ => {}
            }
            // JS bosiş işlovçisi (bölsa)
            if el != NO_EL {
                if let Some(eng) = w.js.as_mut() {
                    if eng.has_handler(el as usize, &w.doc) {
                        eng.click(el as usize, &mut w.doc, crate::timer::uptime_ms());
                        for a in eng.alerts.drain(..) {
                            notify("Sahifa xabari", &a, 0x2f88ff);
                        }
                    }
                }
            }
            if (kind == 1 || kind == 5) && form != NO_EL {
                web_submit(win, form as usize, Some(inp as usize));
            }
            return true;
        }
        if let Some((el, link, _)) = hit {
            // JS hodisasi (onclick / addEventListener)
            if el != NO_EL {
                if let Some(eng) = w.js.as_mut() {
                    if eng.has_handler(el as usize, &w.doc) {
                        eng.click(el as usize, &mut w.doc, crate::timer::uptime_ms());
                        for a in eng.alerts.drain(..) {
                            notify("Sahifa xabari", &a, 0x2f88ff);
                        }
                        return true;
                    }
                }
            }
            if let Some(href) = link {
                if href.starts_with("javascript:") {
                    let code = String::from(&href[11..]);
                    if let Some(eng) = w.js.as_mut() {
                        let _ = eng.run(&code, &mut w.doc, crate::timer::uptime_ms());
                    }
                    return true;
                }
                if let Some(frag) = href.strip_prefix('#') {
                    let frag = String::from(frag);
                    web_anchor(win, &frag);
                    return true;
                }
                if let Some(m) = href.strip_prefix("zamin-play:") {
                    let target = if m.starts_with("data:") { String::from(m) } else { crate::net::http::join(&base, m) };
                    web_media_play(&target);
                    return true;
                }
                let target = crate::net::http::join(&base, &href);
                // Şu sahifaning yakori — qayta yuklamasdan aylantiramiz
                let cur = win.web.as_ref().map(|w| String::from(w.url.split('#').next().unwrap_or(""))).unwrap_or_default();
                if let Some((page, frag)) = target.split_once('#') {
                    if page == cur {
                        let frag = String::from(frag);
                        web_anchor(win, &frag);
                        return true;
                    }
                }
                web_go(win, &target, true);
            }
        }
        return true;
    }
    false
}

/// Hozir çalinayotgan <audio> manbasi (qayta bosilsa — töxtatiladi).
static MEDIA_NOW: crate::sync::SpinLock<String> = crate::sync::SpinLock::new(String::new());

/// <audio> ijrosi: faylni oladi, dekodlaydi (WAV · AIFF · FLAC · MP3 · OGG), virtio-sound ga beradi.
fn web_media_play(url: &str) {
    {
        let mut now = MEDIA_NOW.lock();
        if *now == url && crate::audio::sound::playing() {
            crate::audio::sound::stop();
            now.clear();
            notify("Audio", "Ijro töxtatildi", 0x8e8e93);
            return;
        }
    }
    let bytes = if let Some(rest) = url.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',').unwrap_or((rest, ""));
        Ok(if meta.ends_with(";base64") { base64_decode(data) } else { crate::js::url_decode(data).into_bytes() })
    } else {
        web_bytes(url, None).map(|(d, _, _)| d)
    };
    let d = match bytes {
        Ok(d) if d.len() <= 32 * 1024 * 1024 => d,
        Ok(_) => return notify("Audio", "Fayl juda katta", 0xff9500),
        Err(e) => return notify("Audio yuklanmadi", &e, 0xff3b30),
    };
    let name = url.rsplit('/').next().unwrap_or("media").split('?').next().unwrap_or("media");
    let Some(clip) = crate::audio::decode(name, &d) else {
        return notify("Audio", "Format qöllanmaydi (WAV · AIFF · FLAC · MP3 · OGG)", 0xff9500);
    };
    let secs = clip.duration_ms() / 1000;
    if !crate::audio::sound::present() {
        notify("Audio", &alloc::format!("{} · {}:{:02} — övoz qurilmasi yöq (virtio-sound)", name, secs / 60, secs % 60), 0xff9500);
        return;
    }
    crate::audio::sound::play(&clip.samples, clip.rate, clip.channels, 0);
    *MEDIA_NOW.lock() = String::from(url);
    notify("Ijro etilmoqda", &alloc::format!("♪ {} · {} Hz · {}:{:02}", name, clip.rate, secs / 60, secs % 60), 0x2fbf5f);
}

/// Formani yuboradi: JS submit işlovçilari → GET (so'rov satri) yoki POST (tana).
fn web_submit(win: &mut Window, f: usize, submitter: Option<usize>) {
    let Some(w) = win.web.as_mut() else { return };
    let Some(form) = w.doc.forms.get(f).cloned() else { return };
    if form.el != NO_EL {
        if let Some(eng) = w.js.as_mut() {
            let prevented = eng.submit(form.el as usize, &mut w.doc, crate::timer::uptime_ms());
            for a in eng.alerts.drain(..) {
                notify("Sahifa xabari", &a, 0x2f88ff);
            }
            if prevented {
                return; // sahifa özi işladi (preventDefault)
            }
        }
    }
    let enc = |x: &str| crate::js::url_encode(x, false).replace("%20", "+");
    let mut q = String::new();
    for (k, i) in w.doc.inputs.iter().enumerate().filter(|(_, i)| i.form as usize == f && !i.name.is_empty()) {
        let v = match i.kind {
            0 | 3 | 4 | 6 => Some(i.value.clone()),
            2 if i.checked => Some(if i.value.is_empty() { String::from("on") } else { i.value.clone() }),
            1 | 5 if Some(k) == submitter => Some(i.value.clone()),
            _ => None,
        };
        if let Some(v) = v {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str(&enc(&i.name));
            q.push('=');
            q.push_str(&enc(&v));
        }
    }
    let base = w.url.clone();
    let action = if form.action.is_empty() { base.clone() } else { crate::net::http::join(&base, &form.action) };
    if form.method == "post" {
        web_go_ex(win, &action, true, Some(q));
    } else {
        let a = action.split('?').next().unwrap_or(&action).split('#').next().unwrap_or("");
        let target = alloc::format!("{}?{}", a, q);
        web_go(win, &target, true);
    }
}

fn web_key(win: &mut Window, code: u16, shift: bool) {
    // Forma maydoni fokusda: matn kiritiş
    if let Some(k) = win.web.as_ref().and_then(|w| w.focus) {
        let w = win.web.as_mut().unwrap();
        let n = w.doc.inputs.len();
        match code {
            28 => {
                let form = w.doc.inputs.get(k).map(|i| i.form).unwrap_or(NO_EL);
                w.focus = None;
                if form != NO_EL {
                    web_submit(win, form as usize, None);
                }
            }
            14 => {
                if let Some(i) = w.doc.inputs.get_mut(k) {
                    i.value.pop();
                }
            }
            15 => {
                // Tab — keyingi matn maydoni
                w.focus = (1..=n).map(|d| (k + d) % n).find(|&j| matches!(w.doc.inputs[j].kind, 0 | 3));
            }
            _ => {
                if let Some(c) = keycode_char(code, shift) {
                    if let Some(i) = w.doc.inputs.get_mut(k) {
                        if i.value.chars().count() < 500 {
                            i.value.push(c);
                        }
                    }
                }
            }
        }
        return;
    }
    let Some(web) = win.web.as_mut() else { return };
    if web.editing {
        match code {
            28 => {
                let u = web.edit.clone();
                web_go(win, &u, true);
            }
            14 => {
                web.edit.pop();
            }
            _ => {
                if let Some(c) = keycode_char(code, shift) {
                    if web.edit.len() < 300 {
                        web.edit.push(c);
                    }
                }
            }
        }
        return;
    }
    let (_, _, _, _, (_, _, _, ph)) = web_geo(win);
    let step = (body() * 2) as isize;
    match code {
        103 => web_scroll(win, -step),
        108 => web_scroll(win, step),
        104 => web_scroll(win, -(ph as isize - step)),
        109 | 57 => web_scroll(win, ph as isize - step),
        102 => web_scroll(win, -(1 << 30)),
        107 => web_scroll(win, 1 << 30),
        14 => web_back(win),
        63 => {
            let u = win.web.as_ref().map(|w| w.url.clone()).unwrap_or_default();
            web_go(win, &u, false);
        }
        88 => {
            if let Some(w) = win.web.as_mut() {
                w.console = !w.console;
            }
        }
        _ => {}
    }
}

// ---------------- Chizma (rasm chiziş) ----------------

const PAINT_COLORS: [Color; 10] = [0x111111, 0xffffff, 0xff3b30, 0xff9500, 0xffcc00, 0x34c759, 0x00c7be, 0x007aff, 0xaf52de, 0x8e5a35];
const PAINT_SIZES: [usize; 3] = [2, 5, 11];

fn paint_new(win: &mut Window) {
    let (w, h) = (720, 450);
    win.img = Some(crate::image::Image { w, h, px: alloc::vec![0xffffff; w * h] });
    win.frame = 0;
    win.play_pos = 1;
    win.tab = 0;
    win.dirty = false;
}

/// Chizma geometriyasi: asboblar qatori y, balandligi; tuval ekrandagi töğriburçagi.
fn paint_geo(win: &Window) -> (usize, usize, (usize, usize, usize, usize)) {
    let u = body();
    let ty = win.y + title_h();
    let th = u * 5 / 2;
    let (cw, ch) = win.img.as_ref().map(|i| (i.w, i.h)).unwrap_or((1, 1));
    let ax = win.x + u / 2;
    let ay = ty + th + u / 3;
    let aw = win.w.saturating_sub(u);
    let ah = (win.y + win.h).saturating_sub(ay + u / 2);
    let (w, h) = if aw * ch <= ah * cw { (aw, aw * ch / cw) } else { (ah * cw / ch, ah) };
    (ty, th, (ax + (aw - w) / 2, ay + (ah - h) / 2, w.max(1), h.max(1)))
}

/// Asboblar: 0..2 — qalam/öçirgiç/…, ranglar, ölçamlar, tugmalar (x, y, w, h, amal).
fn paint_tools(win: &Window) -> Vec<((usize, usize, usize, usize), u8, usize)> {
    let u = body();
    let (ty, th, _) = paint_geo(win);
    let s = u * 3 / 2;
    let y = ty + (th - s) / 2;
    let mut v = Vec::new();
    let mut x = win.x + u / 2;
    for k in 0..2 {
        v.push(((x, y, s, s), 0u8, k));
        x += s + u / 4;
    }
    x += u / 2;
    for k in 0..PAINT_SIZES.len() {
        v.push(((x, y, s, s), 1, k));
        x += s + u / 6;
    }
    x += u / 2;
    let cs = s * 3 / 4;
    for k in 0..PAINT_COLORS.len() {
        v.push(((x, y + (s - cs) / 2, cs, cs), 2, k));
        x += cs + u / 5;
    }
    let bw = gfx::text_width("Saqlaş", small()) + u;
    let xs = (win.x + win.w).saturating_sub(u / 2 + bw);
    v.push(((xs, y, bw, s), 3, 1));
    let bw0 = gfx::text_width("Tozalaş", small()) + u;
    v.push(((xs.saturating_sub(bw0 + u / 3), y, bw0, s), 3, 0));
    v
}

fn paint_stamp(win: &mut Window, x: isize, y: isize) {
    let r = PAINT_SIZES[win.play_pos as usize % PAINT_SIZES.len()] as isize;
    let col = if win.tab == 1 { 0xffffff } else { PAINT_COLORS[win.frame % PAINT_COLORS.len()] };
    let Some(img) = win.img.as_mut() else { return };
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && py >= 0 && (px as usize) < img.w && (py as usize) < img.h {
                    img.px[py as usize * img.w + px as usize] = col;
                }
            }
        }
    }
    win.dirty = true;
}

fn paint_map(win: &Window, cx: usize, cy: usize) -> (isize, isize) {
    let (_, _, (x, y, w, h)) = paint_geo(win);
    let (cw, ch) = win.img.as_ref().map(|i| (i.w, i.h)).unwrap_or((1, 1));
    (((cx as isize - x as isize) * cw as isize / w as isize), ((cy as isize - y as isize) * ch as isize / h as isize))
}

/// Oxirgi nuqtadan joriy nuqtagaça chiziq.
fn paint_drag(win: &mut Window, cx: usize, cy: usize) {
    let Some((lx, ly)) = win.pen else { return };
    let (x, y) = paint_map(win, cx, cy);
    let (lx, ly) = (lx as isize, ly as isize);
    let r = PAINT_SIZES[win.play_pos as usize % PAINT_SIZES.len()] as isize;
    let d = ((x - lx).abs().max((y - ly).abs())).max(1);
    let step = (r / 2).max(1);
    let n = d / step + 1;
    for k in 1..=n {
        paint_stamp(win, lx + (x - lx) * k / n, ly + (y - ly) * k / n);
    }
    win.pen = Some((x.max(0) as usize, y.max(0) as usize));
}

fn paint_save(win: &mut Window) {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    let Some(img) = win.img.as_ref() else { return };
    let png = crate::image::png_enc::encode(img.w, img.h, &img.px);
    let t = time::now();
    let dir = if fs::list("~/Rasmlar").is_ok() { "~/Rasmlar" } else { "~" };
    let path = unique_dest(&path_join(dir, &alloc::format!("chizma-{:02}{:02}{:02}.png", t.hour, t.minute, t.second)));
    match fs::open(&path, OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE).and_then(|id| fs::write(id, Some(0), &png)) {
        Ok(_) => {
            let _ = fs::commit();
            win.dirty = false;
            notify("Chizma saqlandi", &alloc::format!("{} · {} KB", path.rsplit('/').next().unwrap_or(""), png.len() / 1024), 0x2fbf5f);
        }
        Err(e) => notify("Saqlab bölmadi", &alloc::format!("{:?}", e), 0xff453a),
    }
}

fn paint_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    for (r, kind, k) in paint_tools(win) {
        if in_rect(r, cx, cy) {
            match kind {
                0 => win.tab = k,
                1 => win.play_pos = k as u32,
                2 => {
                    win.frame = k;
                    win.tab = 0;
                }
                _ => {
                    if k == 0 {
                        let (w, h) = win.img.as_ref().map(|i| (i.w, i.h)).unwrap_or((720, 450));
                        win.img = Some(crate::image::Image { w, h, px: alloc::vec![0xffffff; w * h] });
                        win.dirty = false;
                    } else {
                        paint_save(win);
                    }
                }
            }
            return true;
        }
    }
    let (_, _, cr) = paint_geo(win);
    if in_rect(cr, cx, cy) {
        let (x, y) = paint_map(win, cx, cy);
        paint_stamp(win, x, y);
        win.pen = Some((x.max(0) as usize, y.max(0) as usize));
        return true;
    }
    false
}

fn draw_paint(win: &Window) {
    let u = body();
    let (ty, th, (x, y, w, h)) = paint_geo(win);
    fill_rect(win.x + 1, ty, win.w.saturating_sub(2), th, 0x1f2636);
    fill_rect(win.x + 1, ty + th, win.w.saturating_sub(2), 1, 0x2a3346);
    for (r, kind, k) in paint_tools(win) {
        let (bx, by, bw, bh) = r;
        match kind {
            0 => {
                let on = win.tab == k;
                fill_round_rect_a(bx, by, bw, bh, bh / 4, if on { accent() } else { 0xffffff }, if on { 230 } else { 16 });
                let (c, m) = ((bx + bw / 2) as f32, (by + bh / 2) as f32);
                let d = bw as f32 * 0.28;
                if k == 0 {
                    seg(c - d, m + d, c + d, m - d, (bw as f32 / 9.0).max(1.5), OQ);
                    gfx::fill_circle((c - d) as usize, (m + d) as usize, (bw / 10).max(2), PAINT_COLORS[win.frame % 10]);
                } else {
                    gfx::fill_round_rect((c - d) as usize, (m - d * 0.6) as usize, (d * 2.0) as usize, (d * 1.2) as usize, 3, 0xff9fb4);
                    fill_rect((c - d) as usize, (m + d * 0.2) as usize, (d * 2.0) as usize, (d * 0.4) as usize, 0xe0e6ee);
                }
            }
            1 => {
                let on = win.play_pos as usize == k;
                fill_round_rect_a(bx, by, bw, bh, bh / 4, 0xffffff, if on { 60 } else { 12 });
                gfx::fill_circle(bx + bw / 2, by + bh / 2, (PAINT_SIZES[k] * bw / 30).clamp(2, bw / 2 - 2), 0xe7edf6);
            }
            2 => {
                let on = win.frame == k && win.tab == 0;
                if on {
                    gfx::fill_circle(bx + bw / 2, by + bh / 2, bw / 2 + 3, OQ);
                }
                gfx::fill_circle(bx + bw / 2, by + bh / 2, bw / 2, PAINT_COLORS[k]);
                gfx::fill_circle(bx + bw / 2, by + bh / 2, bw / 2, PAINT_COLORS[k]);
            }
            _ => {
                let (lbl, col) = if k == 0 { ("Tozalaş", 0x3a4458) } else { ("Saqlaş", if win.dirty { 0x30c85a } else { 0x2a6b3d }) };
                gfx::fill_round_rect(bx, by, bw, bh, bh / 3, col);
                gfx::draw_text_centered(bx + bw / 2, by + (bh - small()) / 2, lbl, OQ, small());
            }
        }
    }
    gfx::shadow(x, y, w, h, 2, 6);
    if let Some(img) = &win.img {
        gfx::blit_scaled(x, y, w, h, img.w, img.h, &img.px);
    }
    let _ = u;
}

// ---------------- Soat: sekundomer va taymer ----------------

const CLOCK_TABS: [&str; 3] = ["Soat", "Sekundomer", "Taymer"];

fn clock_tabs(win: &Window) -> Vec<(usize, usize, usize, usize)> {
    let u = body();
    let tw = (win.w.saturating_sub(u * 2)).min(u * 18);
    let x0 = win.x + (win.w - tw) / 2;
    let y = win.y + title_h() + u / 2;
    let h = u * 3 / 2;
    (0..3).map(|k| (x0 + k * tw / 3, y, tw / 3, h)).collect()
}

/// Sekundomer/taymer tugmalari: [chap, öng] (x, y, w, h).
fn clock_btns(win: &Window) -> [(usize, usize, usize, usize); 2] {
    let u = body();
    let bw = u * 6;
    let bh = u * 2;
    let y = (win.y + win.h).saturating_sub(u + bh);
    let cx = win.x + win.w / 2;
    [(cx - bw - u / 2, y, bw, bh), (cx + u / 2, y, bw, bh)]
}

fn sw_elapsed(win: &Window) -> u64 {
    win.play_pos as u64 + if win.playing { crate::timer::uptime_ms().saturating_sub(win.play_t) } else { 0 }
}

fn clock_click(win: &mut Window, cx: usize, cy: usize) -> bool {
    if let Some(k) = clock_tabs(win).iter().position(|&r| in_rect(r, cx, cy)) {
        win.tab = k;
        return true;
    }
    let [l, r] = clock_btns(win);
    let now = crate::timer::uptime_ms();
    match win.tab {
        1 => {
            if in_rect(l, cx, cy) {
                win.playing = false;
                win.play_pos = 0;
                win.lines.clear();
                return true;
            }
            if in_rect(r, cx, cy) {
                if win.playing {
                    win.play_pos = sw_elapsed(win) as u32;
                    win.playing = false;
                } else {
                    win.play_t = now;
                    win.playing = true;
                }
                return true;
            }
        }
        2 => {
            if in_rect(l, cx, cy) {
                if win.frame_ms != 0 {
                    win.frame_ms = 0; // bekor
                } else {
                    win.frame = match win.frame {
                        60 => 180,
                        180 => 300,
                        300 => 600,
                        600 => 30,
                        _ => 60,
                    };
                }
                return true;
            }
            if in_rect(r, cx, cy) {
                if win.frame_ms == 0 {
                    win.frame_ms = now + win.frame as u64 * 1000;
                }
                return true;
            }
        }
        _ => {}
    }
    false
}

fn fmt_ms(ms: u64, cs: bool) -> String {
    let (m, s) = (ms / 60000, (ms / 1000) % 60);
    if cs { alloc::format!("{:02}:{:02}.{:02}", m, s, (ms / 10) % 100) } else { alloc::format!("{:02}:{:02}", m, s) }
}

fn draw_clock_tabs(win: &Window) {
    let u = body();
    let tabs = clock_tabs(win);
    let (x0, y, _, h) = tabs[0];
    let tw: usize = tabs.iter().map(|t| t.2).sum();
    gfx::fill_round_rect(x0, y, tw, h, h / 2, 0x10151f);
    for (k, &(x, y, w, h)) in tabs.iter().enumerate() {
        if k == win.tab {
            gfx::fill_round_rect(x + 2, y + 2, w - 4, h - 4, (h - 4) / 2, 0x3a4458);
        }
        gfx::draw_text_centered(x + w / 2, y + (h - small() * 9 / 10) / 2, CLOCK_TABS[k], if k == win.tab { OQ } else { 0x8ea0b6 }, small() * 9 / 10);
    }
    let _ = u;
}

fn draw_clock_extra(win: &Window) {
    let u = body();
    let top = clock_tabs(win)[0].1 + u * 2;
    let [l, r] = clock_btns(win);
    let mid = (top + l.1) / 2;
    let big = (u * 3).min(win.w / 6);
    let btn = |rc: (usize, usize, usize, usize), lbl: &str, col: Color| {
        gfx::fill_round_rect(rc.0, rc.1, rc.2, rc.3, rc.3 / 2, col);
        gfx::draw_text_centered(rc.0 + rc.2 / 2, rc.1 + (rc.3 - small()) / 2, lbl, OQ, small());
    };
    if win.tab == 1 {
        let t = fmt_ms(sw_elapsed(win), true);
        gfx::draw_text_centered(win.x + win.w / 2, mid.saturating_sub(big / 2), &t, OQ, big);
        btn(l, "Qayta", 0x3a4458);
        if win.playing { btn(r, "Töxtatiş", 0xd83b33) } else { btn(r, "Boşlaş", 0x2fbf5f) }
    } else {
        let now = crate::timer::uptime_ms();
        let left = if win.frame_ms != 0 { win.frame_ms.saturating_sub(now) } else { win.frame as u64 * 1000 };
        let total = (win.frame as u64 * 1000).max(1);
        let cx = win.x + win.w / 2;
        let rr = ((l.1 - top) / 2).saturating_sub(u / 2).max(u * 2);
        let pct = if win.frame_ms != 0 { (left * 100 / total) as u32 } else { 100 };
        ring(cx, top + (l.1 - top) / 2, rr, (u / 2).max(6), pct, 0x262f40, 0xff9f0a);
        let t = fmt_ms(left + 999, false);
        let tb = big * 3 / 4;
        gfx::draw_text_centered(cx, top + (l.1 - top) / 2 - tb / 2, &t, OQ, tb);
        if win.frame_ms != 0 {
            btn(l, "Bekor", 0x3a4458);
            btn(r, "Ketyapti…", 0x7a5200);
        } else {
            btn(l, "Vaqtni almaşt.", 0x3a4458);
            btn(r, "Boşlaş", 0xff9f0a);
        }
    }
}

// ---------------- Dok kontekst menyusi ----------------

fn dock_ctx(windows: &[Window], cx: usize, cy: usize) -> Option<Ctx> {
    let (sw, sh) = gfx::size();
    let i = (0..LAUNCHERS.len()).find(|&i| {
        let (ix, iy, isz) = dock_icon_rect(sw, sh, i);
        in_rect((ix, iy, isz, isz), cx, cy)
    })?;
    let l = &LAUNCHERS[i];
    let s = |t: &str| String::from(t);
    let open = windows.iter().filter(|w| win_of_launcher(w, l.kind)).count();
    let mut items: Vec<(String, CAct)> = alloc::vec![(String::from(l.label), CAct::Sep), (s("Sep"), CAct::Sep)];
    items.clear();
    if open > 0 {
        items.push((s("Körsatiş"), CAct::Focus(l.kind)));
    }
    items.push((s(if open > 0 { "Yangi oyna" } else { "Oçiş" }), CAct::App(l.kind)));
    if l.kind == Kind::Trash && trash_count() > 0 {
        items.push((s("Sep"), CAct::Sep));
        items.push((s("Savatni böşatiş"), CAct::EmptyTrash));
    }
    if open > 0 {
        items.push((s("Sep"), CAct::Sep));
        items.push((alloc::format!("Yopiş ({})", open), CAct::CloseApp(l.kind)));
    }
    let (ix, _, isz) = dock_icon_rect(sw, sh, i);
    Some(Ctx { x: ix + isz / 2, y: usize::MAX, items })
}
