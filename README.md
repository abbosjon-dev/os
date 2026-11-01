# Zamin OS

**Dasturçi:** Abbosjon Dev · **Veb-sayt:** [zamin.app](https://zamin.app) · **Email:** [info@zamin.app](mailto:info@zamin.app)

Rust tilida **noldan** yozilayotgan, **ARM64 (AArch64)** protsessorlari uçun operatsion tizim.
Öz ğoyalarimiz asosida: toza dizayn, mikroyadroga yaqin arxitektura, silliq interfeys. Boşqa tizimlardan kod, nom yoki dizayn köçirilmaydi.

**Asosiy qoida:** heç qanday taşqi crate yoki boşqa OS kodi işlatilmaydi — `Cargo.toml` dagi
`[dependencies]` böş. Faqat Rust kompilyatori va `core` kutubxonasi.

## Işga tuşiriş

```bash
# Talablar: rustup, qemu-system-aarch64
rustup target add aarch64-unknown-none-softfloat   # rust-toolchain.toml buni avtomatik qiladi
cargo run                                # QEMU virt maşinasida işga tuşadi
```

Işga tuşgaç **Zamin terminali** oçiladi (`zamin>`). `yordam` buyruqlar röyxatini
körsatadi, `öçir` (yoki `ochir`) tizimni öçiradi. Majburan çiqiş: `Ctrl+A`, keyin `X`.

## Tillar

Asosiy til — **özbekça**, 2026-yilgi yangi lotin alifbosida (Ş, Ç, Ö, Ğ va tutuq belgisi ʼ).
Qöşimça tillar: **русский** va **English**.

- Terminalda tilni almaştiriş: `til ru`, `til en`, `til uz` (yoki `язык ru`, `lang en`)
- Yuklaşda tanlaş: `cargo run -- -append til=ru`
- Buyruqlarni istalgan tilda yoziş mumkin: `yordam`, `помощь`, `help`

Kod içidagi barça izohlar va hujjatlar ham yangi alifboda yoziladi.

## Dasturlar va jarayonlar

Terminalda:

```
zamin> dasturlar            # mavjud dasturlar röyxati
zamin> işlat salom          # dasturni işga tuşiriş (tugagunça kutadi)
zamin> işlat soat &         # fonda işga tuşiriş
zamin> jarayonlar           # işlayotgan jarayonlar
zamin> tugat 6              # jarayonni tugatiş
zamin> işlat hisobla 1000000 # argument bilan
zamin> işlat mijoz 7 * 6    # mijoz kalkulyator serverini işga tuşirib, kanal orqali söröv yuboradi
zamin> kanallar             # Zamin kanallari
```

Fayllar (hozirça xotirada saqlanadi):

```
zamin> fayllar uy           # papka tarkibi
zamin> yoz uy/a.txt Salom   # faylga yoziş
zamin> öqi uy/a.txt         # faylni öqiş (yoki: oqi)
zamin> papka uy/rasmlar     # papka yaratiş
zamin> taşla uy/a.txt       # öçiriş (yoki: tashla)
```

`/dasturlar` va `/tizim` — tizim papkalari: ularni özgartirib bölmaydi.

Har bir dastur EL0 (foydalanuvçi) rejimida, öz manzil maydonida işlaydi. U yadro
xotirasiga kira olmaydi — urinsa, faqat şu dastur tugatiladi (`işlat buzuq`).

Yangi dastur yoziş: `user/src/bin/` ga fayl qöşing:

```rust
#![no_std]
#![no_main]
use zamin::{fs, format, println, tr};
zamin::entry!(main);

fn main() -> i64 {
    println!("{}", tr(["Salom!", "Привет!", "Hello!"]));
    let ism = zamin::args();                       // işlat dasturim Abbos
    let _ = fs::write("/uy/ism.txt", format!("{}\n", ism).as_bytes());
    0 // çiqiş kodi
}
```

`cargo run` uni avtomatik yiğadi va yadroga joylaydi.

## Tuzilma

```
linker.ld            Xotira xaritasi (yadro 0x4020_0000 da, oldidagi 2 MiB — DTB uçun)
src/arch/boot.s      Boot: coreʼlarni ajratiş, EL2->EL1, stek, .bss
src/arch/vectors.s   Istisnolar vektor jadvali (16 ta kiriş)
src/arch/switch.s    Kontekst almaştiriş, yadro oqimi boşlaniş nuqtasi
src/main.rs          kernel_main, panic handler
src/uart.rs          PL011 UART drayveri, print!/println!
src/exceptions.rs    Istisnolarni qayta işlaş (ESR/ELR/FAR)
src/power.rs         PSCI: öçiriş / qayta yuklaş
src/sync.rs          SpinLock
src/dtb.rs           Device Tree öquvçisi (RAM hajmi, til, PSCI usuli)
src/memory/frame.rs  Fizik sahifa allokatori (bitmap)
src/memory/mmu.rs    MMU, sahifa jadvallari, map/unmap/translate
src/memory/heap.rs   Kernel heap (free list) — Box/Vec/String işlaydi
src/gic.rs           GICv2 uzilişlar boşqaruvçisi
src/timer.rs         ARM taymeri (100 Hz)
src/irq.rs           Uzilişlarni drayverlarga tarqatiş
src/task/mod.rs      Jarayonlar va rejalaştiruvçi (round-robin, 10 ms)
src/task/elf.rs      ELF64 yuklovçi
src/task/syscall.rs  Tizim çaqiruvlari (har bir foydalanuvçi manzili tekşiriladi)
src/fs/mod.rs        VFS + ramfs: /dasturlar, /tizim, /uy
src/ipc.rs           Zamin kanallari — jarayonlararo xabar almaşiş
src/abi.rs           Tizim çaqiruvlari raqamlari (yadro va dasturlar uçun umumiy)
build.rs             Foydalanuvçi dasturlarini yiğib, yadroga joylaydi
user/src/lib.rs      `zamin` kutubxonasi: tizim çaqiruvlari, println!, tr(), Channel, spawn/wait
user/src/heap.rs     Foydalanuvçi xotira ajratuvçisi (Vec, String, format! işlaydi)
user/src/fs.rs       Fayllar: File, read, write, list, mkdir, remove
user/src/bin/        Dasturlar: salom, hisobla, soat, buzuq, sinov, fayl, kalkulyator, mijoz
src/i18n.rs          Köp tillilik: özbekça (asosiy), русский, English
src/shell.rs         Zamin terminali (yordam, jarayonlar, işlat, tugat, til, öçir...)
```

## Yöl xaritasi

- [x] **1. Boot** — AArch64 entry, EL2→EL1, UART konsol, istisnolar vektori, PSCI
- [x] **2. Xotira** — DTB, fizik sahifa allokatori, MMU + sahifa jadvallari, kernel heap (`alloc`)
- [x] **3. Uzilişlar** — GICv2 drayveri, ARM taymeri, klaviatura uzilişi, Zamin terminali
- [x] **4. Jarayonlar** — kontekst almaştiriş, rejalaştiruvçi, EL0 foydalanuvçi rejimi, har jarayonga öz manzil maydoni, ELF yuklovçi
- [x] **5. Syscallʼlar va IPC** — 20 ta tizim çaqiruvi; "Zamin kanallari" — nomli xabar kanallari
- [x] **5½. Yadro markazi** — W^X himoya, foydalanuvçi heapʼi, dastur işga tuşiriş/kutiş, argumentlar, VFS + ramfs
- [ ] **6. SMP** — barça coreʼlarni işga tuşiriş, spinlockʼlar
- [ ] **7. Drayverlar** — virtio (blok, tarmoq, GPU, klaviatura/siçqonça), Device Tree parser
- [ ] **8. Fayl tizimi** — VFS va xotiradagi fayl tizimi tayyor; diskdagi öz fayl tizimimiz — navbatda
- [ ] **9. Grafika** — framebuffer, kompozitor, oyna menejeri, şriftlar
- [ ] **10. Foydalanuvçi muhiti** — Zamin terminali, Zamin paneli, ilovalar
- [ ] **11. Haqiqiy apparat** — Raspberry Pi 4/5 porti

## Versiyalaş

Joriy versiya: **0.0.1**. Versiya har bir özgarişda emas, faqat işlaydigan
bosqiç (etap) yakunlanganda kötariladi. Versiya `Cargo.toml` da saqlanadi
va yadro işga tuşganda ekranga çiqadi.
