//! Matn muharriri uçun sof mantiq: bekor qiliş/qaytariş tarixi (EditHistory)
//! va qidiruv (find_all). GUI ga bogʻliq emas — self-test bilan tekşiriladi.

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
    Ok(n)
}
