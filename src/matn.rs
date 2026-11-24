// Matn utilitalari — terminal buyruqlari `qidir`, `bosh`, `oxir`, `hajm`, `nusxa`, `köçir`
// uçun sof funksiyalar. Bir xil kod yadro (seriya qobiq, `src/shell.rs`) va foydalanuvçi
// terminali (`user/src/bin/terminal.rs`, `zamin::matn` orqali `include!`) da işlatiladi.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// `bosh` / `oxir` uçun standart qatorlar soni.
pub const DEFAULT_LINES: usize = 10;

/// `qidir` parametrlari.
#[derive(Debug, PartialEq)]
pub struct GrepOpts<'a> {
    /// -i: katta-kiçik harf farqlanmaydi
    pub ignore_case: bool,
    /// -n: qator raqamlari
    pub numbers: bool,
    pub pattern: &'a str,
    pub file: &'a str,
}

/// `qidir [-i] [-n] naqsh fayl` (bayroqlar birlaşgan bölişi ham mumkin: `-in`).
pub fn parse_grep(args: &str) -> Option<GrepOpts<'_>> {
    let mut o = GrepOpts { ignore_case: false, numbers: false, pattern: "", file: "" };
    let mut rest: Vec<&str> = Vec::new();
    for w in args.split_whitespace() {
        match w.strip_prefix('-') {
            Some(f) if rest.is_empty() && !f.is_empty() && f.chars().all(|c| c == 'i' || c == 'n') => {
                o.ignore_case |= f.contains('i');
                o.numbers |= f.contains('n');
            }
            _ => rest.push(w),
        }
    }
    match rest.as_slice() {
        [p, f] => {
            o.pattern = p;
            o.file = f;
            Some(o)
        }
        _ => None,
    }
}

/// Naqsh uçraydigan qatorlar (`-n` bölsa "raqam:qator").
pub fn grep(text: &str, o: &GrepOpts) -> Vec<String> {
    let pat = if o.ignore_case { o.pattern.to_lowercase() } else { String::from(o.pattern) };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let hit = if o.ignore_case { line.to_lowercase().contains(&pat) } else { line.contains(&pat) };
        if hit {
            out.push(if o.numbers { format!("{}:{}", i + 1, line) } else { String::from(line) });
        }
    }
    out
}

/// `bosh|oxir [-n N | -N] [fayl]` → (N, fayl). Fayl böş bölişi mumkin.
pub fn parse_count(args: &str) -> Option<(usize, &str)> {
    let mut n = DEFAULT_LINES;
    let mut file = "";
    let mut it = args.split_whitespace();
    while let Some(w) = it.next() {
        if w == "-n" {
            n = it.next()?.parse().ok()?;
        } else if let Some(v) = w.strip_prefix("-n") {
            n = v.parse().ok()?;
        } else if let Some(v) = w.strip_prefix('-').filter(|v| !v.is_empty()) {
            n = v.parse().ok()?;
        } else if file.is_empty() {
            file = w;
        } else {
            return None;
        }
    }
    Some((n, file))
}

/// Birinçi `n` qator.
pub fn head(text: &str, n: usize) -> Vec<&str> {
    text.lines().take(n).collect()
}

/// Oxirgi `n` qator.
pub fn tail(text: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

/// (qatorlar, sözlar, baytlar).
pub fn wc(data: &[u8]) -> (usize, usize, usize) {
    let text = String::from_utf8_lossy(data);
    (text.lines().count(), text.split_whitespace().count(), data.len())
}

/// `nusxa a b` / `köçir a b`: manzil papka bölsa — fayl nomi unga qöşiladi.
pub fn dest_path(src: &str, dst: &str, dst_is_dir: bool) -> String {
    if !dst_is_dir {
        return String::from(dst);
    }
    let name = src.trim_end_matches('/').rsplit('/').next().unwrap_or(src);
    format!("{}/{}", dst.trim_end_matches('/'), name)
}

/// Sof funksiyalarni tekşiradi. Natija: 0 — hammasi töğri, aks holda xato raqami.
pub fn self_test() -> u32 {
    let text = "Salom Zamin\nikkinçi qator\nZAMIN OS\noxirgi zamin\n";
    let Some(g) = parse_grep("-n zamin /tmp/b.txt") else { return 1 };
    if g != (GrepOpts { ignore_case: false, numbers: true, pattern: "zamin", file: "/tmp/b.txt" }) {
        return 2;
    }
    if grep(text, &g) != ["4:oxirgi zamin"] {
        return 3;
    }
    let Some(gi) = parse_grep("-in zamin f") else { return 4 };
    if grep(text, &gi) != ["1:Salom Zamin", "3:ZAMIN OS", "4:oxirgi zamin"] || parse_grep("zamin").is_some() {
        return 5;
    }
    let Some(gc) = parse_grep("-i şahar f") else { return 6 };
    if grep("ŞAHAR\nboşqa\n", &gc) != ["ŞAHAR"] {
        return 6;
    }
    if parse_count("-n 1 a.txt") != Some((1, "a.txt")) || parse_count("-3") != Some((3, "")) || parse_count("a") != Some((DEFAULT_LINES, "a")) {
        return 7;
    }
    if parse_count("-n x a").is_some() || parse_count("a b").is_some() {
        return 8;
    }
    if head(text, 1) != ["Salom Zamin"] || tail(text, 1) != ["oxirgi zamin"] || tail(text, 99).len() != 4 || head("", 3).len() != 0 {
        return 9;
    }
    if wc(text.as_bytes()) != (4, 8, text.len()) || wc(b"") != (0, 0, 0) {
        return 10;
    }
    if dest_path("/tmp/a.txt", "/uy", true) != "/uy/a.txt" || dest_path("/tmp/a.txt", "/tmp/b", false) != "/tmp/b" {
        return 11;
    }
    0
}
