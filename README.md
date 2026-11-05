# Zamin OS

**Dasturçi:** Abbosjon Dev · **Veb-sayt:** [zamin.app](https://zamin.app) · **Email:** [info@zamin.app](mailto:info@zamin.app)

Rust tilida **noldan** yozilayotgan, **ARM64 (AArch64)** protsessorlari uçun operatsion tizim.
Öz ğoyalarimiz asosida: toza dizayn, mikroyadroga yaqin arxitektura, silliq interfeys. Boşqa tizimlardan kod, nom yoki dizayn köçirilmaydi.

**Asosiy qoida:** heç qanday taşqi crate yoki boşqa OS kodi işlatilmaydi — `Cargo.toml` dagi
`[dependencies]` böş. Faqat Rust kompilyatori va `core` kutubxonasi.

## Ekran koʻrinişi

![Zamin OS ish stoli](docs/desktop.png)

To'liq grafik ish stoli (virtio-gpu): yuqori menyu paneli, rangli dok, va oʻng
tomonda vidjetlar — ob-havo, kalendar, musiqa pleeri va tizim halqalari
(CPU/RAM/Disk). Fon rasmi OS ning **öz JPEG dekoderi** bilan chiziladi.

![Bir neçta oyna](docs/showcase.png)

### Menyular va interaktiv vidjetlar

![Menyular va vidjetlar](docs/menus.png)

- **Ilovalar / Joylar / Tizim** — oçiladigan menyular (siçqon ostidagi band ajratiladi,
  Esc yopadi). *Joylar* uy papkalarini Fayllar oynasida oçadi; *Tizim* — haqida,
  sozlamalar, **qayta yuklaş** va **öçiriş** (avval diskka saqlanadi).
- **Kalendar** — ‹ › bilan oylar almaşadi (sarlavhani bosiş — bugungi oyga qaytiş).
- **Pleer** — haqiqiy kuyni (MP3, öz dekoderimiz) chaladi: ijro/pauza, boşiga, progress.
- **Tizim halqalari haqiqiy:** CPU — rejalaştiruvçi har uzilişda band/böş vazifani
  sanaydi; RAM — fizik sahifalar; Disk — ZaminFS band bloklari.

### Oyna boshqaruvi

![Yashiriş, ölçam va qaytariş](docs/window-controls.png)

- 🔴 **yopiş**, 🟡 **dokka yashiriş**, 🟢 **butun ekranga kattalashtiriş** (yana bosilsa —
  oldingi ölçam). Siçqon tugmalar ustida bölganda macOS kabi `× − +` belgilari chiqadi.
- Sarlavhadan sudrab **siljitiş**, öng-pastki burçakdan sudrab **ölçamni özgartiriş**
  (mazmun — Fayllar grid, terminal satrlari, rasm — yangi ölçamga moslaşadi).
- Dokdagi ikona: ilova oçiq bölsa (yashirilgan ham) — oynani old planga qaytaradi,
  bölmasa — yangi oçadi. Ostidagi nuqta oçiq ilovani bildiradi.

### Ish stoli ikonalari va animatsiyalar

<img src="docs/desktop-icons.png" width="160" align="left" style="margin-right:12px">

Ish stolining çap tomonida uy papkalari (Hujjatlar, Yuklamalar, Musiqa, Rasmlar,
Videolar) ikonalari: **bir marta bosiş — tanlaş**, **ikki marta — Fayllarda oçiş**.

Oynalar **animatsiya** bilan: yangi oyna dok ikonasidan kattalaşib çiqadi, yashirilganda
dokka "uçib" kiradi, kattalaştiriş/qaytariş silliq oʻtadi (easeOutCubic, 230 ms).
Quyidagi kadrlar animatsiya sekinlaştirilgan holda olingan:

<br clear="left">

![Animatsiya kadrlari](docs/animation.jpg)

### Sozlamalar: fon rasmi va asosiy rang

![Aurora fon + binafşa rang: ish stoli va qulf ekrani](docs/themes.jpg)

Sozlamalar → **Ekran**: 4 ta fon (Zamin manzarasi — foto; **Tun**, **Aurora**, **Şafaq** —
OS içida protsedural chiziladi, istalgan ekran ölçamida) va 6 ta **asosiy rang**
(menyu, qidiruv, yon panellar, tanlovlar uçun). Tanlov darhol qöllanadi, **diskka
saqlanadi** (`fon=`, `rang=`) va qayta yuklaşdan keyin ham — qulf ekranida ham — qoladi.

Boşqa bölimlar haqiqiy tizim maʼlumotini körsatadi: **Tarmoq** (virtio-net, DHCP, IP,
şlyuz, DNS, MAC), **Övoz** (qurilma va dekoderlar), **Foydalanuvçilar**, **Xavfsizlik**
(W^X, PAN, KASLR, disk şifrlaş…), **Umumiy** (versiya, xotira, ish vaqti).

<img src="docs/settings-network.png" width="480">

### Tezkor qidiruv (Alt+Space)

<img src="docs/search.png" width="420" align="right">

`Alt+Space` yoki menyu panelidagi lupa — bitta maydondan:
- **ilovalar** (nomi böyiça),
- **papka va fayllar** (uy papkasi içida, yöli bilan) — Enter mos ilovada oçadi,
- **kalkulyator**: `12*(3+4)/2` → `= 42` (+ − × ÷, qavslar, kasrlar),
- **tizim amallari**: qayta yuklaş, öçiriş.

↑/↓ — tanlaş, Enter — oçiş, Esc — yopiş, siçqon bilan ham bosiladi.

<br clear="right">

### Klaviatura yorliqlari va bildirişnomalar

| Ilova almaştirgiç (Alt+Tab) | Bildirişnomalar markazi |
|---|---|
| ![Alt+Tab](docs/switcher.png) | ![Bildirişnomalar](docs/notifications.png) |

| Yorliq | Amal |
|---|---|
| `Alt+Space` | Tezkor qidiruv (ilova, fayl, hisob) |
| `Alt+Tab` / `Alt+Shift+Tab` | Ilova almaştirgiç — Alt qoʻyib yuborilganda tanlangan oyna faollaşadi (yashirilgani ham) |
| `Alt+W` | Faol oynani yopiş |
| `Alt+M` | Faol oynani dokka yashiriş |
| `Alt+F` | Kattalaştiriş / oldingi ölçam |
| `Esc` | Menyu, bildirişnomalar paneli va almaştirgiçni yopiş |

**Bildirişnomalar** öng yuqoridan sirpanib çiqadi (4.5 s, bosilsa yopiladi), menyu
panelidagi qoʻngʻiroq — oxirgi xabarlar tarixi ("Tozalaş" bilan). Yadroning istalgan
joyidan `gui::notify(sarlavha, matn, rang)` bilan yuboriladi. Hozir: kiriş, fayl
saqlaş, övoz qurilmasi yöqligi haqida.

### Qulf ekrani

![Qulf ekrani](docs/lockscreen.png)

Fon rasmi (xiralashtirilgan), katta soat va toʻliq sana, shishasimon kiriş kartasi.

### Ilovalar

| Fayllar | Terminal |
|---|---|
| ![Fayllar](docs/apps/files.png) | ![Terminal](docs/apps/terminal.png) |
| Yon panel (Joylar), orqaga tugmasi va yöl, katta ikonali grid. Rasmlar uçun **haqiqiy eskizlar** (PNG/GIF/SVG — öz dekoderlarimiz bilan). | Haqiqiy EL0 jarayoni bilan quvur orqali bogʻlangan. Rangli prompt (`user@zamin` — yashil, yöl — koʻk), blok kursor. |

| Sozlamalar | Brauzer |
|---|---|
| ![Sozlamalar](docs/apps/settings.png) | ![Brauzer](docs/apps/browser.png) |
| Bölimlar ro'yxati va tizim maʼlumoti (versiya, foydalanuvçi, xotira, ekran). | Manzil qatori va boşlang'iç sahifa (tarmoq qatlami tayyorlanmoqda). |

| Musiqa pleeri |
|---|
| ![Pleer](docs/apps/music.png) |
| Muqova, nom, format maʼlumoti, normallaşgan toʻlqin (ijro qilingan qismi yashil), ijro tugmasi. MP3/OGG/FLAC/WAV/AIFF. |

Barça skrinshotlar QEMU da (virtio-gpu) QMP `screendump` orqali olingan — haqiqiy render.
Dizayn namunasi: [`docs/design-mockup.png`](docs/design-mockup.png).

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

## Tarmoq

To'liq TCP/IP stegi — noldan yozilgan (QEMU SLIRP user-net bilan sinaldi):

- **virtio-net** drayveri (eski v1 va modern v2); qabul uzilişlar orqali, jönatiş sinxron;
- **Ethernet → ARP → IPv4**; **ICMP** (ping), **UDP**, **TCP** (mijoz: ulaniş, jönatiş, oliş, yopiş);
- **DHCP** mijozi — yuklanişda avtomatik IP, gateway va DNS oladi;
- **DNS** mijozi — domen nomidan IP;
- dasturlar: `tarmoq` (holat), `ping`, `yukla` (HTTP sahifa).

```
abbosjon@zamin:~> tarmoq
  IP: 10.0.2.15   Gateway: 10.0.2.2   DNS: 10.0.2.3
abbosjon@zamin:~> ping 10.0.2.2
  10.0.2.2 seq=0 vaqt=9 ms ... 4/4 javob
abbosjon@zamin:~> yukla example.com /          # DNS → TCP → HTTP
```

**TLS 1.3 (HTTPS)** — noldan: X25519 kalit almaşinuvi, ChaCha20-Poly1305, HKDF-SHA256.
`yukla -s host /` şifrlangan HTTPS söröv yuboradi (Cloudflare TLS 1.3 serverida sinaldi).
OGOHLANTIRIŞ: sertifikat hali tekşirilmaydi (MITM dan himoya yöq) va faqat bitta şifr
to'plami (TLS_CHACHA20_POLY1305_SHA256) qöllanadi.

Çeklovlar: tashqi manzilga ICMP ni QEMU SLIRP ötkazmaydi (TCP/UDP ötadi); TCP oddiy
(bitta oyna, qayta jönatişli).

## Boşqa fayl tizimlari (FAT)

Ikkinçi disk FAT16/FAT32 formatida bölsa, u yuklanişda `/fat` papkasiga (faqat öqiş) ulanadi:

```
abbosjon@zamin:~> fayllar /fat
  -r--r--r-- tizim   45 B  readme.txt   ...
abbosjon@zamin:~> öqi /fat/readme.txt
```

Qisqa (8.3) nomlar; yoziş ham qöllanadi (yaratiş/qayta yoziş).

## Fayllar va rasmlar

Grafik **Fayllar menejeri** papkalar içida yuradi va fayllarni turiga qarab
tanib, mos ilovada oçadi:

- fayl turi kengaytmadan aniqlanadi (papka, rasm, matn, dastur, audio, video,
  arxiv) va tur ikonasi bilan körsatiladi;
- rasm faylini bossangiz — **rasm köruvçi** uni dekodlab körsatadi;
- matn faylini bossangiz — muharrirda oçiladi (tahrirlab saqlash mumkin);
- dastur (ELF) bossangiz — maʼlumot körsatiladi.

**Rasm dekoderlari — hammasi noldan yozilgan** (ochiq kutubxonasiz):

- Raster: PNG/APNG (zlib/inflate + 5 filtr), JPEG (baseline — Huffman, IDCT,
  YCbCr), GIF (LZW), BMP/DIB, TGA, PCX, ICO/CUR, QOI, PNM (P1-P6).
- Vektor/hujjat: **SVG** (XML + path tili + Bezier/yoy), **PDF** (tuzilma +
  mazmun oqimi + FlateDecode), **EPS/AI/PS** (kiçik PostScript interpretatori),
  **DXF** (CAD), **WMF/EMF** (Windows metafayl), **CDR** (joylangan önköriniş).
  Umumiy vektor poydevori: tez skanlin rasterizator (AA toʻldiriş + chiziq).

`image::decode_file(nom, baytlar)` formatni aniqlaydi. Juda murakkab formatlar
— HEIC/AVIF, JXL, RAW (CR2/NEF…), PSD, JPEG2000 — tur böyiça tanilади (ikona),
ammo hali oçilmaydi. Vektor/hujjat formatlarida matn/font va gradient
soddalaştirilgan.

## Audio va video

**Audio dekoderlar — hammasi noldan yozilgan:**

- **WAV** (RIFF): PCM 8/16/24/32-bit, float, hamda o'yin/media kodeklari —
  G.711 **μ-law**/**A-law**, **IMA/DVI ADPCM**, **Microsoft ADPCM**.
- **AIFF/AIFC** (big-endian, 80-bit kengaytirilgan chastota).
- **FLAC** — yoʻqotishsiz (Rice, fixed/LPC, kanal dekorrelyatsiyasi).
- **MP3** (MPEG-1 Audio Layer III) — to'liq zanjir: sarlavha, yon maʼlumot,
  bit rezervuar, Huffman (ISO jadvallari), qayta kvantlaş, qayta tartiblaş,
  stereo (MS/intensity), antialias, IMDCT (uzun/qisqa bloklar), polifaz sintez
  filtri. Taniş barcha versiyalar uçun (MPEG-1/2/2.5, Layer I/II/III, VBR, ID3).
  ffmpeg bilan tekşirilgan (korrelyatsiya 1.0000).
- **OGG/Vorbis** — to'liq dekod (noldan): OGG konteyner, Vorbis kodkitoblari
  (Huffman + VQ qidiruv), floor1 egri chizigʼi, residue (0/1/2 turlari), kanal
  birikuvi (coupling), teskari MDCT va oyna/overlap-add. Metama'lumot va teglar
  (vendor, TITLE/ARTIST…) ham. ffmpeg bilan tekşirilgan (korrelyatsiya 1.0000).

Grafik **Övoz köruvçi** toʻlqin shaklini chizadi va virtio-sound orqali chaladi.

**Video:**

- **MJPEG/AVI** — to'liq dekod qilinadi (kadrlar JPEG dekoderi orqali), GUI da
  ketma-ket oʻynatiladi. ffmpeg bilan kadr-ba-kadr tekşirilgan.
- **MP4/MOV** (ISO BMFF), **MKV/WebM** (EBML) — konteyner, kodek, oʻlçam va
  davomiylik taniladi (H.264/HEVC/VP9/AV1 kodeklari hozircha dekod qilinmaydi).

`audio::decode(nom, baytlar)` va `video::decode_frames(baytlar)` dekodlaydi.
Namuna fayllar: `Musiqa/zamin.wav`, `Musiqa/zamin.mp3`, `Videolar/zamin.avi`.

## Foydalanuvçilar va ruxsatlar

- `hisoblar` — röyxat; administrator `hisoblar qöş dilnoza` / `hisoblar öçir dilnoza`.
  Har bir foydalanuvçi öz paroli bilan kiradi (disk kaliti har biriga alohida öralgan).
- Fayl ruxsatlari (`rwx`, egasi/boşqalar): uy papkalari maxfiy (700), `/vaqtinçalik` — umumiy
  (har kim faqat öz faylini öçiradi). `ruxsat 600 fayl` — özgartiriş; `fayllar` ruxsat, egasi va
  vaqtni körsatadi. Dasturni işga tuşiriş uçun "x" ruxsati kerak.
- Har bir jarayonning egasi bor; faqat öz jarayonlaringizni töxtata olasiz (administrator — hammasini).
- `çeklangan dastur` — qafas: dastur faqat joriy papka içida işlaydi, boşqa dastur işga tuşira
  olmaydi, nomli kanallarni oça olmaydi (`çeklangan qafas` — tekşiruv).
- Kanallar: boşqa foydalanuvçining kanalini nom orqali oçib bölmaydi (`ommaviy/...` bundan mustasno);
  nomsiz kanallar faqat dastak orqali beriladi.
- `mintaqa +5` — vaqt mintaqasi (administrator); `vaqt` — sana va vaqt.

## Işonçlilik

- **Yadro jurnali** — yadro çiqargan barça xabarlar xotiradagi halqa buferda saqlanadi;
  `jurnal [qatorlar]` ularni körsatadi.
- **Panic diskka yoziladi** — yadro xatosi (panic) yuz bersa, xabar (qaysi yadro, vaqt, sabab)
  qulfsiz va uzilişsiz diskka yoziladi; keyingi yuklanişda `[xavf] oldingi işga tuşirişda...`
  körinadi. Şunda sabab elektr öçsa ham yöqolmaydi.
- **Fuzz sinovi** — `fuzz` dasturi tizim çaqiruvlariga minglab tasodifiy argumentlar yuboradi
  (yadro manzillari, xaritalanmagan manzillar, ulkan sonlar...). Yadro heç qaçon qulamasligi va
  har bir foydalanuvçi manzilini tekşirişi kerak. Yuklanişda 15000 ta çaqiruv bilan avtomatik
  sinaladi; `fuzz [son]` bilan qölda ham işga tuşiriş mumkin.

Fuzz sinovi tufayli uçta haqiqiy xato topildi va tuzatildi: `SYS_SLEEP` va `SYS_MAP` da
son töliş (overflow), hamda kanallar dastak yopilganda böşatilmay, jadvalni töldirişi.

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
user/src/bin/        Dasturlar: terminal, tarmoq, ping, yukla, oqimlar, qafas, fuzz, ekran, izla, sana, tartib, salom, hisobla, soat, buzuq, sinov, fayl, kalkulyator, mijoz, katta
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
src/time.rs          Sana/vaqt, vaqt mintaqasi
src/klog.rs          Yadro jurnali (halqa bufer)
src/drivers/virtio_net.rs  virtio-net drayveri
src/net/              Tarmoq stegi: Ethernet, ARP, IPv4, ICMP, UDP, TCP, DHCP, DNS
src/fs/fat.rs         FAT16/FAT32 öquvçi (faqat öqiş)
src/net/tls.rs        TLS 1.3 mijozi (X25519, ChaCha20-Poly1305, HKDF)
src/crypto/x25519.rs  X25519 kalit almaşinuvi
src/crypto/hkdf.rs    HKDF-SHA256 va TLS 1.3 kalit jadvali
src/drivers/virtio_gpu.rs  virtio-gpu 2D drayveri
src/gfx/mod.rs        Çiziş: piksel, töğriburçak, matn, gradient
src/gfx_font.rs       8×16 bitmap şrift
src/drivers/virtio_input.rs  Klaviatura/planşet drayveri
src/gui.rs            Oynalar menejeri (oynalar, terminal, vazifalar paneli)
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
- [x] **5. Foydalanuvçilar va ruxsatlar** — bir neçta hisob, fayl ruxsatlari, jarayon egasi, qafas, kanal huquqlari
- [x] **6. Vaqt** — RTC, vaqt mintaqasi (standart Toşkent, UTC+5), fayl vaqtlari
- [x] **7. Drayverlar** — virtio-blk disk, virtio-net tarmoq, virtio-rng; GPU/klaviatura — grafika bilan
- [x] **7½. Birinçi sozlaş** — til, hisob (login + parol), tilga mos tizim papkalari, kiriş ekrani
- [x] **8. Fayl tizimi** — VFS + ZaminFS 2 (bloklar, copy-on-write, şifrlangan daraxt)
- [x] **9. Işonçlilik** — yadro jurnali, panic diskka yoziladi, fuzz sinovi
- [x] **8½. Yadro himoyasi** — qörğonli yadro steklari, PAN, WXN, böşatilgan xotirani nollaş, ASLR
- [x] **9. Grafika** — virtio-gpu framebuffer, şrift, oynalar menejeri, grafik terminal, sichqoncha/klaviatura
- [x] **9½. Media** — rasm (16+ format), audio (WAV/AIFF/FLAC/MP3/OGG-Vorbis + ADPCM/μ-law/A-law), video (MJPEG/AVI dekod; MP4/MOV/MKV/WebM taniş) — hammasi noldan, ffmpeg bilan tekşirilgan
- [ ] **10. Foydalanuvçi muhiti** — Zamin terminali, Zamin paneli, ilovalar (davom etmoqda)
- [ ] **11. Haqiqiy apparat** — Raspberry Pi 4/5 porti

## Grafika

Zamin OS'da to'liq grafik muhit bor (virtio-gpu, 32-bit rang) — tiniq
(anti-aliased) va har qanday ekran ölçamiga moslaşadi:

- özimiz yozgan virtio-gpu 2D drayveri (resurs, scanout, transfer, flush);
  ekranning afzal körilgan ölçamini soraydi (GET_DISPLAY_INFO) va shunga moslaşadi;
- silliq (anti-aliased) masshtablanadigan şrift: har belgi 0..255 qamrov
  xaritasi + metrikalar, chizishda 2x2 supersampling bilan istalgan ölçamga
  moslaşadi — DejaVu Sans (UI) va Sans Mono (terminal), özbek/rus/belgilar;
- gfx: alfa aralaştiriş, gradient, yumaloq burçakli shakllar, doira, soya;
- UI mac'dan ilhomlanib (ammo ko'çirmay) — yumaloq oynalar, svetofor nuqtalari,
  yuqori menyu paneli, markazlashgan dok va yumaloq ilova ikonalari;
- barça ölçamlar ekrandan kelib çiqadi: 800×600, 1024×768, 1440×900 da tiniq.

```sh
# Oynada (grafik muhit kerak):
cargo build && sh scripts/qemu-gfx.sh target/aarch64-unknown-none-softfloat/debug/kernel
# Serversiz skrinshot (PNG):
python3 scripts/screenshot.py target/aarch64-unknown-none-softfloat/debug/kernel ekran.png
```

GPU va kiriş (virtio-input) bölsa, to'liq grafik muhit işga tuşadi:

- **Grafik kiriş (login):** yuklanişda chiroyli parol ekrani — foydalanuvçi
  parolni kiritadi, toʻgʻri bölsa şifrlangan disk ulanadi va ish stoliga ötiladi.
  Bir neçta hisob bölsa — sichqoncha bilan tanlaş. (Grafika bölganda serial faqat
  maʼmur zaxira konsuli bölib qoladi.)
- **Oynalar menejeri:** sichqoncha kursori, siljitiladigan oynalar (sarlavha,
  yopiş tugmasi), old plan/fokus, pastda vazifalar paneli.
- **Grafik terminal:** oyna ichida haqiqiy EL0 foydalanuvçi terminali
  (`/dasturlar/terminal`) quvurlar orqali işlaydi — oddiy terminal emulyatori
  (bola stdin/stdout quvurlarga ulanadi, GUI I/O ni haydaydi, ANSI tozalaşni tuşunadi).
- **Ilovalar:** Matn muharriri (`~/qaydlar.txt` ni saqlaydi), Soat (jonli vaqt),
  Rasm köruvçi (namunaviy "Zamin" manzarasi), Fayllar, Haqida.

Klaviatura va sichqoncha — özimiz yozgan virtio-keyboard / virtio-tablet drayveri.
GPU bölmasa, tizim matn rejimida (UART terminali) işlayveradi.

## Backend mustahkamlaş (xavfsizlik va işonçlilik)

Grafikadan oldin backend 11 ta yönalişda mustahkamlandi:

1. TCP ulanişlari jarayonga bogʻlandi (boşqa jarayon öqiy olmaydi), jarayon tugaganda yopiladi
2. Qafas (sandbox) endi tarmoqni ham tösadi
3. ARP keşi eskiradi (60 s)
4. TCP qayta jönatiş (RTO, yöqolgan segmentlar tiklanadi)
5. ZaminFS saqlaş xatosida bloklar qaytariladi (oqmaydi)
6. Talab böyiça sahifalash (demand paging): xotira faqat tegilganda ajratiladi
7. Signallar (SYS_SIGNAL, SIG_TERM/KILL/USER), kutişlar signal bilan uziladi
8. Rejalaştiruvçi: yadro mosligi (affinity) + ish öğirlaş
9. Köp foydalanuvçi, fayl ruxsatlari, jarayon egasi (avval qöşilgan)
10. FAT16/FAT32 öqiş (/fat)
11. TLS 1.3 (HTTPS) — şifrlangan kanal

Keyin yana uchta yönaliş qoşildi:

12. **KASLR:** yadro steklari joylaşuvi har yuklanişda tasodifiy (1 GiB sohada
    16320 xil joy, ~14 bit entropiya). Qaytiş manzillari saqlanadigan eng muhim
    yadro sohasi hujumçi uchun oldindan bilib bölmaydigan boʻladi.
13. **FAT yoziş:** ikkinchi diskka fayl yaratiş/qayta yoziş (FAT16/FAT32) —
    boʻsh klasterlarni topadi, zanjirni bogʻlaydi, FAT nusxalarini va 8.3 papka
    yozuvini yangilaydi. Natija standart FAT drayverlari öqiy oladigan haqiqiy FAT.
14. **X.509 sertifikat tahlili:** TLS qöl alişuvida server sertifikati tekşiriladi —
    DER/ASN.1 tahlil, amal muddati (notBefore/notAfter) va host nomi (CN + SAN,
    `*.` joker). Mos kelmasa ulaniş rad etiladi. *Eslatma:* imzo zanjiri (CA ildiz
    işonçi) hali tekşirilmaydi — bu hujjatlaştirilgan keyingi qadam. Tahlilçi
    haqiqiy sertifikatlar (ISRG Root X1 — RSA, USERTrust ECC — ECDSA) bilan sinaldi.

Yadro öz-özini tekşirişi (yuklanişda `[test]`): heap, MMU, W^X, PAN, kripto,
X.509, rasm (15 format), audio (WAV/FLAC/MP3/OGG), video (MJPEG/AVI), taymer, KASLR,
rejalaştiruvçi, SMP, mutex, disk, fuzz, stek qörğoni, foydalanuvçi rejimi, va
(ikkinchi disk bölsa) FAT yoziş.
Tizim çaqiruvlari fuzz-sinovi (15000 tasodifiy çaqiruv) har yuklanişda ötadi.

## Versiyalaş

Joriy versiya: **0.0.18**. Versiya har bir özgarişda emas, faqat işlaydigan
bosqiç (etap) yakunlanganda kötariladi.
- 0.0.2 — toʻliq grafik muhit (kiriş, oynada haqiqiy EL0 terminal, ilovalar)
  va backend mustahkamlaş (KASLR, FAT yoziş, X.509 tahlili).
- 0.0.3 — tiniq (anti-aliased) masshtablanadigan grafika, mac uslubidagi UI
  (yumaloq oynalar, dok, ikonalar), har qanday ekran ölçamiga moslaşuv.
- 0.0.4 — fayllar bilan işlaş: Fayllar menejeri (tur tanib oçadi) va rasm
  köruvçi; PNG/JPEG/GIF/BMP/QOI/PNM dekoderlari (noldan).
- 0.0.5 — vektor va hujjat formatlari (noldan): SVG, PDF, EPS/AI, DXF, WMF/EMF,
  CDR; umumiy vektor rasterizator. Qoşimça raster: TGA, PCX, ICO/CUR, APNG.
- 0.0.6 — vektor hujjatlarda matn (SVG/PDF/EPS), harakatli rasm (GIF animatsiya
  oʻynatiş), audio (WAV/AIFF dekoder + toʻlqin köruvchi va ijro boshi).
- 0.0.7 — haqiqiy övoz çiqariş: virtio-sound drayveri (PCM, koʻp navbat).
- 0.0.8 — FLAC (yöqotişsiz audio) va APNG (harakatli PNG) dekoderlari.
- 0.0.9 — **MP3** (MPEG-1 Layer III) dekoderi va kengaytirilgan WAV kodeklari
  (μ-law/A-law/ADPCM); **video**: MJPEG/AVI dekod, MP4/MOV/MKV/WebM taniş.
  Barça media dekoderlari ffmpeg bilan tekşirilgan.
- 0.0.10 — **OGG/Vorbis** to'liq dekoderi (noldan): kodkitoblar, floor1,
  residue, kanal birikuvi, teskari MDCT. ffmpeg bilan aniq mos (korr. 1.0000).
- 0.0.11 — **Ish stoli qayta dizayni**: rasm fon (ichki JPEG dekoder bilan),
  yuqori menyu (menyular + holat belgilari + sana), oʻng vidjetlar paneli
  (ob-havo, kalendar, pleer, CPU/RAM/Disk halqalari), 10 ta rangli dok ikonasi.
- 0.0.12 — **Ilovalar qayta chizildi**: Fayllar (yon panel + grid + rasm eskizlari),
  Terminal (rangli prompt, blok kursor), Matn muharriri (qator raqamlari), Soat
  (analog siferblat), Musiqa pleeri (muqova + toʻlqin), Sozlamalar, Brauzer, Savat,
  Haqida (Zamin logosi). UI matnlari yangi alifboda.
- 0.0.13 — **Menyular va interaktiv vidjetlar**: Ilovalar/Joylar/Tizim oçiladigan
  menyulari (qayta yuklaş/öçiriş bilan), kalendarda oylar almaşadi, pleer haqiqiy
  kuyni chaladi ("Zamin kuyi", Karplus-Strong sintezi), CPU/Disk halqalari haqiqiy
  oʻlçovlardan; yangi qulf ekrani (fon rasmi + soat).
- 0.0.14 — **Oyna boshqaruvi**: sariq — dokka yashiriş, yashil — kattalashtiriş/qaytariş,
  hover belgilari (× − +), burçakdan ölçamni özgartiriş, dok ikonasi oçiq oynani qaytaradi.
- 0.0.15 — **Ish stoli ikonalari** (uy papkalari; bir marta — tanlaş, ikki marta — oçiş)
  va **oyna animatsiyalari** (oçiliş, yashiriş, qaytariş, kattalaştiriş).
- 0.0.16 — **Klaviatura yorliqlari** (Alt+Tab almaştirgiç, Alt+W/M/F) va
  **bildirişnomalar** (toastlar + tarix paneli, `gui::notify` API).
- 0.0.17 — **Tezkor qidiruv** (Alt+Space): ilovalar, fayllar, kalkulyator, tizim amallari;
  klaviatura toʻliq US belgilari bilan (Shift+raqam, qavslar va h.k.).
- 0.0.18 — **Jonli Sozlamalar**: fon rasmi (foto + 3 protsedural) va asosiy rang tanlaş,
  diskka saqlaş; Tarmoq/Övoz/Foydalanuvçilar/Xavfsizlik bölimlari haqiqiy maʼlumot bilan.

Versiya `Cargo.toml` da saqlanadi va yadro işga tuşganda ekranga çiqadi.
