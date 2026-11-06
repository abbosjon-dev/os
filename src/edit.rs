//! Matn muharriri uçun sof mantiq: bekor qiliş/qaytariş tarixi (EditHistory)
//! qidiruv (find_all) va matn tanlaş (range_text / delete_range / insert_text / replace_sel). GUI ga bogʻliq emas — self-test bilan tekşiriladi.

use alloc::string::String;
use alloc::vec::Vec;

/// Tarixdagi eng köp qadam.
pub const MAX_STEPS: usize = 200;

/// Tahrir turi — guruhlaş uçun.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EditKind {
    /// Bitta belgi yozildi (`ws` — boşliq/tab).
    Type { ws: bool },
    /// Enter (yangi qator).
    Enter,
    /// Backspace/Delete.
    Delete,
    /// Joylaş va boşqa yirik özgarişlar.
    Paste,
}

/// Matn holati: qatorlar va kursor.
#[derive(Clone, PartialEq, Debug)]
pub struct Snapshot {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
}

/// Bekor qiliş/qaytariş steklari. Har qadam — tahrirdan OLDINGI holat.
pub struct EditHistory {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Joriy ochiq guruh: oxirgi yozilgan belgi boşliq edimi.
    /// `None` — guruh yöq (keyingi tahrir yangi qadam boşlaydi).
    open: Option<bool>,
}

impl EditHistory {
    pub const fn new() -> Self {
        EditHistory { undo: Vec::new(), redo: Vec::new(), open: None }
    }

    /// Tahrirdan OLDIN çaqiriladi: kerak bölsa joriy holatni yangi qadam sifatida saqlaydi.
    /// Ketma-ket yozilgan söz (va undan keyingi boşliqlar) — bitta qadam;
    /// Enter, öçiriş, joylaş — har biri alohida qadam. Redo steki tozalanadi.
    pub fn record(&mut self, kind: EditKind, lines: &[String], row: usize, col: usize) {
        self.redo.clear();
        let merge = match (kind, self.open) {
            // söz davom etyapti yoki söz ortidan boşliq — öşa qadam;
            // boşliqdan keyingi harf — yangi söz, yangi qadam
            (EditKind::Type { ws }, Some(prev_ws)) => ws || !prev_ws,
            _ => false,
        };
        if !merge {
            self.undo.push(Snapshot { lines: lines.to_vec(), row, col });
            if self.undo.len() > MAX_STEPS {
                self.undo.remove(0);
            }
        }
        self.open = match kind {
            EditKind::Type { ws } => Some(ws),
            _ => None,
        };
    }

    /// Kursor siljidi (strelka, siçqon) — joriy söz guruhi yopiladi.
    pub fn break_group(&mut self) {
        self.open = None;
    }

    /// Bekor qiliş: joriy holatni redo ga qöyib, oldingisini qaytaradi.
    pub fn undo(&mut self, cur: Snapshot) -> Option<Snapshot> {
        self.open = None;
        let prev = self.undo.pop()?;
        self.redo.push(cur);
        Some(prev)
    }

    /// Qaytariş: joriy holatni undo ga qöyib, keyingisini qaytaradi.
    pub fn redo(&mut self, cur: Snapshot) -> Option<Snapshot> {
        self.open = None;
        let next = self.redo.pop()?;
        self.undo.push(cur);
        Some(next)
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

/// Matndagi barça mosliklar: (qator, ustun) — ustun BELGILARDA (UTF-8 bayt emas).
/// Böş so'rov — moslik yöq. Bir qatorda ustma-ust tuşmaydigan mosliklar.
pub fn find_all(text: &str, query: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    for (row, line) in text.split('\n').enumerate() {
        let mut from = 0;
        while let Some(i) = line[from..].find(query) {
            let b = from + i;
            out.push((row, line[..b].chars().count()));
            from = b + query.len();
        }
    }
    out
}

/// Qatorlar ustida find_all (muharrir uçun qulay).
pub fn find_in_lines(lines: &[String], query: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    for (row, line) in lines.iter().enumerate() {
        for (_, col) in find_all(line, query) {
            out.push((row, col));
        }
    }
    out
}

// --- Matn tanlaş (selection) ---

/// Matndagi nuqta: (qator, ustun) — ustun BELGILARDA.
pub type Pos = (usize, usize);

fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

/// Ikki nuqtani tartiblaydi: (boşi, oxiri).
pub fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Nuqtani matn çegarasiga keltiradi.
pub fn clamp_pos(lines: &[String], p: Pos) -> Pos {
    let r = p.0.min(lines.len().saturating_sub(1));
    let len = lines.get(r).map_or(0, |l| l.chars().count());
    (r, p.1.min(len))
}

/// [a, b) oraligʻidagi matn (qatorlar "\n" bilan).
pub fn range_text(lines: &[String], a: Pos, b: Pos) -> String {
    let (a, b) = ordered(clamp_pos(lines, a), clamp_pos(lines, b));
    if a.0 == b.0 {
        let l = &lines[a.0];
        return String::from(&l[byte_at(l, a.1)..byte_at(l, b.1)]);
    }
    let mut s = String::new();
    let first = &lines[a.0];
    s.push_str(&first[byte_at(first, a.1)..]);
    for l in &lines[a.0 + 1..b.0] {
        s.push('\n');
        s.push_str(l);
    }
    s.push('\n');
    let last = &lines[b.0];
    s.push_str(&last[..byte_at(last, b.1)]);
    s
}

/// [a, b) oraligʻini öçiradi; kursor örni (boşi) qaytadi.
pub fn delete_range(lines: &mut Vec<String>, a: Pos, b: Pos) -> Pos {
    let (a, b) = ordered(clamp_pos(lines, a), clamp_pos(lines, b));
    let tail = {
        let last = &lines[b.0];
        String::from(&last[byte_at(last, b.1)..])
    };
    let ba = byte_at(&lines[a.0], a.1);
    lines[a.0].truncate(ba);
    lines[a.0].push_str(&tail);
    if b.0 > a.0 {
        lines.drain(a.0 + 1..=b.0);
    }
    a
}

/// `s` ni (köp qatorli bölişi mumkin) p nuqtaga qöyadi; yangi kursor örni qaytadi.
pub fn insert_text(lines: &mut Vec<String>, p: Pos, s: &str) -> Pos {
    let p = clamp_pos(lines, p);
    let b = byte_at(&lines[p.0], p.1);
    let tail = lines[p.0].split_off(b);
    let mut row = p.0;
    let mut parts = s.split('\n');
    if let Some(first) = parts.next() {
        lines[row].push_str(first.trim_end_matches('\r'));
    }
    for part in parts {
        row += 1;
        lines.insert(row, String::from(part.trim_end_matches('\r')));
    }
    let col = lines[row].chars().count();
    lines[row].push_str(&tail);
    (row, col)
}

/// Kursor harakati (strelka/Home/End/PgUp/PgDn) tanlaşga ta'siri: `before` — harakatdan
/// oldingi, `after` — keyingi kursor. Shift bilan — langar (yöq bölsa — `before`) saqlanadi
/// va tanlaş kengayadi/torayadi; kursor langarga qaytsa tanlaş bekor böladi. Shift siz —
/// tanlaş bekor qilinadi.
pub fn nav_select(anchor: &mut Option<Pos>, before: Pos, after: Pos, shift: bool) {
    *anchor = if shift { Some(anchor.unwrap_or(before)).filter(|&a| a != after) } else { None };
}

/// Tanlangan matnni (bölsa) `s` bilan almaştiradi — tarixga BITTA qadam bölib yoziladi
/// (`kind`: yoziş, joylaş, kesiş/öçiriş). `anchor` — tanlaş langari (keyin None).
/// Tanlaş yöq va `s` böş bölsa — hech narsa qilinmaydi (false).
pub fn replace_sel(h: &mut EditHistory, kind: EditKind, lines: &mut Vec<String>, cur: &mut Pos, anchor: &mut Option<Pos>, s: &str) -> bool {
    let sel = anchor.take().filter(|a| *a != *cur);
    if sel.is_none() && s.is_empty() {
        return false;
    }
    // tanlaşni almaştiriş — har doim alohida qadam (yoziş guruhiga qöşilmaydi)
    if sel.is_some() {
        h.break_group();
    }
    h.record(kind, lines, cur.0, cur.1);
    let mut p = *cur;
    if let Some(a) = sel {
        p = delete_range(lines, a, *cur);
    }
    if sel.is_some() && matches!(kind, EditKind::Type { .. }) {
        h.break_group();
    }
    *cur = insert_text(lines, p, s);
    true
}

/// Self-test: yoziş→undo→redo matnni aynan tiklaydi; UTF-8 qidiruvi.
pub fn self_test() -> Result<usize, &'static str> {
    use alloc::string::ToString;
    let mut n = 0;
    let mut h = EditHistory::new();
    let mut lines: Vec<String> = alloc::vec![String::new()];
    let (mut row, mut col) = (0usize, 0usize);
    // "salom dunyo" ni belgima-belgi yozamiz
    for ch in "salom dunyo".chars() {
        h.record(EditKind::Type { ws: ch == ' ' }, &lines, row, col);
        lines[row].push(ch);
        col += 1;
    }
    if h.undo_len() != 2 {
        return Err("ikki söz ikki qadam bölişi kerak");
    }
    n += 1;
    // Enter — alohida qadam
    h.record(EditKind::Enter, &lines, row, col);
    lines.push(String::from("öçir"));
    row = 1;
    col = 4;
    if h.undo_len() != 3 {
        return Err("Enter alohida qadam emas");
    }
    n += 1;
    let full = Snapshot { lines: lines.clone(), row, col };
    // Uç marta undo — böş holatga
    let mut cur = full.clone();
    for _ in 0..3 {
        cur = h.undo(cur).ok_or("undo yöq")?;
    }
    if cur.lines != alloc::vec![String::new()] || cur.row != 0 || cur.col != 0 {
        return Err("undo boşlangʻiç holatni tiklamadi");
    }
    if h.undo(cur.clone()).is_some() {
        return Err("undo steki böşamadi");
    }
    n += 1;
    // Undo birinçi söz: "salom " kursor 6 da
    let mut c2 = h.redo(cur).ok_or("redo yöq")?;
    if c2.lines[0] != "salom " || c2.col != 6 {
        return Err("redo 1-söz notoʻgʻri");
    }
    for _ in 0..2 {
        c2 = h.redo(c2).ok_or("redo yöq")?;
    }
    if c2 != full {
        return Err("redo matnni aynan tiklamadi");
    }
    n += 1;
    // Yangi tahrir redo ni tozalaydi
    let c3 = h.undo(c2).ok_or("undo yöq")?;
    h.record(EditKind::Delete, &c3.lines, c3.row, c3.col);
    if h.redo_len() != 0 {
        return Err("yangi tahrir redo ni tozalamadi");
    }
    n += 1;
    // 200 qadam çegarasi
    let mut h2 = EditHistory::new();
    for i in 0..(MAX_STEPS + 50) {
        h2.record(EditKind::Enter, &[i.to_string()], 0, 0);
    }
    if h2.undo_len() != MAX_STEPS {
        return Err("200 qadam çegarasi işlamadi");
    }
    n += 1;
    // Qidiruv: UTF-8 — ustun belgilarda
    let m = find_all("göz ög\nsöz öö", "ö");
    if m != alloc::vec![(0, 1), (0, 4), (1, 1), (1, 4), (1, 5)] {
        return Err("'ö' qidiruvi notoʻgʻri ustun berdi");
    }
    n += 1;
    if find_all("aaaa", "aa") != alloc::vec![(0, 0), (0, 2)] || !find_all("abc", "").is_empty() {
        return Err("ustma-ust/böş so'rov qidiruvi notoʻgʻri");
    }
    n += 1;
    let ls = alloc::vec![String::from("Ştat ʼuy"), String::from("uy")];
    if find_in_lines(&ls, "uy") != alloc::vec![(0, 6), (1, 0)] {
        return Err("find_in_lines notoʻgʻri");
    }
    n += 1;
    n += self_test_select()?;
    Ok(n)
}

/// Tanlaş, nusxa, kesiş, joylaş, tanlangan matn ustiga yoziş va Alt+Z — UTF-8 bilan.
fn self_test_select() -> Result<usize, &'static str> {
    let v = |a: &[&str]| a.iter().map(|s| String::from(*s)).collect::<Vec<String>>();
    let orig = v(&["ğalla öy", "çiroq yondi", "öğil ça"]);
    let mut lines = orig.clone();
    let mut h = EditHistory::new();
    let mut n = 0;
    // köp qatorli tanlaş: (0,2) dan (2,3) gaça ("lla öy\nçiroq yondi\nöği")
    let (mut cur, mut anchor): (Pos, Option<Pos>) = ((2, 3), Some((0, 2)));
    let sel = range_text(&lines, anchor.unwrap(), cur);
    if sel != "lla öy\nçiroq yondi\nöği" || range_text(&lines, cur, anchor.unwrap()) != sel {
        return Err("köp qatorli tanlaş matni notöğri");
    }
    n += 1;
    // nusxa (almaşuv buferi orqali; asl qiymat tiklanadi)
    let saved = crate::clip::get();
    crate::clip::set(&sel);
    let copied = crate::clip::get();
    crate::clip::set(&saved);
    if copied != sel {
        return Err("almaşuv buferi nusxani saqlamadi");
    }
    n += 1;
    // kesiş — bitta qadam
    if !replace_sel(&mut h, EditKind::Paste, &mut lines, &mut cur, &mut anchor, "") || lines != v(&["ğal ça"]) || cur != (0, 2) || anchor.is_some() || h.undo_len() != 1 {
        return Err("kesiş notöğri");
    }
    n += 1;
    // joylaş (köp qatorli) — yana asl matn, bitta qadam
    replace_sel(&mut h, EditKind::Paste, &mut lines, &mut cur, &mut anchor, &copied);
    if lines != orig || cur != (2, 3) || h.undo_len() != 2 {
        return Err("köp qatorli joylaş notöğri");
    }
    n += 1;
    // tanlangan "çiroq" ustiga yoziş: birinçi harf tanlaşni almaştiradi
    let mut anchor = Some((1, 0));
    cur = (1, 5);
    for ch in ["ç", "ö", "ğ"] {
        replace_sel(&mut h, EditKind::Type { ws: false }, &mut lines, &mut cur, &mut anchor, ch);
    }
    if lines[1] != "çöğ yondi" || cur != (1, 3) || h.undo_len() != 4 {
        return Err("tanlangan matn ustiga yoziş notöğri");
    }
    n += 1;
    // Alt+Z: yoziş, tanlaş almaştiriş, joylaş, kesiş — har biri bitta qadam
    let mut snap = Snapshot { lines: lines.clone(), row: cur.0, col: cur.1 };
    snap = h.undo(snap).ok_or("undo yöq")?; // "öğ" yozilişi
    if snap.lines[1] != "ç yondi" {
        return Err("undo: yozilgan harflar qaytmadi");
    }
    snap = h.undo(snap).ok_or("undo yöq")?; // tanlaşni "ç" bilan almaştiriş
    if snap.lines != orig {
        return Err("undo: tanlangan matn tiklanmadi");
    }
    snap = h.undo(snap).ok_or("undo yöq")?; // joylaş
    if snap.lines != v(&["ğal ça"]) || (snap.row, snap.col) != (0, 2) {
        return Err("undo: joylaş bekor qilinmadi");
    }
    snap = h.undo(snap).ok_or("undo yöq")?; // kesiş
    if snap.lines != orig || (snap.row, snap.col) != (2, 3) {
        return Err("undo: kesiş bekor qilinmadi");
    }
    n += 1;
    // Shift+strelkalar: tanlaş kengayadi, langarga qaytilsa — bekor; Shift siz — bekor
    let mut an: Option<Pos> = None;
    nav_select(&mut an, (1, 2), (1, 3), true); // Shift+→
    nav_select(&mut an, (1, 3), (2, 3), true); // Shift+↓
    if an != Some((1, 2)) || range_text(&orig, an.unwrap(), (2, 3)) != "roq yondi\nöği" {
        return Err("Shift+strelka tanlaşi notöğri");
    }
    nav_select(&mut an, (2, 3), (1, 3), true); // Shift+↑
    nav_select(&mut an, (1, 3), (1, 2), true); // Shift+← — langarga qaytdi
    if an.is_some() {
        return Err("Shift+strelka tanlaşni bekor qilmadi");
    }
    nav_select(&mut an, (0, 0), (0, 8), true); // Shift+End
    nav_select(&mut an, (0, 8), (0, 7), false); // oddiy ←
    if an.is_some() {
        return Err("Shift siz strelka tanlaşni bekor qilmadi");
    }
    n += 1;
    // bir qator içida, chegaralar va böş tanlaş
    let mut l2 = v(&["öç"]);
    let (mut c2, mut a2) = ((0, 1), Some((0, 1)));
    if replace_sel(&mut h, EditKind::Paste, &mut l2, &mut c2, &mut a2, "") || l2 != v(&["öç"]) {
        return Err("böş tanlaş kesilmasligi kerak");
    }
    if range_text(&l2, (0, 0), (0, 99)) != "öç" || clamp_pos(&l2, (5, 9)) != (0, 2) {
        return Err("tanlaş çegarasi notöğri");
    }
    n += 1;
    Ok(n)
}
