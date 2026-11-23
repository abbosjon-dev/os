#!/usr/bin/env bash
# Zamin OS — VPS disk obrazi (GPT, raw): ESP (FAT32, EFI/BOOT/BOOTAA64.EFI) + Zamin maʼlumot bölimi.
#
#   scripts/vps/obraz.sh [--hajm 2G] [--release] [--yadro yadro.elf] [--xz] [çiqiş.img]
#
#   --hajm N     obraz hajmi: 512M, 2G, ... (standart 2G; MiB/GiB). Fayl siyrak — diskda kam joy oladi
#   --release    yadroni release rejimida yiğiş (standart: debug — verify.sh sinovidan ötgan yiğiş)
#   --yadro F    tayyor yadro ELF faylini olish (yiğmasdan)
#   --xz         qöşimça çiqiş.img.xz (yuklaş uçun siqilgan nusxa) yasaş
# Natija: target/zamin-vps.img (standart) va uning SHA-256 summasi (.sha256).
# Faqat Python standart kutubxonasi kerak (mtools, mkfs, sgdisk shart emas).
# Obraz QEMU da: scripts/vps/README.md ("Mahalliy sinov") ga qarang.
set -euo pipefail
cd "$(dirname "$0")/../.."
SIZE=2G; PROFILE=debug; KERNEL=""; XZ=0; OUT=target/zamin-vps.img
while [ $# -gt 0 ]; do
    case "$1" in
        --hajm) SIZE="$2"; shift ;;
        --release) PROFILE=release ;;
        --yadro) KERNEL="$2"; shift ;;
        --xz) XZ=1 ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
        -*) echo "Nomaʼlum bayroq: $1" >&2; exit 1 ;;
        *) OUT="$1" ;;
    esac
    shift
done
case "$SIZE" in
    *G|*g) MIB=$(( ${SIZE%[Gg]} * 1024 )) ;;
    *M|*m) MIB=${SIZE%[Mm]} ;;
    *) MIB=$SIZE ;;
esac
[ "$MIB" -ge 256 ] 2>/dev/null || { echo "XATO: hajm kamida 256M bölişi kerak ($SIZE)" >&2; exit 1; }

if [ -z "$KERNEL" ]; then
    if [ "$PROFILE" = release ]; then cargo build --release -q; else cargo build -q; fi
    KERNEL=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/$PROFILE/kernel
fi
[ -f "$KERNEL" ] || { echo "XATO: yadro topilmadi: $KERNEL" >&2; exit 1; }

mkdir -p "$(dirname "$OUT")"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
python3 scripts/efi/mkefi.py "$KERNEL" "$TMP/BOOTAA64.EFI"
python3 scripts/efi/mkfat.py "$TMP/esp.img" 64 "EFI/BOOT/BOOTAA64.EFI=$TMP/BOOTAA64.EFI"
python3 scripts/vps/gpt.py "$OUT" "$MIB" "$TMP/esp.img"
( cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")" >"$(basename "$OUT").sha256" )
if [ "$XZ" = 1 ]; then
    xz -T0 -k -f -6 "$OUT"
    ( cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT").xz" >"$(basename "$OUT").xz.sha256" )
    echo "siqilgan: $OUT.xz ($(du -h "$OUT.xz" | cut -f1))"
fi
echo "tayyor: $OUT ($MIB MiB, diskda $(du -h "$OUT" | cut -f1)); SHA-256: $(cut -d' ' -f1 "$OUT.sha256")"
