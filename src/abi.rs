// Zamin tizim çaqiruvlari (ABI). Bu fayl yadro va foydalanuvçi kutubxonasi
// tomonidan birga işlatiladi (`include!`), şuning uçun raqamlar hamişa mos.
//
// Çaqiriş: `svc #0`, x8 = çaqiruv raqami, x0..x3 = argumentlar,
// natija x0 da. Manfiy natija — xato (quyidagi ERR_* kodlari).
// Matnlar (yöl, nom) hamişa "manzil + uzunlik" juftligi sifatida beriladi.

// --- Jarayon ---
/// Jarayonni tugatadi. x0 = çiqiş kodi.
pub const SYS_EXIT: u64 = 0;
/// Konsolga yozadi. x0 = manzil, x1 = uzunlik. Natija: yozilgan baytlar.
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
/// Dasturni işga tuşiradi. x0,x1 = fayl yöli; x2,x3 = argumentlar. Natija: PID.
pub const SYS_SPAWN: u64 = 8;
/// Bola jarayon tugaşini kutadi. x0 = PID. Natija: uning çiqiş kodi.
pub const SYS_WAIT: u64 = 9;

// --- Fayllar ---
/// Faylni oçadi. x0,x1 = yöl; x2 = bayroqlar (OPEN_*). Natija: dastak (handle).
pub const SYS_OPEN: u64 = 10;
/// Dastakdan öqiydi. x0 = dastak, x1 = bufer, x2 = siğim. Natija: öqilgan baytlar (0 = oxiri).
pub const SYS_READ: u64 = 11;
/// Faylga yozadi. x0 = dastak, x1 = manzil, x2 = uzunlik. Natija: yozilgan baytlar.
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

/// OPEN bayroqlari
pub const OPEN_CREATE: u64 = 1;
pub const OPEN_TRUNCATE: u64 = 2;
pub const OPEN_APPEND: u64 = 4;

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
