# Zamin OS

**Dasturchi:** Abbosjon Dev · **Veb-sayt:** [zamin.app](https://zamin.app) · **Email:** [info@zamin.app](mailto:info@zamin.app)

Rust tilida **noldan** yozilayotgan, **ARM64 (AArch64)** protsessorlari uchun operatsion tizim.
O'z g'oyalarimiz asosida: toza dizayn, mikroyadroga yaqin arxitektura, silliq interfeys. Boshqa tizimlardan kod, nom yoki dizayn ko'chirilmaydi.

**Asosiy qoida:** hech qanday tashqi crate yoki boshqa OS kodi ishlatilmaydi — `Cargo.toml` dagi
`[dependencies]` bo'sh. Faqat Rust kompilyatori va `core` kutubxonasi.

## Ishga tushirish

```bash
# Talablar: rustup, qemu-system-aarch64
rustup target add aarch64-unknown-none-softfloat   # rust-toolchain.toml buni avtomatik qiladi
cargo run                                # QEMU virt mashinasida ishga tushadi
```

Ishga tushgach **Zamin terminali** ochiladi (`zamin>`). `yordam` buyruqlar ro'yxatini
ko'rsatadi, `ochir` tizimni o'chiradi. Majburan chiqish: `Ctrl+A`, keyin `X`.

## Tuzilma

```
linker.ld            Xotira xaritasi (yadro 0x4020_0000 da, oldidagi 2 MiB — DTB uchun)
src/arch/boot.s      Boot: core'larni ajratish, EL2->EL1, stek, .bss
src/arch/vectors.s   Istisnolar vektor jadvali (16 ta kirish)
src/main.rs          kernel_main, panic handler
src/uart.rs          PL011 UART drayveri, print!/println!
src/exceptions.rs    Istisnolarni qayta ishlash (ESR/ELR/FAR)
src/power.rs         PSCI: o'chirish / qayta yuklash
src/sync.rs          SpinLock
src/dtb.rs           Device Tree o'quvchisi (RAM hajmi, model)
src/memory/frame.rs  Fizik sahifa allokatori (bitmap)
src/memory/mmu.rs    MMU, sahifa jadvallari, map/unmap/translate
src/memory/heap.rs   Kernel heap (free list) — Box/Vec/String ishlaydi
src/gic.rs           GICv2 uzilishlar boshqaruvchisi
src/timer.rs         ARM taymeri (100 Hz)
src/irq.rs           Uzilishlarni drayverlarga tarqatish
src/shell.rs         Zamin terminali (yordam, haqida, vaqt, xotira, ochir...)
```

## Yo'l xaritasi

- [x] **1. Boot** — AArch64 entry, EL2→EL1, UART konsol, istisnolar vektori, PSCI
- [x] **2. Xotira** — DTB, fizik sahifa allokatori, MMU + sahifa jadvallari, kernel heap (`alloc`)
- [x] **3. Uzilishlar** — GICv2 drayveri, ARM taymeri, klaviatura uzilishi, Zamin terminali
- [ ] **4. Jarayonlar** — kontekst almashtirish, rejalashtiruvchi (scheduler), EL0 user mode
- [ ] **5. Syscall'lar va IPC** — `svc`, xabar almashish — "Zamin kanallari" (o'z dizaynimiz)
- [ ] **6. SMP** — barcha core'larni ishga tushirish, spinlock'lar
- [ ] **7. Drayverlar** — virtio (blok, tarmoq, GPU, klaviatura/sichqoncha), Device Tree parser
- [ ] **8. Fayl tizimi** — VFS + o'z fayl tizimimiz
- [ ] **9. Grafika** — framebuffer, kompozitor, oyna menejeri, shriftlar
- [ ] **10. Foydalanuvchi muhiti** — Zamin terminali, Zamin paneli, ilovalar
- [ ] **11. Haqiqiy apparat** — Raspberry Pi 4/5 porti

## Versiyalash

Joriy versiya: **0.0.1**. Versiya har bir o'zgarishda emas, faqat ishlaydigan
bosqich (etap) yakunlanganda ko'tariladi. Versiya `Cargo.toml` da saqlanadi
va yadro ishga tushganda ekranga chiqadi.
