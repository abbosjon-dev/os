//! Zamin terminali — yadro içidagi oddiy buyruqlar qatori.
//! Buyruqlarni istalgan tilda yoziş mumkin: `yordam`, `помощь` yoki `help`.

use crate::i18n::{self, Lang, Text, msg};
use crate::line::{LineEditor, read_line};
use crate::memory::{frame, heap};
use crate::{accounts, config};
use crate::fs::{self, FsError};
use crate::task::{self, State};
use crate::{print, println, power, timer, tprint, tprintln, uart};
use alloc::string::String;
use alloc::format;

const PROMPT: &str = "zamin> ";

#[derive(Clone, Copy)]
enum Cmd {
    Help,
    About,
    Uptime,
    Memory,
    Echo,
    Clear,
    Lang,
    Ps,
    Programs,
    Run,
    Kill,
    Ls,
    Cat,
    Write,
    Mkdir,
    Rm,
    Channels,
    Whoami,
    Passwd,
    Reset,
    Save,
    Security,
    Reboot,
    Shutdown,
}

struct Command {
    cmd: Cmd,
    /// Asosiy nomlar: [uz, ru, en]
    names: Text,
    /// Qöşimça nomlar (masalan, maxsus harflarsiz yoziş uçun)
    aliases: &'static [&'static str],
    help: Text,
}

const COMMANDS: &[Command] = &[
    Command { cmd: Cmd::Help, names: ["yordam", "помощь", "help"], aliases: &[], help: ["şu röyxat", "этот список", "this list"] },
    Command { cmd: Cmd::About, names: ["haqida", "инфо", "about"], aliases: &[], help: ["tizim haqida", "о системе", "about the system"] },
    Command { cmd: Cmd::Uptime, names: ["vaqt", "время", "uptime"], aliases: &[], help: ["işga tuşganidan beri ötgan vaqt", "время с момента запуска", "time since boot"] },
    Command { cmd: Cmd::Memory, names: ["xotira", "память", "mem"], aliases: &[], help: ["xotira holati", "состояние памяти", "memory status"] },
    Command { cmd: Cmd::Echo, names: ["aks", "эхо", "echo"], aliases: &[], help: ["matnni qaytaradi", "выводит текст", "prints text"] },
    Command { cmd: Cmd::Clear, names: ["tozala", "очистить", "clear"], aliases: &[], help: ["ekranni tozalaş", "очистить экран", "clear the screen"] },
    Command { cmd: Cmd::Lang, names: ["til", "язык", "lang"], aliases: &[], help: ["tilni almaştiriş: uz, ru, en", "сменить язык: uz, ru, en", "change language: uz, ru, en"] },
    Command { cmd: Cmd::Ps, names: ["jarayonlar", "процессы", "ps"], aliases: &[], help: ["işlayotgan jarayonlar", "запущенные процессы", "running processes"] },
    Command { cmd: Cmd::Programs, names: ["dasturlar", "программы", "programs"], aliases: &[], help: ["mavjud dasturlar", "доступные программы", "available programs"] },
    Command { cmd: Cmd::Run, names: ["işlat", "запуск", "run"], aliases: &["ishlat", "islat"], help: ["dasturni işga tuşiriş: işlat salom", "запустить программу: запуск salom", "start a program: run salom"] },
    Command { cmd: Cmd::Kill, names: ["tugat", "завершить", "kill"], aliases: &[], help: ["jarayonni tugatiş: tugat <PID>", "завершить процесс: завершить <PID>", "terminate a process: kill <PID>"] },
    Command { cmd: Cmd::Ls, names: ["fayllar", "файлы", "ls"], aliases: &[], help: ["papka tarkibi: fayllar uy", "содержимое папки: файлы uy", "list a directory: ls uy"] },
    Command { cmd: Cmd::Cat, names: ["öqi", "читать", "cat"], aliases: &["oqi"], help: ["faylni körsatiş: öqi tizim/versiya", "показать файл: читать tizim/versiya", "show a file: cat tizim/versiya"] },
    Command { cmd: Cmd::Write, names: ["yoz", "записать", "write"], aliases: &[], help: ["faylga yoziş: yoz uy/a.txt Salom", "записать в файл: записать uy/a.txt Привет", "write to a file: write uy/a.txt Hello"] },
    Command { cmd: Cmd::Mkdir, names: ["papka", "папка", "mkdir"], aliases: &[], help: ["papka yaratiş: papka uy/rasmlar", "создать папку: папка uy/foto", "create a directory: mkdir uy/photos"] },
    Command { cmd: Cmd::Rm, names: ["taşla", "удалить", "rm"], aliases: &["tashla"], help: ["fayl yoki böş papkani öçiriş", "удалить файл или пустую папку", "remove a file or empty directory"] },
    Command { cmd: Cmd::Channels, names: ["kanallar", "каналы", "channels"], aliases: &[], help: ["Zamin kanallari (IPC)", "каналы Zamin (IPC)", "Zamin channels (IPC)"] },
    Command { cmd: Cmd::Whoami, names: ["kimman", "кто", "whoami"], aliases: &[], help: ["joriy foydalanuvçi", "текущий пользователь", "current user"] },
    Command { cmd: Cmd::Passwd, names: ["parol", "пароль", "passwd"], aliases: &[], help: ["parolni özgartiriş", "сменить пароль", "change password"] },
    Command { cmd: Cmd::Reset, names: ["tiklaş", "сброс", "reset"], aliases: &["tiklash"], help: ["sozlamalarni öçirib, birinçi sozlaşga qaytiş", "стереть настройки и начать заново", "erase settings and start over"] },
    Command { cmd: Cmd::Save, names: ["saqla", "сохранить", "sync"], aliases: &[], help: ["fayllarni hozir diskka saqlaş (şifrlangan)", "сохранить файлы на диск сейчас (зашифровано)", "save files to disk now (encrypted)"] },
    Command { cmd: Cmd::Security, names: ["xavfsizlik", "безопасность", "security"], aliases: &[], help: ["himoya holati", "состояние защиты", "protection status"] },
    Command { cmd: Cmd::Reboot, names: ["qayta", "перезагрузка", "reboot"], aliases: &[], help: ["qayta yuklaş", "перезагрузить", "restart"] },
    Command { cmd: Cmd::Shutdown, names: ["öçir", "выкл", "shutdown"], aliases: &["ocir", "ochir"], help: ["tizimni öçiriş", "выключить систему", "power off"] },
];

/// Dasturlar tavsifi: [uz, ru, en]
fn program_help(name: &str) -> Text {
    match name {
        "salom" => ["salomlaşadi", "приветствует", "says hello"],
        "hisobla" => [
            "tub sonlarni sanaydi: hisobla [çegara]",
            "считает простые числа: hisobla [предел]",
            "counts primes: hisobla [limit]",
        ],
        "soat" => [
            "har soniyada vaqtni körsatadi (5 marta)",
            "показывает время каждую секунду (5 раз)",
            "shows the time every second (5 times)",
        ],
        "buzuq" => [
            "yadro xotirasiga kirişga urinadi (himoyani körsatadi)",
            "пытается обратиться к памяти ядра (показывает защиту)",
            "tries to access kernel memory (shows protection)",
        ],
        "sinov" => ["tizim çaqiruvlarini tekşiradi", "проверяет системные вызовы", "tests system calls"],
        "kalkulyator" => [
            "\"hisob\" kanalidagi sörövlarni bajaradigan server",
            "сервер, выполняющий запросы из канала \"hisob\"",
            "server answering requests on channel \"hisob\"",
        ],
        "mijoz" => [
            "kalkulyator serverini işga tuşirib, kanal orqali sörövlar yuboradi",
            "запускает сервер kalkulyator и отправляет ему запросы по каналу",
            "starts the kalkulyator server and sends it requests over a channel",
        ],
        "fayl" => [
            "fayl yaratadi, yozadi va öqiydi",
            "создаёт, записывает и читает файл",
            "creates, writes and reads a file",
        ],
        _ => ["", "", ""],
    }
}

fn state_text(st: State) -> &'static str {
    i18n::pick(match st {
        State::Ready => &msg::STATE_READY,
        State::Running => &msg::STATE_RUNNING,
        State::Sleeping(_) => &msg::STATE_SLEEPING,
        State::WaitingInput | State::WaitingPid(_) | State::WaitingChannel(_) => &msg::STATE_WAITING,
        State::Dead(_) => &msg::STATE_DEAD,
    })
}

fn fs_error_text(e: FsError) -> &'static str {
    i18n::pick(match e {
        FsError::NotFound => &msg::FS_NOT_FOUND,
        FsError::Exists => &msg::FS_EXISTS,
        FsError::NotDir => &msg::FS_NOT_DIR,
        FsError::IsDir => &msg::FS_IS_DIR,
        FsError::NotEmpty => &msg::FS_NOT_EMPTY,
        FsError::Invalid => &msg::FS_INVALID,
        FsError::TooBig => &msg::FS_TOO_BIG,
        FsError::Denied => &msg::FS_DENIED,
    })
}

/// Fayllarni darhol diskka saqlaydi. Natija: şifrlangan tasvir hajmi.
pub fn flush() -> Result<usize, fs::zaminfs::ZfsError> {
    let key = accounts::master_key().ok_or(fs::zaminfs::ZfsError::NoDisk)?;
    fs::take_dirty();
    fs::zaminfs::sync(&key)
}

/// Terminalda yöllar ildizga nisbatan: `uy/a.txt` == `/uy/a.txt`; `~` — uy papkasi.
fn abs_path(p: &str) -> String {
    if p.starts_with('/') || p.starts_with('~') { String::from(p) } else { format!("/{}", p) }
}

/// Terminal yadro oqimi sifatida işlaydi. Avval birinçi sozlaş yoki kiriş.
pub extern "C" fn run(disk: usize) {
    crate::setup::run(disk != 0);
    let mut shell = Shell::new();
    shell.start();
    loop {
        while let Some(b) = uart::read_byte() {
            if let Some(line) = shell.editor.feed(b) {
                shell.execute(line.trim());
                shell.prompt();
            }
        }
        task::wait_input(uart::has_input);
    }
}

fn find(name: &str) -> Option<Cmd> {
    COMMANDS
        .iter()
        .find(|c| c.names.contains(&name) || c.aliases.contains(&name))
        .map(|c| c.cmd)
}

pub struct Shell {
    editor: LineEditor,
}

impl Shell {
    pub fn new() -> Self {
        Self { editor: LineEditor::new() }
    }

    pub fn start(&self) {
        tprintln!(msg::SH_WELCOME);
        self.prompt();
    }

    fn prompt(&self) {
        match accounts::current() {
            Some(a) => print!("{}@zamin> ", a.username),
            None => print!("{}", PROMPT),
        }
    }

    fn execute(&mut self, line: &str) {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let args = args.trim();
        if name.is_empty() {
            return;
        }
        let Some(cmd) = find(name) else {
            tprintln!(msg::SH_UNKNOWN, name);
            return;
        };
        match cmd {
            Cmd::Help => {
                let lang = i18n::current() as usize;
                for c in COMMANDS {
                    let n = c.names[lang];
                    let pad = 14usize.saturating_sub(n.chars().count());
                    println!("  {}{:pad$}— {}", n, "", c.help[lang], pad = pad);
                }
            }
            Cmd::About => {
                tprintln!(msg::SH_VERSION, env!("CARGO_PKG_VERSION"));
                tprintln!(msg::SH_ABOUT);
            }
            Cmd::Uptime => {
                let ms = timer::uptime_ms();
                let s = ms / 1000;
                tprintln!(
                    msg::SH_UPTIME,
                    s / 3600,
                    (s / 60) % 60,
                    format_args!("{}.{:03}", s % 60, ms % 1000),
                    timer::ticks()
                );
            }
            Cmd::Memory => {
                let (free, total) = frame::stats();
                let (used, htotal) = heap::stats();
                tprintln!(msg::SH_MEM, free * 4 / 1024, total * 4 / 1024, used / 1024, htotal / 1024);
                let (files, dirs, bytes) = fs::stats();
                tprintln!(msg::SH_MEM_FS, files, dirs, bytes);
            }
            Cmd::Echo => println!("{}", args),
            Cmd::Clear => print!("\x1b[2J\x1b[H"),
            Cmd::Lang => match Lang::from_code(args) {
                Some(l) => {
                    i18n::set(l);
                    fs::apply_language(l); // tizim papkalari ham yangi tilda
                    config::save_current();
                    tprintln!(msg::SH_LANG_SET, i18n::pick(&msg::LANG_NAME));
                }
                None => tprintln!(msg::SH_LANG_USAGE),
            },
            Cmd::Ps => {
                tprintln!(msg::SH_PS_HEADER);
                for t in task::snapshot() {
                    let (pid, name) = (t.pid, t.name);
                    let kind = i18n::pick(if t.user { &msg::KIND_USER } else { &msg::KIND_KERNEL });
                    let st = state_text(t.state);
                    let pk = 11usize.saturating_sub(kind.chars().count());
                    let ps = 13usize.saturating_sub(st.chars().count());
                    println!("  {:<4} {}{:pk$}{}{:ps$}{}", pid, kind, "", st, "", name, pk = pk, ps = ps);
                }
            }
            Cmd::Programs => {
                tprintln!(msg::SH_PROGRAMS_HEADER);
                let lang = i18n::current() as usize;
                for e in fs::list("/dasturlar").unwrap_or_default() {
                    let pad = 13usize.saturating_sub(e.name.chars().count());
                    println!("    {}{:pad$}— {}", e.name, "", program_help(&e.name)[lang], pad = pad);
                }
            }
            Cmd::Run => {
                let (line, background) = match args.strip_suffix('&') {
                    Some(p) => (p.trim(), true),
                    None => (args, false),
                };
                let (prog, prog_args) = line.split_once(' ').map_or((line, ""), |(p, a)| (p, a.trim()));
                if prog.is_empty() {
                    tprintln!(msg::SH_RUN_USAGE);
                    return;
                }
                let path = if prog.contains('/') { abs_path(prog) } else { format!("/dasturlar/{}", prog) };
                let image = match fs::read_all(&path) {
                    Ok(img) => img,
                    Err(FsError::NotFound) => {
                        tprintln!(msg::SH_NO_PROGRAM, prog);
                        return;
                    }
                    Err(e) => {
                        tprintln!(msg::SH_ERROR, path, fs_error_text(e));
                        return;
                    }
                };
                let name = path.rsplit('/').next().unwrap_or(prog);
                match task::spawn_user(name, &image, prog_args, background, 0) {
                    Ok(pid) if background => tprintln!(msg::SH_STARTED_BG, pid, name),
                    Ok(pid) => {
                        if let Some(code) = task::wait(pid) {
                            tprintln!(msg::SH_EXITED, pid, name, code);
                        }
                    }
                    Err(e) => tprintln!(msg::SH_SPAWN_FAIL, format_args!("{:?}", e)),
                }
            }
            Cmd::Kill => match args.parse::<usize>() {
                Ok(pid) if task::kill(pid) => tprintln!(msg::SH_KILLED, pid),
                Ok(pid) => tprintln!(msg::SH_KILL_FAIL, pid),
                Err(_) => tprintln!(msg::SH_KILL_USAGE),
            },
            Cmd::Ls => {
                let path = abs_path(args);
                match fs::list(&path) {
                    Ok(entries) if entries.is_empty() => tprintln!(msg::SH_EMPTY_DIR),
                    Ok(entries) => {
                        for e in entries {
                            let shown = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
                            let pad = 24usize.saturating_sub(shown.chars().count());
                            if e.is_dir {
                                print!("  {}{:pad$}", shown, "", pad = pad);
                                tprintln!(msg::SH_ITEMS, e.size);
                            } else {
                                println!("  {}{:pad$}{} B", shown, "", e.size, pad = pad);
                            }
                        }
                    }
                    Err(e) => tprintln!(msg::SH_ERROR, path, fs_error_text(e)),
                }
            }
            Cmd::Cat => {
                if args.is_empty() {
                    tprintln!(msg::SH_USAGE_PATH, name);
                    return;
                }
                let path = abs_path(args);
                match fs::read_all(&path) {
                    Ok(data) => match core::str::from_utf8(&data) {
                        Ok(text) => {
                            print!("{}", text);
                            if !text.is_empty() && !text.ends_with('\n') {
                                println!();
                            }
                        }
                        Err(_) => tprintln!(msg::SH_BINARY, data.len()),
                    },
                    Err(e) => tprintln!(msg::SH_ERROR, path, fs_error_text(e)),
                }
            }
            Cmd::Write => {
                let Some((file, text)) = args.split_once(' ') else {
                    tprintln!(msg::SH_WRITE_USAGE);
                    return;
                };
                let path = abs_path(file);
                let data = format!("{}\n", text);
                let res = fs::open(&path, crate::abi::OPEN_CREATE | crate::abi::OPEN_TRUNCATE)
                    .and_then(|id| fs::write(id, Some(0), data.as_bytes()));
                match res {
                    Ok(n) => tprintln!(msg::SH_WRITTEN, n, path),
                    Err(e) => tprintln!(msg::SH_ERROR, path, fs_error_text(e)),
                }
            }
            Cmd::Mkdir => {
                if args.is_empty() {
                    tprintln!(msg::SH_USAGE_PATH, name);
                    return;
                }
                let path = abs_path(args);
                match fs::mkdir(&path) {
                    Ok(_) => tprintln!(msg::SH_DIR_CREATED, path),
                    Err(e) => tprintln!(msg::SH_ERROR, path, fs_error_text(e)),
                }
            }
            Cmd::Rm => {
                if args.is_empty() {
                    tprintln!(msg::SH_USAGE_PATH, name);
                    return;
                }
                let path = abs_path(args);
                match fs::remove(&path) {
                    Ok(()) => tprintln!(msg::SH_REMOVED, path),
                    Err(e) => tprintln!(msg::SH_ERROR, path, fs_error_text(e)),
                }
            }
            Cmd::Channels => {
                let list = crate::ipc::list();
                if list.is_empty() {
                    tprintln!(msg::SH_NO_CHANNELS);
                    return;
                }
                tprintln!(msg::SH_CHANNELS_HEADER);
                for (name, queued, sent) in list {
                    let pad = 21usize.saturating_sub(name.chars().count());
                    println!("  {}{:pad$}{:<11}{}", name, "", queued, sent, pad = pad);
                }
            }
            Cmd::Whoami => match accounts::current() {
                Some(a) => println!("  {} ({})", a.username, a.full_name),
                None => tprintln!(msg::SH_NO_ACCOUNT),
            },
            Cmd::Passwd => {
                let Some(mut a) = accounts::current() else {
                    tprintln!(msg::SH_NO_ACCOUNT);
                    return;
                };
                tprint!(msg::SH_PASSWD_OLD);
                if !accounts::verify(&a, &read_line(true)) {
                    tprintln!(msg::LOGIN_FAIL);
                    return;
                }
                tprint!(msg::SH_PASSWD_NEW);
                let p1 = read_line(true);
                if p1.chars().count() < 4 {
                    tprintln!(msg::SETUP_PASS_SHORT);
                    return;
                }
                tprint!(msg::SETUP_PASS2);
                if read_line(true) != p1 {
                    tprintln!(msg::SETUP_PASS_MISMATCH);
                    return;
                }
                let Some(key) = accounts::master_key() else {
                    tprintln!(msg::SH_NO_ACCOUNT);
                    return;
                };
                tprintln!(msg::SETUP_CREATING);
                accounts::set_password(&mut a, &key, &p1);
                accounts::forget(p1);
                accounts::set_current(Some(a));
                config::save_current();
                tprintln!(msg::SH_PASSWD_CHANGED);
            }
            Cmd::Reset => {
                tprint!(msg::SH_RESET_CONFIRM);
                let answer = read_line(false);
                if !matches!(answer.trim(), "ha" | "h" | "да" | "д" | "yes" | "y") {
                    tprintln!(msg::SH_CANCELLED);
                    return;
                }
                config::wipe();
                accounts::set_master_key(None);
                tprintln!(msg::SH_RESET_DONE);
                power::reboot();
            }
            Cmd::Save => match flush() {
                Ok(n) => tprintln!(msg::SH_SAVED, n),
                Err(e) => tprintln!(msg::SH_SAVE_FAIL, format_args!("{:?}", e)),
            },
            Cmd::Security => crate::security::report(),
            Cmd::Reboot => {
                let _ = flush();
                tprintln!(msg::SH_REBOOT);
                power::reboot();
            }
            Cmd::Shutdown => {
                let _ = flush();
                tprintln!(msg::SH_SHUTDOWN);
                power::shutdown();
            }
        }
    }
}
