//! terminal — Zamin terminali (foydalanuvçi rejimida işlaydi).
//!
//!   dastur arg1 arg2          — dasturni işga tuşiriş (PATH böyiça qidiriladi)
//!   a | b | c                 — quvur: çiqiş keyingisining kirişiga
//!   dastur > fayl / >> fayl   — çiqişni faylga yoziş / qöşiş
//!   dastur &                  — fonda işlatiş
//!   Ctrl+C                    — oldingi plandagi dasturni töxtatiş
//!
//! Tizim buyruqlari (parol, tiklaş, öçir, jarayonlar...) yadroga uzatiladi:
//! terminal — yadro işga tuşirgan "işonçli" jarayon, uning dasturlari esa emas.
#![no_std]
#![no_main]

use zamin::abi::{INHERIT, OPEN_APPEND, OPEN_CREATE, OPEN_TRUNCATE};
use zamin::fs::{self, File};
use zamin::{String, Vec, format, print, println, tr};

zamin::entry!(main);

/// [uz, ru, en], qöşimça nomlar, yordam [uz, ru, en]
type Cmd = ([&'static str; 3], &'static [&'static str], [&'static str; 3]);

const BUILTINS: &[Cmd] = &[
    (["yordam", "помощь", "help"], &[], ["şu röyxat", "этот список", "this list"]),
    (["aks", "эхо", "echo"], &[], ["matnni çiqaradi", "выводит текст", "prints text"]),
    (["tozala", "очистить", "clear"], &[], ["ekranni tozalaş", "очистить экран", "clear the screen"]),
    (["fayllar", "файлы", "ls"], &[], ["papka tarkibi", "содержимое папки", "list a directory"]),
    (["öqi", "читать", "cat"], &["oqi"], ["faylni körsatiş", "показать файл", "show a file"]),
    (["yoz", "записать", "write"], &[], ["faylga yoziş: yoz fayl matn", "записать: записать файл текст", "write: write file text"]),
    (["papka", "папка", "mkdir"], &[], ["papka yaratiş", "создать папку", "create a directory"]),
    (["taşla", "удалить", "rm"], &["tashla"], ["fayl yoki böş papkani öçiriş", "удалить файл или пустую папку", "remove a file or empty directory"]),
    (["kir", "перейти", "cd"], &[], ["papkaga kiriş: kir Hujjatlar, kir ..", "перейти в папку: перейти ..", "change directory: cd .."]),
    (["joy", "где", "pwd"], &[], ["joriy papka", "текущая папка", "current directory"]),
    (["muhit", "окружение", "env"], &[], ["muhit özgaruvçisi: muhit NOM yoki muhit NOM=qiymat", "переменная: окружение ИМЯ=значение", "variable: env NAME or env NAME=value"]),
    (["örnat", "установить", "install"], &["ornat"], ["dasturni örnatiş: örnat fayl [nom]", "установить программу: установить файл [имя]", "install a program: install file [name]"]),
    (["işlat", "запуск", "run"], &["ishlat", "islat"], ["dasturni işga tuşiriş", "запустить программу", "run a program"]),
    (["ruxsat", "права", "chmod"], &[], ["ruxsatlar: ruxsat 600 fayl", "права: права 600 файл", "permissions: chmod 600 file"]),
    (["çeklangan", "песочница", "sandbox"], &["cheklangan"], ["dasturni qafasda (faqat joriy papka) işlatiş", "запустить в песочнице (только текущая папка)", "run sandboxed (current folder only)"]),
    (["signal", "сигнал", "signal"], &[], ["signal yuboriş: signal PID [raqam]", "послать сигнал: сигнал PID [номер]", "send a signal: signal PID [num]"]),
    (["çiqiş", "выход", "exit"], &["chiqish", "chiqiş", "ciqis"], ["çiqiş (qayta kiriş söraladi)", "выйти (снова запросит вход)", "log out"]),
];

/// Yadroga uzatiladigan tizim buyruqlari
const ADMIN: &[Cmd] = &[
    (["haqida", "инфо", "about"], &[], ["tizim haqida", "о системе", "about the system"]),
    (["vaqt", "время", "uptime"], &[], ["işga tuşganidan beri", "время работы", "time since boot"]),
    (["xotira", "память", "mem"], &[], ["xotira holati", "состояние памяти", "memory status"]),
    (["dasturlar", "программы", "programs"], &[], ["örnatilgan dasturlar", "установленные программы", "installed programs"]),
    (["jarayonlar", "процессы", "ps"], &[], ["işlayotgan jarayonlar", "запущенные процессы", "running processes"]),
    (["tugat", "завершить", "kill"], &[], ["jarayonni tugatiş: tugat PID", "завершить процесс: завершить PID", "terminate: kill PID"]),
    (["kanallar", "каналы", "channels"], &[], ["Zamin kanallari", "каналы Zamin", "Zamin channels"]),
    (["til", "язык", "lang"], &[], ["tilni almaştiriş: uz, ru, en", "сменить язык: uz, ru, en", "change language: uz, ru, en"]),
    (["kimman", "кто", "whoami"], &[], ["joriy foydalanuvçi", "текущий пользователь", "current user"]),
    (["parol", "пароль", "passwd"], &[], ["parolni özgartiriş", "сменить пароль", "change password"]),
    (["saqla", "сохранить", "sync"], &[], ["fayllarni hozir saqlaş", "сохранить файлы сейчас", "save files now"]),
    (["xavfsizlik", "безопасность", "security"], &[], ["himoya holati", "состояние защиты", "protection status"]),
    (["mintaqa", "пояс", "tz"], &[], ["vaqt mintaqasi: mintaqa +5", "часовой пояс: пояс +5", "time zone: tz +5"]),
    (["jurnal", "журнал", "log"], &[], ["yadro jurnali: jurnal [qatorlar]", "журнал ядра: журнал [строк]", "kernel log: log [lines]"]),
    (["tiklaş", "сброс", "reset"], &["tiklash"], ["sozlamalarni öçiriş", "стереть настройки", "erase settings"]),
    (["hisoblar", "пользователи", "users"], &[], ["foydalanuvçilar: hisoblar [qöş | öçir] login", "пользователи: [добавить | удалить] логин", "users: [add | del] login"]),
    (["qayta", "перезагрузка", "reboot"], &[], ["qayta yuklaş", "перезагрузить", "restart"]),
    (["öçir", "выкл", "shutdown"], &["ochir", "ocir"], ["tizimni öçiriş", "выключить систему", "power off"]),
];

fn find(table: &[Cmd], name: &str) -> Option<usize> {
    table.iter().position(|(names, aliases, _)| names.contains(&name) || aliases.contains(&name))
}

// --- Buyruqlar tarixi va Tab bilan töldiriş (sof funksiyalar, --self-test tekşiradi) ---

const HISTORY_MAX: usize = 100;
const HISTORY_DIR: &str = "~/.zamin";
const HISTORY_PATH: &str = "~/.zamin/terminal_tarix";

/// Buyruqlar tarixi: ↑ — oldingisi, ↓ — keyingisi (oxirida — terilayotgan qoralama).
struct History {
    items: Vec<String>,
    /// Joriy körsatilayotgan element; `items.len()` — qoralama
    pos: usize,
    draft: String,
}

impl History {
    fn new() -> History {
        History { items: Vec::new(), pos: 0, draft: String::new() }
    }

    /// Fayl mazmunidan (har qatorda bitta buyruq).
    fn from_text(text: &str) -> History {
        let mut h = History::new();
        for line in text.lines() {
            h.push(line);
        }
        h
    }

    fn to_text(&self) -> String {
        let mut s = self.items.join("\n");
        if !s.is_empty() {
            s.push('\n');
        }
        s
    }

    /// Qöşadi (böş va oldingisi bilan bir xil qator qöşilmaydi). Natija: qöşildimi.
    fn push(&mut self, line: &str) -> bool {
        let line = line.trim();
        let added = !line.is_empty() && self.items.last().map(|l| l.as_str()) != Some(line);
        if added {
            self.items.push(String::from(line));
            if self.items.len() > HISTORY_MAX {
                self.items.remove(0);
            }
        }
        self.pos = self.items.len();
        self.draft.clear();
        added
    }

    /// ↑: oldingi buyruq. `current` — hozir terilgan matn (qoralama sifatida eslab qolinadi).
    fn prev(&mut self, current: &str) -> Option<&str> {
        if self.pos >= self.items.len() {
            self.pos = self.items.len();
            self.draft = String::from(current);
        }
        if self.pos == 0 {
            return None;
        }
        self.pos -= 1;
        Some(&self.items[self.pos])
    }

    /// ↓: keyingi buyruq yoki oxirida qoralama.
    fn next(&mut self) -> Option<&str> {
        if self.pos >= self.items.len() {
            return None;
        }
        self.pos += 1;
        Some(if self.pos == self.items.len() { &self.draft } else { &self.items[self.pos] })
    }
}

/// Tab: `prefix` bilan boşlanadigan nomzodlar. Natija: (umumiy prefiksgaça töldirilgan
/// matn, saralangan moslar). Moslik yöq bölsa — (prefix, böş röyxat).
fn complete(prefix: &str, candidates: &[String]) -> (String, Vec<String>) {
    let mut matches: Vec<String> = candidates.iter().filter(|c| c.starts_with(prefix)).cloned().collect();
    matches.sort();
    matches.dedup();
    let Some(first) = matches.first() else {
        return (String::from(prefix), matches);
    };
    // Umumiy prefiks (belgi çegarasida)
    let mut common = first.as_str();
    for m in &matches[1..] {
        let end = common.char_indices().zip(m.char_indices()).find(|((_, a), (_, b))| a != b).map_or(common.len().min(m.len()), |((i, _), _)| i);
        common = &common[..end];
    }
    (String::from(common), matches)
}

/// Qatorni Tab bilan töldiriş natijasi.
enum Tab {
    /// Yangi qator
    Replace(String),
    /// Variantlar röyxatini körsatiş
    List(Vec<String>),
    None,
}

/// Qatorning oxirgi sözini töldiradi. `cmds` — birinçi söz uçun nomzodlar,
/// `files(papka)` — papkadagi nomlar (papkalar '/' bilan). `second` — ketma-ket ikkinçi Tab.
fn tab_line(line: &str, second: bool, cmds: &[String], files: &dyn Fn(&str) -> Vec<String>) -> Tab {
    let start = line.rfind(' ').map_or(0, |i| i + 1);
    let before = line[..start].trim_end();
    let word = &line[start..];
    let is_cmd = before.is_empty() || before.ends_with('|');
    let (dir, cands) = if is_cmd && !word.contains('/') {
        ("", cmds.to_vec())
    } else {
        let dir = word.rfind('/').map_or("", |i| &word[..=i]);
        let rest = &word[dir.len()..];
        let names = files(dir).into_iter().filter(|n| rest.starts_with('.') || !n.starts_with('.'));
        (dir, names.map(|n| format!("{}{}", dir, n)).collect())
    };
    let (done, variants) = complete(word, &cands);
    let head = &line[..start];
    match variants.len() {
        0 => Tab::None,
        1 => {
            let sep = if done.ends_with('/') { "" } else { " " };
            Tab::Replace(format!("{}{}{}", head, done, sep))
        }
        _ if done.len() > word.len() => Tab::Replace(format!("{}{}", head, done)),
        _ if second => Tab::List(variants.iter().map(|v| String::from(&v[dir.len()..])).collect()),
        _ => Tab::None,
    }
}

/// `terminal --self-test`: tarix va Tab mantiqini tekşiradi.
fn self_test() -> i64 {
    let s = |v: &[&str]| v.iter().map(|x| String::from(*x)).collect::<Vec<String>>();
    // Tarix
    let mut h = History::new();
    if !h.push("salom") || h.push("salom") || h.push("   ") || !h.push("fayllar") {
        return 1;
    }
    if h.prev("qoralama") != Some("fayllar") || h.prev("") != Some("salom") || h.prev("").is_some() {
        return 2;
    }
    if h.next() != Some("fayllar") || h.next() != Some("qoralama") || h.next().is_some() {
        return 3;
    }
    for i in 0..150 {
        h.push(&format!("b{}", i));
    }
    if h.items.len() != HISTORY_MAX || h.items[0] != "b50" {
        return 4;
    }
    let h2 = History::from_text(&h.to_text());
    if h2.items != h.items {
        return 5;
    }
    // Tab
    let cmds = s(&["kir", "kimman", "katta", "fayllar"]);
    if complete("ki", &cmds) != (String::from("ki"), s(&["kimman", "kir"])) {
        return 6;
    }
    if complete("fa", &cmds) != (String::from("fayllar"), s(&["fayllar"])) || complete("x", &cmds).1.len() != 0 {
        return 7;
    }
    if complete("ö", &s(&["öqi", "öçir"])).0 != "ö" || complete("öq", &s(&["öqi", "öçir"])).0 != "öqi" {
        return 8;
    }
    let files = |d: &str| if d.is_empty() { s(&["Hujjatlar/", "Hisob.txt", ".yashirin"]) } else { s(&["xat.txt"]) };
    let ok = |t: Tab, want: &str| matches!(t, Tab::Replace(ref r) if r == want);
    if !ok(tab_line("kir", false, &cmds, &files), "kir ") || !matches!(tab_line("ki", false, &cmds, &files), Tab::None) {
        return 9;
    }
    if !matches!(tab_line("ki", true, &cmds, &files), Tab::List(ref v) if *v == s(&["kimman", "kir"])) {
        return 10;
    }
    if !ok(tab_line("kir Hu", false, &cmds, &files), "kir Hujjatlar/") || !ok(tab_line("öqi Hujjatlar/x", false, &cmds, &files), "öqi Hujjatlar/xat.txt ") {
        return 11;
    }
    if !matches!(tab_line("öqi H", false, &cmds, &files), Tab::None) {
        return 12;
    }
    if !ok(tab_line("aks 1 | fa", false, &cmds, &files), "aks 1 | fayllar ") {
        return 13;
    }
    println!("[ok] terminal tarix/tab: History push/prev/next, {} ta çeklov, complete() va tab_line()", HISTORY_MAX);
    0
}

struct Shell {
    cwd: String,
    /// Fondagi jarayonlar: (PID, nomi)
    background: Vec<(u64, String)>,
    history: History,
    /// Standart kirişdan kelgan, hali işlanmagan baytlar
    inbuf: Vec<u8>,
}

impl Shell {
    /// Yölni joriy papkaga nisbatan töliq yölga aylantiradi.
    fn resolve(&self, p: &str) -> String {
        if p.starts_with('/') || p.starts_with('~') {
            String::from(p)
        } else if p.is_empty() {
            self.cwd.clone()
        } else {
            format!("{}/{}", self.cwd.trim_end_matches('/'), p)
        }
    }

    fn prompt_str(&self) -> String {
        let user = zamin::env("FOYDALANUVCHI").unwrap_or_default();
        format!("{}@zamin:{}> ", user, self.cwd)
    }

    fn prompt(&self) {
        print!("{}", self.prompt_str());
    }

    /// Joriy qatorni qayta çizadi: \r + taklif + matn + ESC[K.
    fn redraw(&self, edit: &str) {
        print!("\r{}{}\x1b[K", self.prompt_str(), edit);
    }

    /// Navbatdagi bayt (kerak bölsa stdin dan öqiydi). `None` — kiriş tugadi.
    fn next_byte(&mut self) -> Option<u8> {
        if self.inbuf.is_empty() {
            let mut chunk = [0u8; 512];
            match zamin::read(zamin::abi::STDIN, &mut chunk) {
                Ok(n) if n > 0 => self.inbuf.extend_from_slice(&chunk[..n]),
                _ => return None,
            }
        }
        Some(self.inbuf.remove(0))
    }

    /// Qator öqiş: oddiy matn + boşqaruv baytlari (grafik terminal yuboradi):
    /// ESC[A / ESC[B — tarix, \t — töldiriş, 0x7f / 0x08 — oxirgi belgini öçiriş.
    fn read_line(&mut self) -> Option<String> {
        let mut edit: Vec<u8> = Vec::new();
        let mut last_tab = false;
        loop {
            let Some(b) = self.next_byte() else {
                return (!edit.is_empty()).then(|| String::from_utf8_lossy(&edit).into_owned());
            };
            let mut is_tab = false;
            match b {
                b'\n' => return Some(String::from_utf8_lossy(&edit).into_owned()),
                b'\r' => {}
                b'\t' => {
                    let cur = String::from_utf8_lossy(&edit).into_owned();
                    match tab_line(&cur, last_tab, &self.command_names(), &|d| self.dir_names(d)) {
                        Tab::Replace(new) => {
                            edit = new.into_bytes();
                            self.redraw(&String::from_utf8_lossy(&edit));
                        }
                        Tab::List(variants) => {
                            print!("\r{}{}\n  {}\n", self.prompt_str(), cur, variants.join("  "));
                            self.redraw(&cur);
                        }
                        Tab::None => self.redraw(&cur),
                    }
                    is_tab = true;
                }
                0x1b => {
                    // ESC [ A / ESC [ B
                    let (Some(b'['), Some(k)) = (self.next_byte(), self.next_byte()) else { continue };
                    let cur = String::from_utf8_lossy(&edit).into_owned();
                    let new = match k {
                        b'A' => self.history.prev(&cur).map(String::from),
                        b'B' => self.history.next().map(String::from),
                        _ => None,
                    };
                    if let Some(n) = new {
                        edit = n.into_bytes();
                    }
                    self.redraw(&String::from_utf8_lossy(&edit));
                }
                0x7f | 0x08 => {
                    let mut cur = String::from_utf8_lossy(&edit).into_owned();
                    cur.pop();
                    edit = cur.into_bytes();
                    self.redraw(&String::from_utf8_lossy(&edit));
                }
                // Ctrl+C (grafik terminal `^C` ni özi çiqaradi): qator bekor, yangi taklif
                0x03 => {
                    edit.clear();
                    self.prompt();
                }
                0..=0x1f => {}
                _ => edit.push(b),
            }
            last_tab = is_tab;
        }
    }

    /// Birinçi söz uçun nomzodlar: içki buyruqlar (uç tilda) va PATH dagi dasturlar.
    fn command_names(&self) -> Vec<String> {
        let mut out: Vec<String> = BUILTINS.iter().chain(ADMIN.iter()).flat_map(|(names, _, _)| names.iter().map(|n| String::from(*n))).collect();
        let path = zamin::env("PATH").unwrap_or_else(|| String::from("/dasturlar"));
        for dir in path.split(':') {
            if let Ok(list) = fs::entries(dir) {
                out.extend(list.into_iter().filter(|e| !e.is_dir).map(|e| e.name));
            }
        }
        out
    }

    /// Papkadagi nomlar (joriy papkaga nisbatan); papkalarga '/' qöşiladi.
    fn dir_names(&self, dir: &str) -> Vec<String> {
        fs::entries(&self.resolve(dir))
            .map(|list| list.into_iter().map(|e| if e.is_dir { format!("{}/", e.name) } else { e.name }).collect())
            .unwrap_or_default()
    }

    /// Qatorni tarixga qöşadi va ~/.zamin/terminal_tarix ga saqlaydi.
    fn remember(&mut self, line: &str) {
        if self.history.push(line) {
            let _ = fs::mkdir(HISTORY_DIR);
            let _ = fs::write(HISTORY_PATH, self.history.to_text().as_bytes());
        }
    }

    /// Dastur faylini topadi: yöl bölsa — özi, aks holda PATH böyiça.
    fn find_program(&self, name: &str) -> Option<String> {
        if name.contains('/') || name.starts_with('~') {
            let p = self.resolve(name);
            return fs::entries(&p).is_err().then_some(p); // papka emas
        }
        let path = zamin::env("PATH").unwrap_or_else(|| String::from("/dasturlar"));
        for dir in path.split(':') {
            let candidate = format!("{}/{}", dir.trim_end_matches('/'), name);
            if File::open(&candidate, 0).is_ok() {
                return Some(candidate);
            }
        }
        None
    }

    /// Fondagi tugagan jarayonlar haqida xabar beradi.
    fn reap_background(&mut self) {
        self.background.retain(|(pid, name)| match zamin::try_wait(*pid) {
            Ok(Some(code)) => {
                println!("  [{} {} {}: {}]", pid, name, tr(["tugadi, kod", "завершён, код", "exited, code"]), code);
                false
            }
            Ok(None) => true,
            Err(_) => false,
        });
    }

    fn run_line(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return true;
        }
        let (line, background) = match line.strip_suffix('&') {
            Some(l) => (l.trim(), true),
            None => (line, false),
        };
        // Yönaltiriş: oxirgi buyruqdan keyin "> fayl" yoki ">> fayl"
        let (line, redirect) = if let Some((l, f)) = line.split_once(">>") {
            (l.trim(), Some((f.trim(), true)))
        } else if let Some((l, f)) = line.split_once('>') {
            (l.trim(), Some((f.trim(), false)))
        } else {
            (line, None)
        };
        let stages: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        // `aks matn > fayl` — içki buyruq, lekin yönaltiriş bilan
        if stages.len() == 1 && !background {
            if let (Some((f, append)), Some(text)) = (redirect, stages[0].strip_prefix("aks ").or_else(|| stages[0].strip_prefix("echo "))) {
                let flags = OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE };
                match File::open(&self.resolve(f), flags) {
                    Ok(mut file) => {
                        let _ = file.write(format!("{}\n", text.trim()).as_bytes());
                    }
                    Err(e) => println!("  {}: {}", f, e),
                }
                return true;
            }
        }
        if stages.len() == 1 && redirect.is_none() && !background {
            let (name, args) = stages[0].split_once(' ').map_or((stages[0], ""), |(n, a)| (n, a.trim()));
            if let Some(i) = find(BUILTINS, name) {
                return self.builtin(i, args);
            }
            if find(ADMIN, name).is_some() {
                if zamin::admin(stages[0]).is_err() {
                    println!("  {}", tr(["ruxsat yöq", "доступ запрещён", "permission denied"]));
                }
                return true;
            }
        }
        self.run_pipeline(&stages, redirect, background);
        true
    }

    fn run_pipeline(&mut self, stages: &[&str], redirect: Option<(&str, bool)>, background: bool) {
        // Har bir bosqiç uçun dasturni topamiz
        let mut programs = Vec::new();
        for st in stages {
            let st = st.strip_prefix("işlat ").or_else(|| st.strip_prefix("ishlat ")).unwrap_or(st).trim();
            let (name, args) = st.split_once(' ').map_or((st, ""), |(n, a)| (n, a.trim()));
            if name.is_empty() {
                println!("  {}", tr(["böş buyruq", "пустая команда", "empty command"]));
                return;
            }
            match self.find_program(name) {
                Some(path) => programs.push((path, String::from(name), String::from(args))),
                None => {
                    println!("  {}: {}", tr(["nomaʼlum buyruq yoki dastur", "неизвестная команда или программа", "unknown command or program"]), name);
                    return;
                }
            }
        }
        let out_file = match redirect {
            Some((f, append)) => {
                let flags = OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE };
                match File::open(&self.resolve(f), flags) {
                    Ok(file) => Some(file),
                    Err(e) => {
                        println!("  {}: {}", f, e);
                        return;
                    }
                }
            }
            None => None,
        };

        let mut pids = Vec::new();
        // Fondagi dastur klaviaturani terminaldan "öğirlamasin": uning kirişi — böş quvur (darhol EOF)
        let mut prev_read: Option<u64> = None;
        if background {
            if let Ok((r, w)) = zamin::pipe() {
                zamin::close(w);
                prev_read = Some(r);
            }
        }
        let n = programs.len();
        for (k, (path, name, args)) in programs.iter().enumerate() {
            let (next_read, write_end) = if k + 1 < n {
                match zamin::pipe() {
                    Ok((r, w)) => (Some(r), Some(w)),
                    Err(e) => {
                        println!("  {}", e);
                        break;
                    }
                }
            } else {
                (None, None)
            };
            let stdin = prev_read.unwrap_or(INHERIT);
            let stdout = match (write_end, &out_file) {
                (Some(w), _) => w,
                (None, Some(f)) => f.handle(),
                (None, None) => INHERIT,
            };
            match zamin::spawn_with(path, args, [stdin, stdout, INHERIT]) {
                Ok(pid) => pids.push((pid, name.clone())),
                Err(e) => println!("  {}: {}", name, e),
            }
            // Bizdagi nusxalarni yopamiz — aks holda öquvçi heç qaçon "oxiri" ni körmaydi
            if let Some(r) = prev_read {
                zamin::close(r);
            }
            if let Some(w) = write_end {
                zamin::close(w);
            }
            prev_read = next_read;
        }
        drop(out_file);

        if background {
            for (pid, name) in pids {
                println!("  [{} {} {}]", pid, name, tr(["fonda", "в фоне", "in background"]));
                self.background.push((pid, name));
            }
            return;
        }
        let last = pids.last().map(|(p, _)| *p).unwrap_or(0);
        zamin::set_foreground(last);
        for (pid, name) in &pids {
            match zamin::wait(*pid) {
                Ok(0) => {}
                Ok(-9) => println!("  [{} {}]", name, tr(["töxtatildi", "остановлен", "stopped"])),
                Ok(code) => println!("  [{} {}: {}]", name, tr(["çiqiş kodi", "код выхода", "exit code"]), code),
                Err(_) => {}
            }
        }
        zamin::set_foreground(0);
    }

    fn builtin(&mut self, i: usize, args: &str) -> bool {
        let lang = zamin::lang();
        match i {
            0 => {
                println!("  {}", tr(["Terminal buyruqlari:", "Команды терминала:", "Terminal commands:"]));
                for (names, _, help) in BUILTINS.iter().chain(ADMIN.iter()) {
                    let pad = 14usize.saturating_sub(names[lang].chars().count());
                    println!("    {}{:pad$}— {}", names[lang], "", help[lang], pad = pad);
                }
                println!("  {}", tr([
                    "Dasturlar: nomi bilan (salom, hisobla 100000); a | b — quvur; > fayl — yönaltiriş; & — fonda",
                    "Программы: по имени; a | b — конвейер; > файл — перенаправление; & — в фоне",
                    "Programs: by name; a | b — pipe; > file — redirect; & — background",
                ]));
            }
            1 => println!("{}", args),
            2 => print!("\x1b[2J\x1b[H"),
            3 => {
                let path = self.resolve(args);
                match fs::entries(&path) {
                    Ok(list) if list.is_empty() => println!("  {}", tr(["(böş papka)", "(пустая папка)", "(empty directory)"])),
                    Ok(list) => {
                        for e in list {
                            let shown = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
                            let size = if e.is_dir { format!("{} {}", e.size, tr(["ta", "шт.", "items"])) } else { format!("{} B", e.size) };
                            let po = 10usize.saturating_sub(e.owner.chars().count());
                            let when = if e.mtime > 0 { zamin::time::short(&zamin::time::local(e.mtime)) } else { String::from("-") };
                            println!("  {} {}{:po$} {:>10}  {}  {}", fs::mode_string(e.mode, e.is_dir), e.owner, "", size, when, shown, po = po);
                        }
                    }
                    Err(e) => println!("  {}: {}", path, e),
                }
            }
            4 => {
                let path = self.resolve(args);
                match fs::read(&path) {
                    Ok(data) => match core::str::from_utf8(&data) {
                        Ok(t) => {
                            print!("{}", t);
                            if !t.is_empty() && !t.ends_with('\n') {
                                println!();
                            }
                        }
                        Err(_) => println!("  ({} B)", data.len()),
                    },
                    Err(e) => println!("  {}: {}", path, e),
                }
            }
            5 => match args.split_once(' ') {
                Some((f, text)) => {
                    let path = self.resolve(f);
                    match fs::write(&path, format!("{}\n", text).as_bytes()) {
                        Ok(()) => println!("  {}: {}", tr(["yozildi", "записано", "written"]), path),
                        Err(e) => println!("  {}: {}", path, e),
                    }
                }
                None => println!("  {}", BUILTINS[5].2[lang]),
            },
            6 => {
                let path = self.resolve(args);
                if let Err(e) = fs::mkdir(&path) {
                    println!("  {}: {}", path, e);
                }
            }
            7 => {
                let path = self.resolve(args);
                if let Err(e) = fs::remove(&path) {
                    println!("  {}: {}", path, e);
                }
            }
            8 => {
                let target = if args.is_empty() { String::from("~") } else { self.resolve(args) };
                match zamin::chdir(&target) {
                    Ok(()) => {
                        self.cwd = normalize(&target);
                        let _ = zamin::chdir(&self.cwd); // yadroga ham soddalaştirilgan yöl
                    }
                    Err(e) => println!("  {}: {}", target, e),
                }
            }
            9 => println!("  {}", self.cwd),
            10 => match args.split_once('=') {
                Some((k, v)) => {
                    if let Err(e) = zamin::set_env(k.trim(), v.trim()) {
                        println!("  {}", e);
                    }
                }
                None if !args.is_empty() => println!("  {}={}", args, zamin::env(args).unwrap_or_default()),
                None => {
                    for k in ["PATH", "FOYDALANUVCHI", "UY"] {
                        println!("  {}={}", k, zamin::env(k).unwrap_or_default());
                    }
                }
            },
            11 => self.install(args),
            12 => {
                self.run_pipeline(&[args], None, false);
            }
            16 => return false, // çiqiş
            15 => {
                let mut it = args.split_whitespace();
                let (Some(pid), sig) = (it.next().and_then(|v| v.parse::<u64>().ok()), it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(zamin::abi::SIG_TERM)) else {
                    println!("  {}", BUILTINS[15].2[lang]);
                    return true;
                };
                match zamin::signal(pid, sig) {
                    Ok(()) => println!("  {} {} -> {}", tr(["signal yuborildi:", "сигнал отправлен:", "signal sent:"]), sig, pid),
                    Err(e) => println!("  {}: {}", pid, e),
                }
            }
            13 => {
                let Some((mode, file)) = args.split_once(' ') else {
                    println!("  {}", BUILTINS[13].2[lang]);
                    return true;
                };
                match u16::from_str_radix(mode.trim(), 8) {
                    Ok(m) => {
                        if let Err(e) = zamin::chmod(&self.resolve(file.trim()), m) {
                            println!("  {}: {}", file, e);
                        }
                    }
                    Err(_) => println!("  {}", BUILTINS[13].2[lang]),
                }
            }
            14 => {
                // Qafas: faqat joriy papka; dastur işga tuşira olmaydi, nomli kanallar yöq
                let (name, rest) = args.split_once(' ').map_or((args, ""), |(n, a)| (n, a.trim()));
                let Some(path) = self.find_program(name) else {
                    println!("  {}: {}", tr(["nomaʼlum dastur", "неизвестная программа", "unknown program"]), name);
                    return true;
                };
                match zamin::spawn_full(&path, rest, [INHERIT; 3], zamin::SpawnExtra { handle: None, sandbox: true }) {
                    Ok(pid) => {
                        zamin::set_foreground(pid);
                        let code = zamin::wait(pid).unwrap_or(-1);
                        zamin::set_foreground(0);
                        println!("  [{} {}: {}]", name, tr(["qafasda tugadi, kod", "завершён в песочнице, код", "finished in sandbox, code"]), code);
                    }
                    Err(e) => println!("  {}: {}", name, e),
                }
            }
            _ => {}
        }
        true
    }

    /// Dasturni ~/dasturlar ga nusxalaydi (ELF ekanligi tekşiriladi).
    fn install(&self, args: &str) {
        let mut it = args.split_whitespace();
        let Some(src) = it.next() else {
            println!("  {}", BUILTINS[11].2[zamin::lang()]);
            return;
        };
        let src_path = self.resolve(src);
        let data = match fs::read(&src_path) {
            Ok(d) => d,
            Err(e) => {
                println!("  {}: {}", src_path, e);
                return;
            }
        };
        if data.get(..4) != Some(b"\x7fELF") || data.get(18) != Some(&183) {
            println!("  {}", tr(["bu Zamin dasturi emas (AArch64 ELF kerak)", "это не программа Zamin (нужен AArch64 ELF)", "not a Zamin program (AArch64 ELF required)"]));
            return;
        }
        let name = it.next().unwrap_or_else(|| src.rsplit('/').next().unwrap_or(src));
        let _ = fs::mkdir("~/dasturlar");
        let dst = format!("~/dasturlar/{}", name);
        let result = fs::write(&dst, &data).and_then(|_| zamin::chmod(&dst, 0o755));
        match result {
            Ok(()) => println!("  {} {} ({} B)", tr(["Örnatildi:", "Установлено:", "Installed:"]), name, data.len()),
            Err(e) => println!("  {}: {}", dst, e),
        }
    }
}

/// "a/b/../c" → "a/c" (körinadigan yöl uçun).
fn normalize(path: &str) -> String {
    let (root, rest) = if let Some(r) = path.strip_prefix('~') { ("~", r) } else { ("", path) };
    let mut parts: Vec<&str> = Vec::new();
    for p in rest.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return if root.is_empty() { String::from("/") } else { String::from(root) };
    }
    format!("{}/{}", root, parts.join("/"))
}

fn main() -> i64 {
    if zamin::args() == "--self-test" {
        return self_test();
    }
    let history = fs::read(HISTORY_PATH).map(|d| History::from_text(&String::from_utf8_lossy(&d))).unwrap_or_else(|_| History::new());
    let mut sh = Shell { cwd: String::from("~"), background: Vec::new(), history, inbuf: Vec::new() };
    println!("{}", tr([
        "Zamin terminali. Buyruqlar röyxati uçun: yordam",
        "Терминал Zamin. Список команд: помощь",
        "Zamin terminal. For a list of commands: help",
    ]));
    loop {
        sh.reap_background();
        sh.prompt();
        let Some(line) = sh.read_line() else { return 0 }; // Ctrl+D — çiqiş
        sh.remember(&line);
        if !sh.run_line(&line) {
            return 0;
        }
    }
}
