//! Zamin terminali — yadro içidagi oddiy buyruqlar qatori.
//! Buyruqlarni istalgan tilda yoziş mumkin: `yordam`, `помощь` yoki `help`.

use crate::i18n::{self, Lang, Text, msg};
use crate::memory::{frame, heap};
use crate::{print, println, power, timer, tprintln};
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
    Command { cmd: Cmd::Reboot, names: ["qayta", "перезагрузка", "reboot"], aliases: &[], help: ["qayta yuklaş", "перезагрузить", "restart"] },
    Command { cmd: Cmd::Shutdown, names: ["öçir", "выкл", "shutdown"], aliases: &["ocir", "ochir"], help: ["tizimni öçiriş", "выключить систему", "power off"] },
];

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
