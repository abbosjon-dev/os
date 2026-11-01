# Zamin OS

**Dasturchi:** Abbosjon Dev · **Veb-sayt:** [zamin.app](https://zamin.app) · **Email:** [info@zamin.app](mailto:info@zamin.app)

Rust tilida **noldan** yozilayotgan, **ARM64 (AArch64)** protsessorlari uchun operatsion tizim.
macOS'dan ilhomlangan (toza dizayn, mikroyadroga yaqin arxitektura, silliq UI), lekin undan nusxa ko'chirilmaydi.

**Asosiy qoida:** hech qanday tashqi crate yoki boshqa OS kodi ishlatilmaydi — `Cargo.toml` dagi
`[dependencies]` bo'sh. Faqat Rust kompilyatori va `core` kutubxonasi.

## Ishga tushirish

```bash
# Talablar: rustup, qemu-system-aarch64
rustup target add aarch64-unknown-none   # rust-toolchain.toml buni avtomatik qiladi
cargo run                                # QEMU virt mashinasida ishga tushadi
```

QEMU'dan chiqish: `Ctrl+A`, keyin `X`.

## Tuzilma

```
linker.ld            Xotira xaritasi (yadro 0x4008_0000 da)
src/arch/boot.s      Boot: core'larni ajratish, EL2->EL1, stek, .bss
src/arch/vectors.s   Istisnolar vektor jadvali (16 ta kirish)
src/main.rs          kernel_main, panic handler
src/uart.rs          PL011 UART drayveri, print!/println!
src/exceptions.rs    Istisnolarni qayta ishlash (ESR/ELR/FAR)
src/power.rs         PSCI: o'chirish / qayta yuklash
```

## Yo'l xaritasi

- [x] **1. Boot** — AArch64 entry, EL2→EL1, UART konsol, istisnolar vektori, PSCI
- [ ] **2. Xotira** — fizik sahifa allokatori, MMU + sahifa jadvallari, kernel heap (`alloc`)
- [ ] **3. Uzilishlar** — GICv2/v3 drayveri, ARM generic timer
- [ ] **4. Jarayonlar** — kontekst almashtirish, rejalashtiruvchi (scheduler), EL0 user mode
- [ ] **5. Syscall'lar va IPC** — `svc`, xabar almashish (mach-port'larga o'xshash g'oya, o'z dizaynimiz)
- [ ] **6. SMP** — barcha core'larni ishga tushirish, spinlock'lar
- [ ] **7. Drayverlar** — virtio (blok, tarmoq, GPU, klaviatura/sichqoncha), Device Tree parser
- [ ] **8. Fayl tizimi** — VFS + o'z fayl tizimimiz
- [ ] **9. Grafika** — framebuffer, kompozitor, oyna menejeri, shriftlar
- [ ] **10. Foydalanuvchi muhiti** — shell, dock, ilovalar
- [ ] **11. Haqiqiy apparat** — Raspberry Pi 4/5 porti
