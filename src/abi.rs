// Zamin tizim çaqiruvlari (ABI). Bu fayl yadro va foydalanuvçi kutubxonasi
// tomonidan birga işlatiladi (`include!`), şuning uçun raqamlar hamişa mos.
//
// Çaqiriş: `svc #0`, x8 = çaqiruv raqami, x0..x3 = argumentlar,
// natija x0 da. Manfiy natija — xato (quyidagi ERR_* kodlari).
// Matnlar (yöl, nom) hamişa "manzil + uzunlik" juftligi sifatida beriladi.

// --- Jarayon ---
/// Jarayonni tugatadi. x0 = çiqiş kodi.
pub const SYS_EXIT: u64 = 0;
/// Standart çiqişga (1-dastak: konsol, quvur yoki fayl) yozadi. x0 = manzil, x1 = uzunlik.
pub const SYS_WRITE: u64 = 1;
/// Protsessorni boşqa jarayonga böşatib beradi.
pub const SYS_YIELD: u64 = 2;
/// Uxlaydi. x0 = millisekund.
pub const SYS_SLEEP: u64 = 3;
/// Tizim işga tuşganidan beri ötgan vaqt (ms).
pub const SYS_UPTIME: u64 = 4;
/// Joriy jarayon raqami (PID).
pub const SYS_PID: u64 = 5;
/// Joriy til: 0 = özbekça, 1 = русский, 2 = English.
pub const SYS_LANG: u64 = 6;
/// Xotira oladi. x0 = bayt. Natija: yangi (nollangan) xotira manzili.
pub const SYS_MAP: u64 = 7;
/// Dasturni işga tuşiradi. x0,x1 = fayl yöli; x2,x3 = argumentlar;
/// x4 = standart dastaklar (qarang: INHERIT). Natija: PID.
pub const SYS_SPAWN: u64 = 8;
/// Bola jarayon tugaşini kutadi. x0 = PID, x1 = 1 — kutmasdan tekşiriş.
/// Natija: uning çiqiş kodi (yoki ERR_RUNNING).
pub const SYS_WAIT: u64 = 9;

// --- Fayllar ---
/// Faylni oçadi. x0,x1 = yöl; x2 = bayroqlar (OPEN_*). Natija: dastak (handle).
pub const SYS_OPEN: u64 = 10;
/// Dastakdan öqiydi (fayl, quvur yoki konsol — konsoldan bitta qator). x0 = dastak,
/// x1 = bufer, x2 = siğim. Natija: öqilgan baytlar (0 = oxiri).
pub const SYS_READ: u64 = 11;
/// Dastakka yozadi (fayl, quvur yoki konsol). x0 = dastak, x1 = manzil, x2 = uzunlik.
pub const SYS_FWRITE: u64 = 12;
/// Dastakni yopadi. x0 = dastak.
pub const SYS_CLOSE: u64 = 13;
/// Papka tarkibi. x0,x1 = yöl; x2,x3 = bufer. Natija: "nom\n" lar uzunligi
/// (papkalar nomi oxirida "/").
pub const SYS_READDIR: u64 = 14;
/// Papka yaratadi. x0,x1 = yöl.
pub const SYS_MKDIR: u64 = 15;
/// Fayl yoki böş papkani öçiradi. x0,x1 = yöl.
pub const SYS_REMOVE: u64 = 16;

// --- Zamin kanallari (IPC) ---
/// Nomli kanalni oçadi (yöq bölsa — yaratadi). x0,x1 = nom. Natija: dastak.
pub const SYS_CHAN_OPEN: u64 = 17;
/// Kanalga xabar yuboradi. x0 = dastak, x1 = manzil, x2 = uzunlik.
pub const SYS_CHAN_SEND: u64 = 18;
/// Kanaldan xabar oladi (kelgunça kutadi). x0 = dastak, x1 = bufer, x2 = siğim.
/// Natija: xabar uzunligi.
pub const SYS_CHAN_RECV: u64 = 19;

// --- Kiriş/çiqiş, muhit, terminal ---
/// Quvur yaratadi. x0 = [u64; 2] bufer: (öqiş dastagi, yoziş dastagi).
pub const SYS_PIPE: u64 = 20;
/// Muhit özgaruvçisini örnatadi. x0,x1 = nom; x2,x3 = qiymat (böş — öçiriladi).
pub const SYS_SETENV: u64 = 21;
/// Muhit özgaruvçisini öqiydi. x0,x1 = nom; x2,x3 = bufer. Natija: qiymat uzunligi.
pub const SYS_GETENV: u64 = 22;
/// Konsolning oldingi plandagi jarayoni (Ctrl+C uni töxtatadi). x0 = PID (0 — heç kim).
pub const SYS_FOREGROUND: u64 = 23;
/// Tizim buyruği (faqat işonçli terminal uçun). x0,x1 = buyruq qatori.
pub const SYS_ADMIN: u64 = 24;
/// Joriy papkani özgartiradi. x0,x1 = yöl.
pub const SYS_CHDIR: u64 = 25;

// --- Oqimlar ---
/// Yangi oqim. x0 = kiriş manzili, x1 = stek tepasi, x2 = argument. Natija: oqim raqami (TID).
pub const SYS_THREAD_CREATE: u64 = 26;
/// Faqat joriy oqimni tugatadi. x0 = kod. (SYS_EXIT — butun jarayonni.)
pub const SYS_THREAD_EXIT: u64 = 27;
/// Oqim tugaşini kutadi. x0 = TID. Natija: uning kodi.
pub const SYS_THREAD_JOIN: u64 = 28;
/// Futex: manzildagi u32 hali `x1` ga teng bölsa — uxlaydi. x0 = manzil, x1 = kutilgan qiymat.
pub const SYS_FUTEX_WAIT: u64 = 29;
/// Futex: şu manzilni kutayotgan oqimlarni uyğotadi. x0 = manzil.
pub const SYS_FUTEX_WAKE: u64 = 30;
/// Joriy oqim raqami (TID).
pub const SYS_TID: u64 = 31;
/// Ruxsatlarni özgartiradi. x0,x1 = yöl; x2 = ruxsat (sakkizlik, masalan 0o644).
pub const SYS_CHMOD: u64 = 32;
/// Nomsiz kanal yaratadi (faqat dastak orqali beriladi). Natija: dastak.
pub const SYS_CHAN_CREATE: u64 = 33;

// --- Tarmoq ---
/// Tarmoq holati. x0 = 23-baytli bufer: ip[4] gw[4] mask[4] dns[4] mac[6] configured[1].
pub const SYS_NET_INFO: u64 = 36;
/// DNS: domen → IP. x0,x1 = nom; x2 = 4-baytli bufer. Natija: 0 yoki xato.
pub const SYS_DNS: u64 = 37;
/// Ping. x0 = IP (4 bayt bufer), x1 = ketma-ketlik raqami. Natija: ms yoki -1.
pub const SYS_PING: u64 = 38;
/// TCP ulaniş. x0 = IP (4 bayt), x1 = port. Natija: ulaniş raqami (handle) yoki xato.
pub const SYS_TCP_CONNECT: u64 = 39;
/// TCP jönatiş. x0 = ulaniş, x1 = manzil, x2 = uzunlik. Natija: 0 yoki xato.
pub const SYS_TCP_SEND: u64 = 40;
/// TCP oliş. x0 = ulaniş, x1 = bufer, x2 = siğim, x3 = kutiş (ms). Natija: öqilgan (0 = yopildi).
pub const SYS_TCP_RECV: u64 = 41;
/// TCP yopiş. x0 = ulaniş.
pub const SYS_TCP_CLOSE: u64 = 42;

// --- Signallar ---
/// Signal yuboradi. x0 = PID, x1 = signal (9/15 — töxtatiş). Natija: 0 yoki xato.
pub const SYS_SIGNAL: u64 = 43;
/// Kelgan signallar bitmaskasini qaytaradi (va tozalaydi).
pub const SYS_SIGPENDING: u64 = 44;

/// Signallar
pub const SIG_TERM: u64 = 15;
pub const SIG_KILL: u64 = 9;
pub const SIG_USER: u64 = 10;
/// Hozirgi vaqt: Unix soniyalar (UTC).
pub const SYS_TIME: u64 = 34;
/// Vaqt mintaqasi: UTC dan farq (daqiqalarda).
pub const SYS_TZ: u64 = 35;

/// Standart dastaklar
pub const STDIN: u64 = 0;
pub const STDOUT: u64 = 1;
pub const STDERR: u64 = 2;
/// SYS_SPAWN: x4 = stdin | stdout << 16 | stderr << 32; INHERIT — otanikini işlatiş
pub const INHERIT: u64 = 0xffff;
/// SYS_SPAWN: x5 = qöşimça dastak (bolada 3-raqam; INHERIT — yöq) | SPAWN_SANDBOX
pub const SPAWN_SANDBOX: u64 = 1 << 16;

/// OPEN bayroqlari
pub const OPEN_CREATE: u64 = 1;
pub const OPEN_TRUNCATE: u64 = 2;
pub const OPEN_APPEND: u64 = 4;
/// Yoziş uçun oçiş (yaratmasdan va tozalamasdan)
pub const OPEN_WRITE: u64 = 8;

/// Bitta xabarning eng katta hajmi (bayt).
pub const MAX_MESSAGE: usize = 4096;

// --- Xato kodlari ---
/// Notöğri manzil (masalan, yadro xotirasiga işora).
pub const ERR_BAD_ADDRESS: i64 = -1;
/// Nomaʼlum tizim çaqiruvi.
pub const ERR_UNKNOWN_SYSCALL: i64 = -2;
/// Topilmadi.
pub const ERR_NOT_FOUND: i64 = -3;
/// Allaqaçon mavjud.
pub const ERR_EXISTS: i64 = -4;
/// Xotira yetmadi.
pub const ERR_NO_MEMORY: i64 = -5;
/// Notöğri dastak.
pub const ERR_BAD_HANDLE: i64 = -6;
/// Kanal navbati töla.
pub const ERR_FULL: i64 = -7;
/// Juda katta (xabar buferga siğmaydi va h.k.).
pub const ERR_TOO_BIG: i64 = -8;
/// Papka emas.
pub const ERR_NOT_DIR: i64 = -9;
/// Bu papka.
pub const ERR_IS_DIR: i64 = -10;
/// Papka böş emas.
pub const ERR_NOT_EMPTY: i64 = -11;
/// Notöğri argument.
pub const ERR_INVALID: i64 = -12;
/// Bu jarayon sizning bolangiz emas.
pub const ERR_NOT_CHILD: i64 = -13;
/// Ruxsat yöq.
pub const ERR_DENIED: i64 = -14;
/// Disk xatosi.
pub const ERR_IO: i64 = -15;
/// Diskdagi maʼlumot buzilgan (şifr tekşiruvidan ötmadi).
pub const ERR_CORRUPT: i64 = -16;
/// Quvurning öquvçisi qolmagan.
pub const ERR_BROKEN_PIPE: i64 = -17;
/// Jarayon hali işlayapti (SYS_WAIT kutmaslik rejimida).
pub const ERR_RUNNING: i64 = -18;
/// Kutiş töxtatildi (jarayonga töxtatiş sörovi keldi).
pub const ERR_INTERRUPTED: i64 = -19;
