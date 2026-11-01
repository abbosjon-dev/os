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

`cargo run` disk obrazini (`target/zamin-disk.img`, 32 MiB) birinçi marta özi yaratadi —
unda tizim sozlamalari saqlanadi.

Işga tuşgaç **Zamin terminali** oçiladi (`zamin>`). `yordam` buyruqlar röyxatini
körsatadi, `öçir` (yoki `ochir`) tizimni öçiradi. Majburan çiqiş: `Ctrl+A`, keyin `X`.

## Birinçi işga tuşiriş

Tizim birinçi marta yoqilganda **birinçi sozlaş** boşlanadi:

1. **Til** — özbekça, русский yoki English. Tanlangan til butun tizimga qöllanadi:
   xabarlar, buyruqlar yordami va **tizim papkalari nomlari** ham.
2. **Töliq ism** — masalan, "Abbosjon Dev".
3. **Foydalanuvçi nomi (login)** — ismdan avtomatik taklif qilinadi ("Аббосжон" → `abbosjon`).
4. **Parol** — ikki marta, ekranda yaşirin (`*`).

Sozlamalar diskka yoziladi. Keyingi yuklanişlarda faqat parol söraladi.

## Xavfsizlik va şifrlaş

- **ZaminFS 2** — diskdagi bloklarga asoslangan fayl tizimi, töliq şifrlangan:
  - 4 KiB bloklar, har biri alohida ChaCha20-Poly1305 bilan (har yozuvda yangi nonce);
  - autentifikatsiyalangan daraxt: superblok → metadata → maʼlumot bloklari (nonce va blok raqami
    yuqori darajada saqlanadi) — blokni özgartiriş, köçiriş yoki eski nusxasini qaytariş aniqlanadi;
  - copy-on-write: bloklar joyida qayta yozilmaydi, superblok A/B navbatma-navbat almaştiriladi —
    tok qaysi payt öçmasin, oxirgi butun holat saqlanadi; yangisi buzilsa — oldingisi oçiladi;
  - faqat özgargan bloklar yoziladi; katta fayllar (256 MiB gaça) xotiraga töliq yuklanmaydi,
    kerakli bloklar diskdan öqiladi, keş töla bölsa çiqariladi;
  - metadata: egasi, ruxsatlar, yaratilgan/özgartirilgan vaqt (RTC dan);
  - fayllar har 2 soniyada, `saqla` bilan yoki öçirişdan oldin saqlanadi; diskni oçib bölmasa,
    ustidan heç narsa yozilmaydi.
- **Kalitlar** — har bir hisobga tasodifiy 256-bitli asosiy kalit. U paroldan PBKDF2-HMAC-SHA256
  (300 000 takror) bilan olingan kalit orqali "öralib" saqlanadi. Parol heç qayerda saqlanmaydi;
  parol özgarganda maʼlumotlar qayta şifrlanmaydi; `tiklaş` kalitni öçiradi — maʼlumotlar abadiy oçilmaydi.
- **Kriptografiya noldan** — SHA-256, HMAC, PBKDF2, ChaCha20, Poly1305; har yuklanişda standart
  test vektorlari bilan tekşiriladi.
- **Tasodifiy sonlar** — ChaCha20 asosidagi CSPRNG; manbalar: virtio-rng, RNDR (bölsa), taymer tebranişi.
- **Kiriş** — notöğri paroldan keyin kutiş ikki baravar östiriladi (eng köpi 30 s).
- **Yadro xotirasi himoyasi**:
  - **W^X + WXN** — yadro kodi yozilmaydi, yoziladigan xotira (apparat darajasida) bajarilmaydi;
  - **PAN** — yadro foydalanuvçi xotirasiga faqat `uaccess` nusxalaş funksiyalari orqali tegadi;
    har bir manzil sahifa jadvali böyiça tekşiriladi va yadro buferiga nusxalanadi (TOCTOU yöq);
  - **Qörğonli yadro steklari** — har bir stek 64 KiB oynaning yuqori yarmida, pastki yarmi
    xaritalanmagan. Töliş istisno vektorida aniqlanadi (registrlarni buzmasdan), favqulodda stekka
    ötiladi va faqat aybdor oqim tugatiladi;
  - **Böşatilgan xotira nollanadi** — kalitlar va maʼlumotlar boşqa jarayonga ötmaydi;
  - **ASLR** — har bir dasturning steki va heapʼi tasodifiy manzilda.
- Har yuklanişda bu himoyalar ham tekşiriladi (`[test]` qatorlari).
- `xavfsizlik` buyruği himoya holatini körsatadi. PAN va RNDR uçun `-cpu max` kerak
  (`cargo run` şuni işlatadi); eski protsessorlarda bular avtomatik öçiriladi.

| Rol | özbekça | русский | English |
|---|---|---|---|
| Dasturlar | `/dasturlar` | `/программы` | `/programs` |
| Tizim | `/tizim` | `/система` | `/system` |
| Uy | `/uy/<login>` | `/дом/<login>` | `/home/<login>` |
| Hujjatlar | `Hujjatlar` | `Документы` | `Documents` |
| Yuklamalar | `Yuklamalar` | `Загрузки` | `Downloads` |
| Rasmlar | `Rasmlar` | `Изображения` | `Pictures` |
| Musiqa | `Musiqa` | `Музыка` | `Music` |

Tizim papkalari joriy tilda körsatiladi, lekin uç tildagi istalgan nomi bilan topiladi —
şuning uçun dasturlardagi yöllar (`/dasturlar/salom`) har qanday tilda işlaydi.
`~` — joriy foydalanuvçining uy papkasi (`~/Hujjatlar`).

Hisob buyruqlari: `kimman` (kimligingiz), `parol` (parolni özgartiriş),
`tiklaş` (sozlamalarni öçirib, birinçi sozlaşga qaytiş).

## Köp yadrolilik (SMP)

Zamin OS protsessorning barça yadrolarini işlatadi (`cargo run` — 4 ta yadro):

- birinçi yadro tizimni tayyorlab, qolganlarini PSCI `CPU_ON` bilan uyğotadi; har bir yadro öz
  MMU, PAN, istisnolar vektori, GIC interfeysi va taymerini sozlaydi;
- rejalaştiruvçi umumiy: istalgan tayyor vazifa istalgan böş yadroda işlaydi. Vazifa konteksti
  töliq saqlanmaguncha (`on_cpu`) uni boşqa yadro ola olmaydi; tozalaş ham şundan keyin;
- uxlaş/uyğotiş şarti rejalaştiruvçi qulfi ostida tekşiriladi — yöqolgan uyğotiş bölmaydi;
- yangi vazifa tayyor bölganda uxlab yotgan yadrolar SGI bilan darhol uyğotiladi;
- har bir yadroning öz yuklaniş va favqulodda steklari (qörğon qoidasi bilan);
- konsol qulfi — bir neçta yadro yozsa ham qatorlar aralaşmaydi.

`jarayonlar` buyruği har bir işlayotgan vazifa qaysi yadroda ekanini (CPU) körsatadi.

## Tillar

Asosiy til — **özbekça**, 2026-yilgi yangi lotin alifbosida (Ş, Ç, Ö, Ğ va tutuq belgisi ʼ).
Qöşimça tillar: **русский** va **English**.

- Terminalda tilni almaştiriş: `til ru`, `til en`, `til uz` (yoki `язык ru`, `lang en`)
- Yuklaşda tanlaş: `cargo run -- -append til=ru`
- Buyruqlarni istalgan tilda yoziş mumkin: `yordam`, `помощь`, `help`

Kod içidagi barça izohlar va hujjatlar ham yangi alifboda yoziladi.

## Zamin terminali

Terminal — oddiy foydalanuvçi dasturi (`/dasturlar/terminal`). Yadro uni kirişdan keyin
"işonçli" jarayon sifatida işga tuşiradi: faqat u tizim buyruqlarini (parol, tiklaş, öçir...)
yadroga uzata oladi; u işga tuşirgan dasturlarga bu huquq ötmaydi.

```
abbosjon@zamin:~> ekran mevalar.txt | izla an | sana     # quvurlar
abbosjon@zamin:~> tartib -t mevalar.txt > tartib.txt     # çiqişni faylga
abbosjon@zamin:~> aks yana bir qator >> mevalar.txt       # faylga qöşiş
abbosjon@zamin:~> kir Hujjatlar                           # joriy papka
abbosjon@zamin:~/Hujjatlar> hisobla 50000000              # Ctrl+C — töxtatiş
abbosjon@zamin:~> soat &                                   # fonda
abbosjon@zamin:~> muhit SALOM=dunyo                        # muhit özgaruvçisi
abbosjon@zamin:~> örnat /dasturlar/salom meningsalom       # ~/dasturlar ga örnatiş
abbosjon@zamin:~> çiqiş                                    # çiqiş (qayta kiriş)
```

- har bir jarayonda standart kiriş/çiqiş/xato (0, 1, 2): konsol, quvur yoki fayl;
- quvurlar: öquvçi maʼlumot kelgunça uxlaydi, yozuvçilar yopilsa — "fayl oxiri";
- joriy papka va muhit özgaruvçilari (`PATH`, `UY`, `FOYDALANUVCHI`) bolaga meros bölib ötadi;
- Ctrl+C — oldingi plandagi dasturni töxtatadi, Ctrl+D — kiriş oxiri;
- fondagi dastur klaviaturani "öğirlamaydi" (uning kirişi böş);
- töxtatiş xavfsiz: jarayon yadro içida (masalan, qulf uşlab turganda) uzilmaydi, foydalanuvçi
  rejimiga qaytayotganda özi tugaydi.

## Oqimlar

Bitta jarayon içida bir neçta oqim parallel (turli yadrolarda) işlaydi. Ular jarayonning
manzil maydoni, dastaklari, muhiti va joriy papkasini böliştadi.

```rust
let h = zamin::thread::spawn(|| hisobla(0, 1_000_000));
let natija = h.join();

static HISOB: zamin::sync::Mutex<u64> = zamin::sync::Mutex::new(0);
*HISOB.lock() += 1; // band bölsa — oqim uxlaydi (futex)
```

- `SYS_THREAD_CREATE / EXIT / JOIN`, `SYS_FUTEX_WAIT / WAKE`;
- istalgan oqim `exit` qilsa — butun jarayon tugaydi (qolgan oqimlar xavfsiz nuqtada töxtaydi);
- jarayon resurslari faqat barça oqimlari tugagaç böşatiladi;
- `oqimlar 4` — 4 oqimda tub sonlarni sanaydi va tezlaşişni körsatadi (~3.6x).

Yordamçi dasturlar: `ekran` (fayl/kirişni çiqariş), `izla` (qidiriş), `sana` (sanaş), `tartib` (tartiblaş).

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
src/task/syscall.rs  Tizim çaqiruvlari
src/task/uaccess.rs  Foydalanuvçi xotirasi: tekşiriş + nusxalaş (PAN bilan)
src/fs/mod.rs        VFS + ramfs: /dasturlar, /tizim, /uy
src/ipc.rs           Zamin kanallari — jarayonlararo xabar almaşiş
src/abi.rs           Tizim çaqiruvlari raqamlari (yadro va dasturlar uçun umumiy)
build.rs             Foydalanuvçi dasturlarini yiğib, yadroga joylaydi
user/src/lib.rs      `zamin` kutubxonasi: tizim çaqiruvlari, println!, tr(), Channel, spawn/wait
user/src/heap.rs     Foydalanuvçi xotira ajratuvçisi (Vec, String, format! işlaydi; oqimlar uçun xavfsiz)
user/src/thread.rs   Oqimlar: spawn, join
user/src/sync.rs     Futex asosidagi Mutex
user/src/fs.rs       Fayllar: File, read, write, list, mkdir, remove
user/src/bin/        Dasturlar: terminal, oqimlar, ekran, izla, sana, tartib, salom, hisobla, soat, buzuq, sinov, fayl, kalkulyator, mijoz, katta
src/setup.rs         Birinçi sozlaş (til, ism, login, parol) va kiriş
src/accounts.rs      Foydalanuvçi hisoblari, parol xeşi
src/config.rs        Diskdagi sozlamalar
src/crypto/          SHA-256, HMAC, PBKDF2, ChaCha20, Poly1305, AEAD (noldan)
src/random.rs        Kriptografik tasodifiy sonlar generatori
src/fs/zaminfs.rs    ZaminFS 2 — bloklar, copy-on-write, şifrlangan daraxt
src/fs/file.rs       Fayl mazmuni: bloklar, keş, diskdan öqiş
src/security.rs      `xavfsizlik` buyruği
src/drivers/virtio.rs      virtio-mmio transporti
src/drivers/virtio_rng.rs  virtio-rng drayveri
src/drivers/virtio_blk.rs  virtio-blk disk drayveri (v1 va v2)
src/line.rs          Qator tahrirlovçisi (UTF-8, yaşirin parol kiritiş)
scripts/qemu.sh      `cargo run` uçun: disk obrazini yaratadi va QEMU ni işga tuşiradi
src/i18n.rs          Köp tillilik: özbekça (asosiy), русский, English
src/shell.rs         Kiriş, terminalni işga tuşiriş, tizim buyruqlari (SYS_ADMIN), zaxira terminal
src/pipe.rs          Quvurlar
user/src/bin/terminal.rs  Zamin terminali (foydalanuvçi rejimida)
```

## Yöl xaritasi

- [x] **1. Boot** — AArch64 entry, EL2→EL1, UART konsol, istisnolar vektori, PSCI
- [x] **2. Xotira** — DTB, fizik sahifa allokatori, MMU + sahifa jadvallari, kernel heap (`alloc`)
- [x] **3. Uzilişlar** — GICv2 drayveri, ARM taymeri, klaviatura uzilişi, Zamin terminali
- [x] **4. Jarayonlar** — kontekst almaştiriş, rejalaştiruvçi, EL0 foydalanuvçi rejimi, har jarayonga öz manzil maydoni, ELF yuklovçi
- [x] **5. Syscallʼlar va IPC** — 20 ta tizim çaqiruvi; "Zamin kanallari" — nomli xabar kanallari
- [x] **5½. Yadro markazi** — W^X himoya, foydalanuvçi heapʼi, dastur işga tuşiriş/kutiş, argumentlar, VFS + ramfs
- [x] **6. SMP** — barça yadrolar (PSCI CPU_ON), köp yadroli rejalaştiruvçi, yadrolararo uzilişlar (SGI)
- [ ] **7. Drayverlar** — virtio-blk disk tayyor; tarmoq, GPU, klaviatura/siçqonça — navbatda
- [x] **7½. Birinçi sozlaş** — til, hisob (login + parol), tilga mos tizim papkalari, kiriş ekrani
- [x] **8. Fayl tizimi** — VFS + ZaminFS 2 (bloklar, copy-on-write, şifrlangan daraxt)
- [x] **8½. Yadro himoyasi** — qörğonli yadro steklari, PAN, WXN, böşatilgan xotirani nollaş, ASLR
- [ ] **9. Grafika** — framebuffer, kompozitor, oyna menejeri, şriftlar
- [ ] **10. Foydalanuvçi muhiti** — Zamin terminali, Zamin paneli, ilovalar
- [ ] **11. Haqiqiy apparat** — Raspberry Pi 4/5 porti

## Versiyalaş

Joriy versiya: **0.0.1**. Versiya har bir özgarişda emas, faqat işlaydigan
bosqiç (etap) yakunlanganda kötariladi. Versiya `Cargo.toml` da saqlanadi
va yadro işga tuşganda ekranga çiqadi.
