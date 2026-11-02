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
    crate::uart::with_console(|c| {
        let mut j = crate::uart::Journaled(c);
        let _ = write_template(&mut j, pick(t), args);
        if newline {
            let _ = j.write_str("\n");
        }
    });
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
    pub const STATE_READY: Text = ["tayyor", "готов", "ready"];
    pub const STATE_RUNNING: Text = ["işlamoqda", "работает", "running"];
    pub const STATE_SLEEPING: Text = ["uxlamoqda", "спит", "sleeping"];
    pub const STATE_WAITING: Text = ["kutmoqda", "ожидает", "waiting"];
    pub const STATE_DEAD: Text = ["tugagan", "завершён", "exited"];
    pub const KIND_KERNEL: Text = ["yadro", "ядро", "kernel"];
    pub const KIND_USER: Text = ["dastur", "программа", "program"];

    pub const SH_PS_HEADER: Text = [
        "  PID  CPU  EGASI     TURI       HOLAT        NOM",
        "  PID  CPU  ВЛАДЕЛЕЦ  ТИП        СОСТОЯНИЕ    ИМЯ",
        "  PID  CPU  OWNER     TYPE       STATE        NAME",
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
        "  {} ni tugatib bölmaydi (faqat öz dasturlaringizni; administrator — hammasini)",
        "  невозможно завершить {} (только свои программы; администратор — все)",
        "  cannot terminate {} (only your own programs; administrators — any)",
    ];
    pub const SH_OWN_PROGRAMS: Text = [
        "  Siz örnatgan dasturlar (~/dasturlar):",
        "  Установленные вами (~/dasturlar):",
        "  Installed by you (~/dasturlar):",
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

    // --- Disk va sozlamalar ---
    pub const DISK_FOUND: Text = ["[disk] virtio-blk: {} MiB", "[disk] virtio-blk: {} МиБ", "[disk] virtio-blk: {} MiB"];
    pub const DISK_NONE: Text = [
        "[disk] disk topilmadi — sozlamalar saqlanmaydi",
        "[disk] диск не найден — настройки не сохраняются",
        "[disk] no disk — settings will not be saved",
    ];
    pub const CONFIG_LOADED: Text = [
        "[conf] sozlamalar diskdan öqildi (hisob: {})",
        "[conf] настройки загружены с диска (учётная запись: {})",
        "[conf] settings loaded from disk (account: {})",
    ];
    pub const CONFIG_FRESH: Text = [
        "[conf] birinçi işga tuşiriş — sozlaş boşlanadi",
        "[conf] первый запуск — начинается настройка",
        "[conf] first boot — setup will start",
    ];
    pub const TEST_SHA: Text = [
        "[test] kripto: SHA-256, HMAC, PBKDF2, ChaCha20-Poly1305 — standart vektorlar — OK",
        "[test] крипто: SHA-256, HMAC, PBKDF2, ChaCha20-Poly1305 — стандартные векторы — OK",
        "[test] crypto: SHA-256, HMAC, PBKDF2, ChaCha20-Poly1305 — standard vectors — OK",
    ];

    // --- Birinçi sozlaş va kiriş ---
    pub const SETUP_TITLE: Text = ["  Birinçi sozlaş", "  Первоначальная настройка", "  First-time setup"];
    pub const SETUP_NAME: Text = ["  Töliq ismingiz: ", "  Ваше полное имя: ", "  Your full name: "];
    pub const SETUP_NAME_BAD: Text = [
        "  Ism böş bölmasligi kerak (64 belgigaça).",
        "  Имя не может быть пустым (до 64 символов).",
        "  Name must not be empty (up to 64 characters).",
    ];
    pub const SETUP_USER: Text = [
        "  Foydalanuvçi nomi (login) [{}]: ",
        "  Имя пользователя (логин) [{}]: ",
        "  Username (login) [{}]: ",
    ];
    pub const SETUP_USER_HINT: Text = [
        "  Login: kiçik lotin harflari, raqamlar, - va _ (2–32 belgi, harf bilan boşlanadi).",
        "  Логин: строчные латинские буквы, цифры, - и _ (2–32 символа, начинается с буквы).",
        "  Login: lowercase Latin letters, digits, - and _ (2–32 characters, starts with a letter).",
    ];
    pub const SETUP_PASS: Text = ["  Parol: ", "  Пароль: ", "  Password: "];
    pub const SETUP_PASS2: Text = ["  Parolni takrorlang: ", "  Повторите пароль: ", "  Repeat password: "];
    pub const SETUP_PASS_SHORT: Text = [
        "  Parol kamida 4 belgidan iborat bölişi kerak.",
        "  Пароль должен быть не короче 4 символов.",
        "  Password must be at least 4 characters.",
    ];
    pub const SETUP_PASS_MISMATCH: Text = [
        "  Parollar mos kelmadi, qaytadan kiriting.",
        "  Пароли не совпадают, попробуйте снова.",
        "  Passwords do not match, try again.",
    ];
    pub const SETUP_CREATING: Text = ["  Hisob yaratilmoqda...", "  Создание учётной записи...", "  Creating account..."];
    pub const SETUP_DONE: Text = [
        "  Tayyor! Hisob yaratildi: {} ({})",
        "  Готово! Учётная запись создана: {} ({})",
        "  Done! Account created: {} ({})",
    ];
    pub const SETUP_HOME: Text = ["  Uy papkasi: {}", "  Домашняя папка: {}", "  Home folder: {}"];
    pub const SETUP_SAVED: Text = [
        "  Sozlamalar diskka saqlandi — keyingi safar faqat parol söraladi.",
        "  Настройки сохранены на диск — в следующий раз будет запрошен только пароль.",
        "  Settings saved to disk — next time only your password will be asked.",
    ];
    pub const SETUP_NOT_SAVED: Text = [
        "  Disk topilmadi: sozlamalar faqat şu seans uçun.",
        "  Диск не найден: настройки действуют только в этом сеансе.",
        "  No disk found: settings are kept for this session only.",
    ];
    pub const USER_ADMIN: Text = ["administrator", "администратор", "administrator"];
    pub const USER_REGULAR: Text = ["foydalanuvçi", "пользователь", "user"];
    pub const USER_NEED_ADMIN: Text = [
        "  Bu buyruq faqat administrator uçun.",
        "  Эта команда только для администратора.",
        "  This command is for administrators only.",
    ];
    pub const USER_EXISTS: Text = ["  {} allaqaçon bor.", "  {} уже существует.", "  {} already exists."];
    pub const USER_ADDED: Text = [
        "  Qöşildi: {} (uid {}), uy papkasi: {}",
        "  Добавлен: {} (uid {}), домашняя папка: {}",
        "  Added: {} (uid {}), home folder: {}",
    ];
    pub const USER_REMOVED: Text = [
        "  Öçirildi: {} (uy papkasi saqlanib qoldi)",
        "  Удалён: {} (домашняя папка сохранена)",
        "  Removed: {} (home folder kept)",
    ];
    pub const USER_NOT_SELF: Text = ["  Özingizni öçirib bölmaydi.", "  Нельзя удалить себя.", "  You cannot remove yourself."];
    pub const LOGIN_USERS: Text = ["  Foydalanuvçilar: {}", "  Пользователи: {}", "  Users: {}"];
    pub const LOGIN_USER_PROMPT: Text = ["  Foydalanuvçi nomi: ", "  Имя пользователя: ", "  Username: "];
    pub const LOGIN_NO_USER: Text = ["  Bunday foydalanuvçi yöq.", "  Нет такого пользователя.", "  No such user."];
    pub const LOGIN_WELCOME: Text = ["  Xuş kelibsiz, {}!", "  Добро пожаловать, {}!", "  Welcome, {}!"];
    pub const LOGIN_PROMPT: Text = ["  {} — parol: ", "  {} — пароль: ", "  {} — password: "];
    pub const LOGIN_FAIL: Text = ["  Notöğri parol.", "  Неверный пароль.", "  Wrong password."];

    // --- Hisob buyruqlari ---
    pub const SH_PASSWD_OLD: Text = ["  Joriy parol: ", "  Текущий пароль: ", "  Current password: "];
    pub const SH_PASSWD_NEW: Text = ["  Yangi parol: ", "  Новый пароль: ", "  New password: "];
    pub const SH_PASSWD_CHANGED: Text = ["  Parol özgartirildi.", "  Пароль изменён.", "  Password changed."];
    pub const SH_RESET_CONFIRM: Text = [
        "  Barça sozlamalar va hisob öçiriladi. Davom etilsinmi? (ha/yöq): ",
        "  Все настройки и учётная запись будут удалены. Продолжить? (да/нет): ",
        "  All settings and the account will be erased. Continue? (yes/no): ",
    ];
    pub const SH_RESET_DONE: Text = [
        "  Sozlamalar öçirildi. Qayta yuklanmoqda...",
        "  Настройки удалены. Перезагрузка...",
        "  Settings erased. Rebooting...",
    ];
    pub const SH_CANCELLED: Text = ["  Bekor qilindi.", "  Отменено.", "  Cancelled."];
    pub const SH_NO_ACCOUNT: Text = ["  Hisob yöq.", "  Нет учётной записи.", "  No account."];

    // --- Xavfsizlik va ZaminFS ---
    pub const PREV_PANIC: Text = [
        "[xavf] Oldingi işga tuşirişda yadro xatosi böldi: {}",
        "[угр.] В прошлый запуск произошла ошибка ядра: {}",
        "[risk] The kernel crashed during the previous run: {}",
    ];
    pub const SH_JOURNAL_EMPTY: Text = ["  (jurnal böş)", "  (журнал пуст)", "  (log is empty)"];
    pub const TEST_FUZZ: Text = [
        "[test] fuzz: {} ta tasodifiy tizim çaqiruvi — yadro barqaror — OK",
        "[test] fuzz: {} случайных системных вызовов — ядро устойчиво — OK",
        "[test] fuzz: {} random system calls — kernel stable — OK",
    ];
    pub const TIME_NOW: Text = ["[vaqt] {} ({})", "[время] {} ({})", "[time] {} ({})"];
    pub const SH_DATE: Text = ["  Sana va vaqt: {} ({})", "  Дата и время: {} ({})", "  Date and time: {} ({})"];
    pub const SH_TZ_SET: Text = ["  Mintaqa: {}", "  Часовой пояс: {}", "  Time zone: {}"];
    pub const SH_TZ_USAGE: Text = [
        "  Foydalaniş: mintaqa +5 (Toşkent), +3 (Moskva), 0 (UTC), +5:30",
        "  Использование: пояс +5 (Ташкент), +3 (Москва), 0 (UTC), +5:30",
        "  Usage: tz +5 (Tashkent), +3 (Moscow), 0 (UTC), +5:30",
    ];
    pub const RNG_READY: Text = [
        "[rand] tasodifiy sonlar generatori: ChaCha20, manbalar: {}",
        "[rand] генератор случайных чисел: ChaCha20, источники: {}",
        "[rand] random number generator: ChaCha20, sources: {}",
    ];
    pub const SETUP_ENCRYPTED: Text = [
        "  Fayllaringiz diskda şifrlanadi (ChaCha20-Poly1305); kalit parolingizdan olinadi.",
        "  Ваши файлы шифруются на диске (ChaCha20-Poly1305); ключ получается из пароля.",
        "  Your files are encrypted on disk (ChaCha20-Poly1305); the key is derived from your password.",
    ];
    pub const LOGIN_CHECKING: Text = ["  Tekşirilmoqda...", "  Проверка...", "  Checking..."];
    pub const ZFS_RESTORED: Text = [
        "  Fayllar diskdan tiklandi: {} ta yozuv (ZaminFS, şifrlangan)",
        "  Файлы восстановлены с диска: записей — {} (ZaminFS, зашифровано)",
        "  Files restored from disk: {} entries (ZaminFS, encrypted)",
    ];
    pub const ZFS_ERROR: Text = [
        "  Diskdagi fayllarni oçib bölmadi: {} — ular ustidan yozilmasligi uçun saqlaş töxtatildi",
        "  Не удалось открыть файлы на диске: {} — сохранение остановлено, чтобы их не перезаписать",
        "  Could not open files on disk: {} — saving is paused so they are not overwritten",
    ];
    pub const SH_SAVED: Text = [
        "  Saqlandi: {} ta blok yozildi (faqat özgarganlari, şifrlangan)",
        "  Сохранено: записано блоков — {} (только изменённые, зашифровано)",
        "  Saved: {} blocks written (only changed ones, encrypted)",
    ];
    pub const FS_CORRUPT: Text = [
        "diskdagi maʼlumot buzilgan (tekşiruvdan ötmadi)",
        "данные на диске повреждены (не прошли проверку)",
        "data on disk is corrupted (failed verification)",
    ];
    pub const FS_IO: Text = ["disk xatosi", "ошибка диска", "disk error"];
    pub const SH_SAVE_FAIL: Text = ["  Saqlab bölmadi: {}", "  Не удалось сохранить: {}", "  Save failed: {}"];
    pub const GPU_READY: Text = ["[gpu ] virtio-gpu: {}x{} ish stoli çizildi", "[gpu ] virtio-gpu: рабочий стол {}x{}", "[gpu ] virtio-gpu: desktop {}x{}"];
    pub const GPU_NONE: Text = ["[gpu ] grafik qurilma yöq (matn rejimi)", "[gpu ] нет видеоустройства (текстовый режим)", "[gpu ] no GPU (text mode)"];
    pub const FAT_MOUNTED: Text = [
        "[fat ] ikkinçi disk (FAT) /fat ga ulandi: {} ta fayl",
        "[fat ] второй диск (FAT) подключён к /fat: файлов — {}",
        "[fat ] second disk (FAT) mounted at /fat: {} files",
    ];
    pub const NET_NIC: Text = ["[net ] virtio-net: MAC {}", "[net ] virtio-net: MAC {}", "[net ] virtio-net: MAC {}"];
    pub const NET_NONE: Text = [
        "[net ] tarmoq qurilmasi topilmadi",
        "[net ] сетевое устройство не найдено",
        "[net ] no network device",
    ];
    pub const NET_READY: Text = [
        "[net ] DHCP: IP {}, gateway {}, DNS {}",
        "[net ] DHCP: IP {}, шлюз {}, DNS {}",
        "[net ] DHCP: IP {}, gateway {}, DNS {}",
    ];
    pub const NET_NO_DHCP: Text = [
        "[net ] DHCP javob bermadi (tarmoq sozlanmagan böliş mumkin)",
        "[net ] DHCP не ответил (сеть может быть не настроена)",
        "[net ] DHCP did not respond (network may be unconfigured)",
    ];
    pub const SMP_READY: Text = [
        "[smp ] {} ta protsessor yadrosi işlamoqda",
        "[smp ] работает ядер процессора: {}",
        "[smp ] {} CPU cores online",
    ];
    pub const TEST_SMP: Text = [
        "[test] SMP: oqimlar {} ta yadroda parallel işladi — OK",
        "[test] SMP: потоки параллельно работали на {} ядрах — OK",
        "[test] SMP: threads ran in parallel on {} cores — OK",
    ];
    pub const TEST_MUTEX: Text = [
        "[test] mutex: 4 oqim, uxlab kutiş, natija {} — OK",
        "[test] mutex: 4 потока, ожидание во сне, результат {} — OK",
        "[test] mutex: 4 threads, sleeping waits, result {} — OK",
    ];
    pub const TEST_DISK_IRQ: Text = [
        "[test] disk: söröv uziliş orqali tugadi (vazifa uxlab kutdi) — OK",
        "[test] диск: запрос завершён по прерыванию (задача ждала во сне) — OK",
        "[test] disk: request completed via interrupt (task slept) — OK",
    ];
    pub const TEST_STACK_GUARD: Text = [
        "[test] yadro steki qörğoni: töliş aniqlandi, faqat aybdor oqim tugatildi — OK",
        "[test] защита стека ядра: переполнение обнаружено, завершён только виновный поток — OK",
        "[test] kernel stack guard: overflow detected, only the faulty thread terminated — OK",
    ];
    pub const STACK_OVERFLOW: Text = [
        "[xavf] yadro steki töldi (PID {}) — oqim tugatildi",
        "[угр.] переполнение стека ядра (PID {}) — поток завершён",
        "[risk] kernel stack overflow (PID {}) — thread terminated",
    ];
    pub const TEST_PAN: Text = [
        "[test] PAN: yadro foydalanuvçi xotirasiga faqat nusxalaş orqali tegadi — OK",
        "[test] PAN: ядро обращается к памяти пользователя только через копирование — OK",
        "[test] PAN: kernel touches user memory only through copy routines — OK",
    ];
    pub const TEST_FAT_WRITE: Text = [
        "[test] FAT yoziş: fayl yaratildi va qayta öqildi (round-trip) — OK",
        "[test] запись FAT: файл создан и прочитан обратно (round-trip) — OK",
        "[test] FAT write: file created and read back (round-trip) — OK",
    ];
    pub const TEST_KASLR: Text = [
        "[test] KASLR: yadro steklari har yuklanişda {} xil joydan birida — OK",
        "[test] KASLR: стеки ядра при каждой загрузке в одном из {} мест — OK",
        "[test] KASLR: kernel stacks placed in one of {} spots each boot — OK",
    ];
    pub const TEST_IMAGE: Text = [
        "[test] rasm dekoderlari: {} ta format (raster + vektor: SVG, PDF, EPS, DXF…) — OK",
        "[test] декодеры изображений: {} форматов (raster + vektor: SVG, PDF, EPS, DXF…) — OK",
        "[test] image decoders: {} formats (raster + vektor: SVG, PDF, EPS, DXF…) — OK",
    ];
    pub const TEST_X509: Text = [
        "[test] X.509: sertifikat tahlili, amal muddati va host nomi tekşiruvi — OK",
        "[test] X.509: разбор сертификата, проверка срока и имени хоста — OK",
        "[test] X.509: certificate parsing, validity and hostname checks — OK",
    ];
    pub const TLS_CERT_OK: Text = [
        "[tls ] sertifikat qabul qilindi: {} (muddat va host nomi tekşirildi)",
        "[tls ] сертификат принят: {} (срок и имя хоста проверены)",
        "[tls ] certificate accepted: {} (validity and hostname checked)",
    ];
    pub const TLS_CERT_BAD: Text = [
        "[tls ] sertifikat rad etildi: {} — ulaniş böldi",
        "[tls ] сертификат отклонён: {} — соединение прервано",
        "[tls ] certificate rejected: {} — connection aborted",
    ];
    pub const SEC_TITLE: Text = ["  Zamin OS himoyasi:", "  Защита Zamin OS:", "  Zamin OS protection:"];
    pub const SEC_ON: Text = ["yoqiq", "включено", "on"];
    pub const SEC_OFF: Text = ["yöq (protsessor qöllamaydi)", "нет (не поддерживается процессором)", "off (not supported by CPU)"];
    pub const SEC_WX: Text = ["W^X (yadro kodi yozilmaydi, maʼlumot bajarilmaydi)", "W^X (код ядра не записывается, данные не исполняются)", "W^X (kernel code not writable, data not executable)"];
    pub const SEC_WXN: Text = [
        "WXN (apparat: yoziladigan xotira bajarilmaydi)",
        "WXN (аппаратно: записываемая память не исполняется)",
        "WXN (hardware: writable memory never executes)",
    ];
    pub const SEC_PAN: Text = [
        "PAN (yadro foydalanuvçi xotirasiga faqat nusxalaş orqali tegadi)",
        "PAN (ядро обращается к памяти пользователя только через копирование)",
        "PAN (kernel touches user memory only through copy routines)",
    ];
    pub const SEC_GUARD: Text = [
        "Yadro steklari qörğon bilan (töliş aniqlanadi)",
        "Стеки ядра с охранной зоной (переполнение обнаруживается)",
        "Kernel stacks with guard zones (overflow detected)",
    ];
    pub const SEC_ASLR: Text = [
        "ASLR (dastur steki va heap manzili tasodifiy)",
        "ASLR (адреса стека и кучи программ случайны)",
        "ASLR (program stack and heap addresses randomized)",
    ];
    pub const SEC_ZERO: Text = [
        "Böşatilgan xotira nollanadi",
        "Освобождённая память обнуляется",
        "Freed memory is zeroed",
    ];
    pub const SEC_ISOLATION: Text = ["Jarayonlar: alohida manzil maydoni, EL0", "Процессы: отдельное адресное пространство, EL0", "Processes: separate address spaces, EL0"];
    pub const SEC_UCOPY: Text = [
        "Foydalanuvçi bergan har bir manzil sahifa jadvali böyiça tekşiriladi",
        "Каждый адрес от пользователя проверяется по таблице страниц",
        "Every user pointer is checked against the page tables",
    ];
    pub const SEC_RNG: Text = ["Tasodifiy sonlar (CSPRNG)", "Случайные числа (CSPRNG)", "Random numbers (CSPRNG)"];
    pub const SEC_DISK: Text = ["Disk şifrlaş", "Шифрование диска", "Disk encryption"];
    pub const SEC_DISK_ON: Text = [
        "ChaCha20-Poly1305, kalit: PBKDF2-SHA256 ({} takror)",
        "ChaCha20-Poly1305, ключ: PBKDF2-SHA256 ({} итераций)",
        "ChaCha20-Poly1305, key: PBKDF2-SHA256 ({} iterations)",
    ];
    pub const SEC_DISK_STATE: Text = [
        "ZaminFS 2: avlod {}, superblok {}, bloklar {} / {} (4 KiB)",
        "ZaminFS 2: поколение {}, суперблок {}, блоки {} / {} (4 КиБ)",
        "ZaminFS 2: generation {}, superblock {}, blocks {} / {} (4 KiB)",
    ];
    pub const SEC_PASSWORD: Text = ["Parol diskda saqlanmaydi", "Пароль на диске не хранится", "Password is never stored"];
}
