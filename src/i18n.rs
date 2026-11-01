//! Köp tillilik: özbekça (asosiy), русский, English.
//! Har bir xabar — `[uz, ru, en]` uçligi. Joriy til ixtiyoriy payt almaştirilişi mumkin.
//! Özbek matnlari 2026-yilgi yangi lotin alifbosida: Ş, Ç, Ö, Ğ va tutuq belgisi ʼ.

use core::fmt::{self, Display, Write};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    Uz = 0,
    Ru = 1,
    En = 2,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "uz" | "öz" => Some(Lang::Uz),
            "ru" => Some(Lang::Ru),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(Lang::Uz as u8);

pub fn current() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::Ru,
        2 => Lang::En,
        _ => Lang::Uz,
    }
}

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// Bitta xabarning uç tildagi matni: `[uz, ru, en]`.
pub type Text = [&'static str; 3];

pub fn pick(t: &Text) -> &'static str {
    t[current() as usize]
}

/// Şablondagi har bir `{}` ni navbatdagi argument bilan almaştiradi.
pub fn write_template(w: &mut impl Write, tpl: &str, args: &[&dyn Display]) -> fmt::Result {
    let mut rest = tpl;
    let mut i = 0;
    while let Some(pos) = rest.find("{}") {
        w.write_str(&rest[..pos])?;
        if let Some(a) = args.get(i) {
            write!(w, "{}", a)?;
        }
        i += 1;
        rest = &rest[pos + 2..];
    }
    w.write_str(rest)
}

#[doc(hidden)]
pub fn _tprint(t: &Text, args: &[&dyn Display], newline: bool) {
    let mut c = crate::uart::console();
    let _ = write_template(&mut c, pick(t), args);
    if newline {
        let _ = c.write_str("\n");
    }
}

/// Joriy tilda çiqaradi: `tprintln!(msg::RAM, a, b)`.
#[macro_export]
macro_rules! tprintln {
    ($t:expr $(, $a:expr)* $(,)?) => {
        $crate::i18n::_tprint(&$t, &[$(&$a as &dyn core::fmt::Display),*], true)
    };
}

#[macro_export]
macro_rules! tprint {
    ($t:expr $(, $a:expr)* $(,)?) => {
        $crate::i18n::_tprint(&$t, &[$(&$a as &dyn core::fmt::Display),*], false)
    };
}

/// Barça xabarlar.
pub mod msg {
    use super::Text;

    pub const LANG_NAME: Text = ["Özbekça", "Русский", "English"];

    // --- Yuklaniş ---
    pub const DEVELOPER: Text = [
        "Dasturçi: Abbosjon Dev | zamin.app | info@zamin.app",
        "Разработчик: Abbosjon Dev | zamin.app | info@zamin.app",
        "Developer: Abbosjon Dev | zamin.app | info@zamin.app",
    ];
    pub const BOOT_LANG: Text = ["[til ] Til: {}", "[язык] Язык: {}", "[lang] Language: {}"];
    pub const BOOT_EL: Text = [
        "[boot] Imtiyoz darajasi: EL{}",
        "[boot] Уровень привилегий: EL{}",
        "[boot] Privilege level: EL{}",
    ];
    pub const DTB_MISSING: Text = [
        "[dtb ] qurilma tavsifi topilmadi, standart RAM hajmi işlatiladi",
        "[dtb ] описание устройства не найдено, используется размер ОЗУ по умолчанию",
        "[dtb ] device tree not found, using default RAM size",
    ];
    pub const DTB_OK: Text = [
        "[dtb ] qurilma tavsifi öqildi ({} KiB)",
        "[dtb ] описание устройства прочитано ({} КиБ)",
        "[dtb ] device tree loaded ({} KiB)",
    ];
    pub const MEM_RAM: Text = [
        "[mem ] RAM: {}..{} ({} MiB)",
        "[mem ] ОЗУ: {}..{} ({} МиБ)",
        "[mem ] RAM: {}..{} ({} MiB)",
    ];
    pub const MEM_KERNEL: Text = [
        "[mem ] yadro: {}..{} ({} KiB)",
        "[mem ] ядро: {}..{} ({} КиБ)",
        "[mem ] kernel: {}..{} ({} KiB)",
    ];
    pub const MMU_ON: Text = [
        "[mmu ] yoqildi: 4 KiB sahifalar, {}-bit VA, keşlar yoqiq",
        "[mmu ] включён: страницы 4 КиБ, {}-битные VA, кэши включены",
        "[mmu ] enabled: 4 KiB pages, {}-bit VA, caches on",
    ];
    pub const MEM_FRAMES: Text = [
        "[mem ] sahifalar: {} böş / {} jami",
        "[mem ] страницы: {} свободно / {} всего",
        "[mem ] pages: {} free / {} total",
    ];
    pub const HEAP_READY: Text = ["[heap] {} MiB tayyor", "[heap] {} МиБ готово", "[heap] {} MiB ready"];
    pub const IRQ_READY: Text = [
        "[irq ] GIC: {} ta uziliş liniyasi, taymer: {} Hz (çastota {} MHz)",
        "[irq ] GIC: линий прерываний — {}, таймер: {} Гц (частота {} МГц)",
        "[irq ] GIC: {} interrupt lines, timer: {} Hz (frequency {} MHz)",
    ];
    pub const IRQ_UNKNOWN: Text = [
        "[irq ] nomaʼlum uziliş: {}",
        "[irq ] неизвестное прерывание: {}",
        "[irq ] unknown interrupt: {}",
    ];
    pub const EXC_BRK: Text = [
        "[exc ] BRK #{} @ {} — qayta işlandi, davom etamiz",
        "[exc ] BRK #{} @ {} — обработано, продолжаем",
        "[exc ] BRK #{} @ {} — handled, continuing",
    ];
    pub const PANIC: Text = ["*** YADRO XATOSI ***", "*** ОШИБКА ЯДРА ***", "*** KERNEL PANIC ***"];

    // --- Öz-özini tekşiriş ---
    pub const TEST_HEAP: Text = [
        "[test] heap: Box/Vec/String va 3200 ajratiş — OK",
        "[test] куча: Box/Vec/String и 3200 выделений — OK",
        "[test] heap: Box/Vec/String and 3200 allocations — OK",
    ];
    pub const TEST_MMU: Text = [
        "[test] mmu: {} -> {} xaritalaş, yoziş va olib taşlaş — OK",
        "[test] mmu: {} -> {} отображение, запись и снятие — OK",
        "[test] mmu: {} -> {} map, write and unmap — OK",
    ];
    pub const TEST_WX: Text = [
        "[test] W^X: yadro kodi — faqat öqiş, maʼlumotlar — bajarib bölmaydi — OK",
        "[test] W^X: код ядра только для чтения, данные неисполняемые — OK",
        "[test] W^X: kernel code read-only, data non-executable — OK",
    ];
    pub const TEST_TIMER: Text = [
        "[test] taymer: 50 ms da {} ta uziliş — OK",
        "[test] таймер: {} прерываний за 50 мс — OK",
        "[test] timer: {} interrupts in 50 ms — OK",
    ];

    // --- Zamin terminali ---
    pub const SH_WELCOME: Text = [
        "Zamin terminali. Buyruqlar röyxati uçun: yordam",
        "Терминал Zamin. Список команд: помощь",
        "Zamin terminal. For a list of commands: help",
    ];
    pub const SH_UNKNOWN: Text = [
        "  nomaʼlum buyruq: {} (yordam deb yozing)",
        "  неизвестная команда: {} (введите: помощь)",
        "  unknown command: {} (type: help)",
    ];
    pub const SH_VERSION: Text = ["  Zamin OS v{} (AArch64)", "  Zamin OS v{} (AArch64)", "  Zamin OS v{} (AArch64)"];
    pub const SH_ABOUT: Text = [
        "  Dasturçi:  Abbosjon Dev\n  Veb-sayt:  zamin.app\n  Email:     info@zamin.app",
        "  Разработчик: Abbosjon Dev\n  Веб-сайт:    zamin.app\n  Эл. почта:   info@zamin.app",
        "  Developer: Abbosjon Dev\n  Website:   zamin.app\n  Email:     info@zamin.app",
    ];
    pub const SH_UPTIME: Text = [
        "  {} soat {} daqiqa {} soniya (taymer uzilişlari: {})",
        "  {} ч {} мин {} с (прерываний таймера: {})",
        "  {} h {} min {} s (timer interrupts: {})",
    ];
    pub const SH_MEM: Text = [
        "  RAM:  {} MiB böş / {} MiB jami\n  Heap: {} KiB işlatilgan / {} KiB",
        "  ОЗУ:  {} МиБ свободно / {} МиБ всего\n  Куча: {} КиБ занято / {} КиБ",
        "  RAM:  {} MiB free / {} MiB total\n  Heap: {} KiB used / {} KiB",
    ];
    pub const SH_LANG_SET: Text = ["  Til: {}", "  Язык: {}", "  Language: {}"];
    pub const SH_LANG_USAGE: Text = [
        "  Foydalaniş: til uz | ru | en",
        "  Использование: язык uz | ru | en",
        "  Usage: lang uz | ru | en",
    ];
    pub const SH_REBOOT: Text = ["Qayta yuklanmoqda...", "Перезагрузка...", "Rebooting..."];
    pub const SH_SHUTDOWN: Text = [
        "Tizim öçirilmoqda. Xayr!",
        "Выключение. До свидания!",
        "Shutting down. Goodbye!",
    ];

    // --- Jarayonlar ---
    pub const SCHED_READY: Text = [
        "[task] rejalaştiruvçi işga tuşdi (vaqt böligi {} ms), dasturlar: {} ta",
        "[task] планировщик запущен (квант {} мс), программ: {}",
        "[task] scheduler started (time slice {} ms), programs: {}",
    ];
    pub const TEST_SCHED: Text = [
        "[test] rejalaştiruvçi: 2 ta oqim bir vaqtda işladi ({} / {} aylanish) — OK",
        "[test] планировщик: 2 потока работали одновременно ({} / {} итераций) — OK",
        "[test] scheduler: 2 threads ran concurrently ({} / {} iterations) — OK",
    ];
    pub const TEST_USER: Text = [
        "[test] foydalanuvçi rejimi (EL0): tizim çaqiruvlari va himoya, çiqiş kodi {} — OK",
        "[test] режим пользователя (EL0): системные вызовы и защита, код выхода {} — OK",
        "[test] user mode (EL0): system calls and protection, exit code {} — OK",
    ];
    pub const PROC_FAULT: Text = [
        "[jarayon {}] himoya xatosi: {} manzilga ruxsatsiz murojaat (EC {}) — jarayon tugatildi",
        "[процесс {}] ошибка защиты: недопустимый доступ к адресу {} (EC {}) — процесс завершён",
        "[process {}] protection fault: illegal access to address {} (EC {}) — process terminated",
    ];
    pub const PROC_BAD_SYSCALL: Text = [
        "[jarayon {}] nomaʼlum tizim çaqiruvi: {}",
        "[процесс {}] неизвестный системный вызов: {}",
        "[process {}] unknown system call: {}",
    ];
    pub const STATE_READY: Text = ["tayyor", "готов", "ready"];
    pub const STATE_RUNNING: Text = ["işlamoqda", "работает", "running"];
    pub const STATE_SLEEPING: Text = ["uxlamoqda", "спит", "sleeping"];
    pub const STATE_WAITING: Text = ["kutmoqda", "ожидает", "waiting"];
    pub const STATE_DEAD: Text = ["tugagan", "завершён", "exited"];
    pub const KIND_KERNEL: Text = ["yadro", "ядро", "kernel"];
    pub const KIND_USER: Text = ["dastur", "программа", "program"];

    pub const SH_PS_HEADER: Text = [
        "  PID  TURI       HOLAT        NOM",
        "  PID  ТИП        СОСТОЯНИЕ    ИМЯ",
        "  PID  TYPE       STATE        NAME",
    ];
    pub const SH_EXITED: Text = [
        "  [{} {} tugadi, çiqiş kodi: {}]",
        "  [{} {} завершён, код выхода: {}]",
        "  [{} {} exited, code: {}]",
    ];
    pub const SH_STARTED_BG: Text = [
        "  [{} {} fonda işga tuşirildi]",
        "  [{} {} запущен в фоне]",
        "  [{} {} started in background]",
    ];
    pub const SH_NO_PROGRAM: Text = [
        "  bunday dastur yöq: {} (dasturlar deb yozing)",
        "  нет такой программы: {} (введите: программы)",
        "  no such program: {} (type: programs)",
    ];
    pub const SH_RUN_USAGE: Text = [
        "  Foydalaniş: işlat <dastur> [argumentlar] [&]   (& — fonda işlatiş)",
        "  Использование: запуск <программа> [аргументы] [&]   (& — в фоне)",
        "  Usage: run <program> [arguments] [&]   (& — in background)",
    ];
    pub const SH_SPAWN_FAIL: Text = [
        "  dasturni işga tuşirib bölmadi: {}",
        "  не удалось запустить программу: {}",
        "  failed to start program: {}",
    ];
    pub const SH_KILL_USAGE: Text = ["  Foydalaniş: tugat <PID>", "  Использование: завершить <PID>", "  Usage: kill <PID>"];
    pub const SH_KILLED: Text = ["  {} tugatildi", "  {} завершён", "  {} terminated"];
    pub const SH_KILL_FAIL: Text = [
        "  {} ni tugatib bölmaydi (faqat fondagi dasturlarni tugatiş mumkin)",
        "  невозможно завершить {} (можно завершать только фоновые программы)",
        "  cannot terminate {} (only background programs can be terminated)",
    ];
    pub const SH_PROGRAMS_HEADER: Text = [
        "  Dasturlar (işlatiş: işlat <nom>):",
        "  Программы (запуск: запуск <имя>):",
        "  Programs (start: run <name>):",
    ];

    // --- Fayl tizimi ---
    pub const FS_NOT_FOUND: Text = ["topilmadi", "не найдено", "not found"];
    pub const FS_EXISTS: Text = ["allaqaçon mavjud", "уже существует", "already exists"];
    pub const FS_NOT_DIR: Text = ["papka emas", "не папка", "not a directory"];
    pub const FS_IS_DIR: Text = ["bu papka", "это папка", "is a directory"];
    pub const FS_NOT_EMPTY: Text = ["papka böş emas", "папка не пуста", "directory not empty"];
    pub const FS_INVALID: Text = ["notöğri nom", "неверное имя", "invalid name"];
    pub const FS_TOO_BIG: Text = ["juda katta", "слишком большой", "too big"];
    pub const FS_DENIED: Text = [
        "ruxsat yöq (tizim fayli)",
        "доступ запрещён (системный файл)",
        "permission denied (system file)",
    ];
    pub const FS_READY: Text = [
        "[fs  ] fayl tizimi tayyor: {} ta fayl, {} ta papka",
        "[fs  ] файловая система готова: файлов — {}, папок — {}",
        "[fs  ] file system ready: {} files, {} directories",
    ];
    pub const SH_ERROR: Text = ["  xato: {}: {}", "  ошибка: {}: {}", "  error: {}: {}"];
    pub const SH_USAGE_PATH: Text = ["  Foydalaniş: {} <yöl>", "  Использование: {} <путь>", "  Usage: {} <path>"];
    pub const SH_WRITE_USAGE: Text = [
        "  Foydalaniş: yoz <fayl> <matn>",
        "  Использование: записать <файл> <текст>",
        "  Usage: write <file> <text>",
    ];
    pub const SH_WRITTEN: Text = ["  {} bayt yozildi: {}", "  записано {} байт: {}", "  wrote {} bytes: {}"];
    pub const SH_DIR_CREATED: Text = ["  papka yaratildi: {}", "  папка создана: {}", "  directory created: {}"];
    pub const SH_REMOVED: Text = ["  öçirildi: {}", "  удалено: {}", "  removed: {}"];
    pub const SH_BINARY: Text = ["  (ikkilik fayl, {} bayt)", "  (двоичный файл, {} байт)", "  (binary file, {} bytes)"];
    pub const SH_EMPTY_DIR: Text = ["  (böş papka)", "  (пустая папка)", "  (empty directory)"];
    pub const SH_ITEMS: Text = ["{} ta", "{} шт.", "{} items"];
    pub const SH_MEM_FS: Text = [
        "  Fayllar: {} ta fayl, {} ta papka, {} bayt",
        "  Файлы: файлов — {}, папок — {}, {} байт",
        "  Files: {} files, {} directories, {} bytes",
    ];
    pub const SH_CHANNELS_HEADER: Text = [
        "  KANAL                NAVBATDA   YUBORILGAN",
        "  КАНАЛ                В ОЧЕРЕДИ  ОТПРАВЛЕНО",
        "  CHANNEL              QUEUED     SENT",
    ];
    pub const SH_NO_CHANNELS: Text = ["  hozirça kanallar yöq", "  каналов пока нет", "  no channels yet"];
}
