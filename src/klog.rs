//! Yadro jurnali: yadro çiqargan barça xabarlar xotiradagi halqa buferda saqlanadi
//! (`jurnal` buyruği). Bufer töla bölsa — eng eski yozuvlar örniga yoziladi.
//! Konsol qulfi ostida çaqiriladi, şuning uçun alohida qulf kerak emas.

const SIZE: usize = 32 * 1024;

static mut BUF: [u8; SIZE] = [0; SIZE];
static mut POS: usize = 0;
static mut WRAPPED: bool = false;

/// Baytlarni jurnalga qöşadi (konsol qulfi uşlab turilganda).
pub fn append(bytes: &[u8]) {
    unsafe {
        let buf = &mut *(&raw mut BUF);
        for &b in bytes {
            if b == b'\r' {
                continue;
            }
            buf[POS] = b;
            POS += 1;
            if POS == SIZE {
                POS = 0;
                WRAPPED = true;
            }
        }
    }
}

/// Jurnal mazmuni (eng eskisidan yangisigaça).
pub fn snapshot() -> alloc::vec::Vec<u8> {
    crate::uart::with_console(|_| unsafe {
        let buf = &*(&raw const BUF);
        let mut out = alloc::vec::Vec::with_capacity(SIZE);
        if WRAPPED {
            out.extend_from_slice(&buf[POS..]);
        }
        out.extend_from_slice(&buf[..POS]);
        out
    })
}
