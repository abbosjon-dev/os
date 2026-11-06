# Zamin OS — agent xotirasi (doimiy kontekst)

Sen Zamin OS ustida avtonom ishlayotgan Claude Code agentisan. Har sikl yangi
kontekstda boshlanadi — kerakli hamma narsa shu faylda va repoda.

## Loyiha
- Rust, AArch64, noldan (faqat `core`/`alloc`), QEMU `virt`. Muallif: Abbosjon Dev (zamin.app).
- Build: `cargo build` (target `aarch64-unknown-none-softfloat`). Ogohlantirishlar bo'lmasligi shart.
- Tekshiruv: `scripts/agent/verify.sh full` (build + QEMU yuklanish, self-testlar, panic yo'q).
- Skrinshot: `scripts/agent/shot.py` (QMP; `CLICKS`/`OUT` muhit o'zgaruvchilari).
- Asosiy fayllar: `src/gui.rs` (GUI + brauzer), `src/html.rs`, `src/css.rs`, `src/js.rs`,
  `src/net/*` (TCP/TLS/HTTP), `src/gfx_font.rs` (686 belgi).
- UI uch tilli; o'zbekcha YANGI 2026 lotin alifbosida: Sh→Ş, Ch→Ç, Oʻ→Ö, Gʻ→Ğ, tutuq ʼ (U+02BC).

## Qoidalar
- Mavjud ishlaydigan kodni buzma; kichik, tekshiriladigan qadamlar.
- Tashqi uchinchi tomon kodini yuklab olma/kompilyatsiya qilma.
- Commit/push QILMA — supervisor verify.sh o'tgandan keyin o'zi qiladi.
- README.md dagi tegishli bo'limni yangila (o'zbekcha). Versiyani faqat bosqich to'liq tugaganda oshir.
- Billing/auth sozlamalariga TEGMA (`~/.claude*`, `.claude/`, muhit o'zgaruvchilari).
- Oxirgi qator: `TASK_DONE` yoki `TASK_BLOCKED: <sabab>`.
