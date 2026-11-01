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
    (["tiklaş", "сброс", "reset"], &["tiklash"], ["sozlamalarni öçiriş", "стереть настройки", "erase settings"]),
    (["hisoblar", "пользователи", "users"], &[], ["foydalanuvçilar: hisoblar [qöş | öçir] login", "пользователи: [добавить | удалить] логин", "users: [add | del] login"]),
    (["qayta", "перезагрузка", "reboot"], &[], ["qayta yuklaş", "перезагрузить", "restart"]),
    (["öçir", "выкл", "shutdown"], &["ochir", "ocir"], ["tizimni öçiriş", "выключить систему", "power off"]),
];

fn find(table: &[Cmd], name: &str) -> Option<usize> {
    table.iter().position(|(names, aliases, _)| names.contains(&name) || aliases.contains(&name))
}

struct Shell {
    cwd: String,
    /// Fondagi jarayonlar: (PID, nomi)
    background: Vec<(u64, String)>,
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

    fn prompt(&self) {
        let user = zamin::env("FOYDALANUVCHI").unwrap_or_default();
        print!("{}@zamin:{}> ", user, self.cwd);
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
            15 => return false, // çiqiş
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
    let mut sh = Shell { cwd: String::from("~"), background: Vec::new() };
    println!("{}", tr([
        "Zamin terminali. Buyruqlar röyxati uçun: yordam",
        "Терминал Zamin. Список команд: помощь",
        "Zamin terminal. For a list of commands: help",
    ]));
    loop {
        sh.reap_background();
        sh.prompt();
        let Some(line) = zamin::read_line() else { return 0 }; // Ctrl+D — çiqiş
        if !sh.run_line(&line) {
            return 0;
        }
    }
}
