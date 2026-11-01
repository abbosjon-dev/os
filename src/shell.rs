//! Zamin terminali — yadro içidagi oddiy buyruqlar qatori.
//! Buyruqlarni istalgan tilda yoziş mumkin: `yordam`, `помощь` yoki `help`.

use crate::i18n::{self, Lang, Text, msg};
use crate::memory::{frame, heap};
use crate::task::{self, State};
use crate::{print, println, power, timer, tprintln, uart};
use alloc::string::String;

const PROMPT: &str = "zamin> ";
const MAX_LINE: usize = 256;

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
    Command { cmd: Cmd::Reboot, names: ["qayta", "перезагрузка", "reboot"], aliases: &[], help: ["qayta yuklaş", "перезагрузить", "restart"] },
    Command { cmd: Cmd::Shutdown, names: ["öçir", "выкл", "shutdown"], aliases: &["ocir", "ochir"], help: ["tizimni öçiriş", "выключить систему", "power off"] },
];

/// Dasturlar tavsifi: [uz, ru, en]
fn program_help(name: &str) -> Text {
    match name {
        "salom" => ["salomlaşadi", "приветствует", "says hello"],
        "hisobla" => [
            "tub sonlarni sanaydi (köp vazifalilikni körsatadi)",
            "считает простые числа (показывает многозадачность)",
            "counts primes (shows multitasking)",
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
        _ => ["", "", ""],
    }
}

fn state_text(st: State) -> &'static str {
    i18n::pick(match st {
        State::Ready => &msg::STATE_READY,
        State::Running => &msg::STATE_RUNNING,
        State::Sleeping(_) => &msg::STATE_SLEEPING,
        State::WaitingInput | State::WaitingPid(_) => &msg::STATE_WAITING,
        State::Dead(_) => &msg::STATE_DEAD,
    })
}

/// Terminal yadro oqimi sifatida işlaydi.
pub extern "C" fn run(_: usize) {
    let mut shell = Shell::new();
    shell.start();
    loop {
        while let Some(b) = uart::read_byte() {
            shell.feed(b);
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
    line: String,
    /// Yarim kelgan UTF-8 belgi baytlari (ö, ş, кириллица...)
    utf8: [u8; 4],
    utf8_len: usize,
    utf8_need: usize,
}

impl Shell {
    pub fn new() -> Self {
        Self { line: String::new(), utf8: [0; 4], utf8_len: 0, utf8_need: 0 }
    }

    pub fn start(&self) {
        tprintln!(msg::SH_WELCOME);
        print!("{}", PROMPT);
    }

    /// Klaviaturadan kelgan bitta baytni qayta işlaydi.
    pub fn feed(&mut self, b: u8) {
        if self.utf8_need > 0 {
            if b & 0xc0 != 0x80 {
                self.utf8_need = 0; // buzilgan ketma-ketlik — taşlab yuboramiz
            } else {
                self.utf8[self.utf8_len] = b;
                self.utf8_len += 1;
                if self.utf8_len == self.utf8_need {
                    self.utf8_need = 0;
                    let (buf, len) = (self.utf8, self.utf8_len);
                    if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                        self.insert(s);
                    }
                }
                return;
            }
        }
        match b {
            b'\r' | b'\n' => {
                println!();
                let line = core::mem::take(&mut self.line);
                self.execute(line.trim());
                print!("{}", PROMPT);
            }
            0x7f | 0x08 => {
                if self.line.pop().is_some() {
                    print!("\x08 \x08");
                }
            }
            0x20..=0x7e => {
                let buf = [b];
                self.insert(unsafe { core::str::from_utf8_unchecked(&buf) });
            }
            0xc0..=0xf7 => {
                self.utf8[0] = b;
                self.utf8_len = 1;
                self.utf8_need = match b {
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
            }
            _ => {} // boşqaruv belgilari (strelkalar va h.k.) hozirça eʼtiborsiz
        }
    }

    fn insert(&mut self, s: &str) {
        if self.line.len() + s.len() <= MAX_LINE {
            self.line.push_str(s);
            print!("{}", s);
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
            }
            Cmd::Echo => println!("{}", args),
            Cmd::Clear => print!("\x1b[2J\x1b[H"),
            Cmd::Lang => match Lang::from_code(args) {
                Some(l) => {
                    i18n::set(l);
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
                for (name, _) in task::programs::PROGRAMS {
                    let pad = 10usize.saturating_sub(name.len());
                    println!("    {}{:pad$}— {}", name, "", program_help(name)[lang], pad = pad);
                }
            }
            Cmd::Run => {
                let (prog, background) = match args.strip_suffix('&') {
                    Some(p) => (p.trim(), true),
                    None => (args, false),
                };
                if prog.is_empty() {
                    tprintln!(msg::SH_RUN_USAGE);
                    return;
                }
                let Some(image) = task::programs::find(prog) else {
                    tprintln!(msg::SH_NO_PROGRAM, prog);
                    return;
                };
                match task::spawn_user(prog, image, background) {
                    Ok(pid) if background => tprintln!(msg::SH_STARTED_BG, pid, prog),
                    Ok(pid) => {
                        if let Some(code) = task::wait(pid) {
                            tprintln!(msg::SH_EXITED, pid, prog, code);
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
            Cmd::Reboot => {
                tprintln!(msg::SH_REBOOT);
                power::reboot();
            }
            Cmd::Shutdown => {
                tprintln!(msg::SH_SHUTDOWN);
                power::shutdown();
            }
        }
    }
}
