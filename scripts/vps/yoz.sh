#!/usr/bin/env bash
# Zamin OS — VPS obrazini (scripts/vps/obraz.sh) diskka yozish. Netcup Rescue tizimida işlatiladi.
#
#   bash yoz.sh <obraz.img | obraz.img.xz | https://.../obraz.img[.xz]> <disk> [bayroqlar]
#   masalan:  bash yoz.sh zamin-vps.img.xz /dev/vda
#
# DIQQAT: <disk> dagi HAMMA maʼlumot (oldingi tizim, fayllar) öçadi. Skript avval disk haqida
# maʼlumot körsatadi va ikki marta tasdiq söraydi: disk nomini qayta yoziş va "HA".
# Uni faqat siz özingiz, Rescue tizimida (VPS diski ulanmagan holda) işga tuşiring.
#
# Bayroqlar:
#   --kengaytirma   Zamin bölimini diskning oxirigaça kengaytirmaslik (standart: kengaytiriladi,
#                   zaxira GPT disk oxiriga köçiriladi)
#   --sha256 HEX    obraz nazorat summasini tekşiriş (URL bölsa, <URL>.sha256 avtomatik sinaladi)
#   --sinov         (faqat sinov uçun) <disk> örniga oddiy fayl qabul qilinadi, root şart emas
set -euo pipefail

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mXATO:\033[0m %s\n' "$*" >&2; exit 1; }

SRC=""; DEV=""; GROW=1; SUM=""; TEST=0
while [ $# -gt 0 ]; do
    case "$1" in
        --kengaytirma) GROW=0 ;;
        --sha256) SUM="$2"; shift ;;
        --sinov) TEST=1 ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        -*) die "nomaʼlum bayroq: $1" ;;
        *) if [ -z "$SRC" ]; then SRC="$1"; elif [ -z "$DEV" ]; then DEV="$1"; else die "ortiqça argument: $1"; fi ;;
    esac
    shift
done
[ -n "$SRC" ] && [ -n "$DEV" ] || die "işlatiş: bash yoz.sh <obraz> <disk>   (masalan: bash yoz.sh zamin-vps.img.xz /dev/vda)"

if [ "$TEST" = 1 ]; then
    [ -f "$DEV" ] || die "--sinov: $DEV oddiy fayl bölişi kerak"
else
    [ "$(id -u)" = 0 ] || die "root sifatida işga tuşiring"
    [ -b "$DEV" ] || die "$DEV blok qurilma emas (diskni körish: lsblk)"
    case "$DEV" in *[0-9]p[0-9]*|/dev/[sv]d[a-z][0-9]*|/dev/xvd[a-z][0-9]*)
        die "$DEV — bölim; butun diskni körsating (masalan /dev/vda)" ;; esac
    # Disk yoki uning bölimlari ulangan bölsa (masalan, işlab turgan tizim diski) — yozmaymiz
    if lsblk -nro MOUNTPOINT "$DEV" 2>/dev/null | grep -q .; then
        lsblk "$DEV"
        die "$DEV (yoki bölimi) ulangan. Rescue tizimidan işga tuşiring yoki avval umount qiling"
    fi
    if grep -qs "^$DEV" /proc/swaps; then die "$DEV da swap yoqilgan (swapoff)"; fi
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
case "$SRC" in
    http://*|https://*)
        say "Obraz yuklab olinmoqda: $SRC"
        F="$WORK/$(basename "${SRC%%\?*}")"
        curl -fL --retry 3 -o "$F" "$SRC" || die "yuklab bölmadi"
        if [ -z "$SUM" ] && curl -fsL -o "$WORK/sum" "$SRC.sha256" 2>/dev/null; then SUM=$(cut -d' ' -f1 "$WORK/sum"); fi
        SRC="$F" ;;
esac
[ -f "$SRC" ] || die "obraz topilmadi: $SRC"
if [ -n "$SUM" ]; then
    say "Nazorat summasi tekşirilmoqda"
    [ "$(sha256sum "$SRC" | cut -d' ' -f1)" = "$SUM" ] || die "SHA-256 mos kelmadi — obraz buzilgan"
fi

XZ=0
case "$SRC" in *.xz) XZ=1; command -v xz >/dev/null || die "xz topilmadi (apt-get install xz-utils)" ;; esac
if [ "$XZ" = 1 ]; then
    IMG_SIZE=$(xz --robot --list "$SRC" | awk '$1=="totals"{print $5}')
    HEAD=$( { xz -dc "$SRC" 2>/dev/null || true; } | head -c 520 | tail -c 8)
else
    IMG_SIZE=$(stat -c %s "$SRC")
    HEAD=$(head -c 520 "$SRC" | tail -c 8)
fi
[ "$HEAD" = "EFI PART" ] || die "$SRC Zamin OS GPT obrazi emas (sektor 1 da GPT sarlavhasi yöq)"
if [ "$TEST" = 1 ]; then DEV_SIZE=$(stat -c %s "$DEV"); else DEV_SIZE=$(blockdev --getsize64 "$DEV"); fi
[ "$IMG_SIZE" -le "$DEV_SIZE" ] || die "obraz ($((IMG_SIZE >> 20)) MiB) diskdan ($((DEV_SIZE >> 20)) MiB) katta"

cat <<EOI

  Zamin OS diskka yoziladi
  ------------------------
  Obraz:  $SRC ($((IMG_SIZE >> 20)) MiB$( [ "$XZ" = 1 ] && echo ", xz siqilgan"))
  Disk:   $DEV ($((DEV_SIZE >> 30)) GiB)
EOI
[ "$TEST" = 1 ] || lsblk -o NAME,SIZE,TYPE,FSTYPE,LABEL,MODEL "$DEV" | sed 's/^/          /'
cat <<EOI

  !!! $DEV dagi HAMMA maʼlumot QAYTARIB BÖLMAYDIGAN qilib öçiriladi !!!
  (oldingi operatsion tizim, fayllar, bölimlar)

EOI
read -r -p "Tasdiqlaş uçun disk nomini qayta yozing ($DEV): " J1
[ "$J1" = "$DEV" ] || { echo "Mos kelmadi — bekor qilindi."; exit 1; }
read -r -p "Rostdan ham $DEV ga yozamizmi? (HA / yöq): " J2
[ "$J2" = "HA" ] || { echo "Bekor qilindi (davom etiş uçun katta harflar bilan HA yozing)."; exit 1; }

say "Yozilmoqda: $SRC → $DEV"
CONV=fsync; [ "$TEST" = 1 ] && CONV=notrunc,fsync
if [ "$XZ" = 1 ]; then
    xz -dc "$SRC" | dd of="$DEV" bs=4M iflag=fullblock conv=$CONV status=progress
else
    dd if="$SRC" of="$DEV" bs=4M conv=$CONV status=progress
fi
sync

if [ "$GROW" = 1 ] && [ "$DEV_SIZE" -gt "$IMG_SIZE" ]; then
    command -v python3 >/dev/null || { echo "python3 yöq — bölim kengaytirilmadi (Zamin OS $((IMG_SIZE >> 20)) MiB bilan işlaydi)"; GROW=0; }
fi
if [ "$GROW" = 1 ] && [ "$DEV_SIZE" -gt "$IMG_SIZE" ]; then
    say "Zamin bölimi diskning oxirigaça kengaytirilmoqda, zaxira GPT köçirilmoqda"
    python3 - "$DEV" <<'PY'
# GPT: zaxira sarlavha + jadval disk oxiriga, 2-bölim (Zamin, tur 7A616D69-...) oxirgi foydali LBA gaça.
import os, struct, sys, uuid, zlib
S = 512
ZAMIN = uuid.UUID("7A616D69-6E00-4F53-9A4D-494E44534B31").bytes_le
fd = os.open(sys.argv[1], os.O_RDWR)
total = os.lseek(fd, 0, os.SEEK_END) // S
def rd(lba, n=1): os.lseek(fd, lba * S, 0); return os.read(fd, n * S)
def wr(lba, b): os.lseek(fd, lba * S, 0); os.write(fd, b)
h = bytearray(rd(1)); assert h[:8] == b"EFI PART"
old_backup, old_last = struct.unpack_from("<Q", h, 32)[0], struct.unpack_from("<Q", h, 48)[0]
el, n, es = struct.unpack_from("<QII", h, 72)
nsec = (n * es + S - 1) // S
ents = bytearray(rd(el, nsec)[: n * es])
new_last = total - 1 - nsec
grown = False
for i in range(n):
    e = i * es
    if ents[e:e + 16] == ZAMIN and struct.unpack_from("<Q", ents, e + 40)[0] == old_last:
        struct.pack_into("<Q", ents, e + 40, new_last); grown = True
ecrc = zlib.crc32(ents)
def hdr(cur, bak, elba):
    x = bytearray(h[:92]); struct.pack_into("<QQ", x, 24, cur, bak); struct.pack_into("<Q", x, 48, new_last)
    struct.pack_into("<Q", x, 72, elba); struct.pack_into("<I", x, 88, ecrc); struct.pack_into("<I", x, 16, 0)
    struct.pack_into("<I", x, 16, zlib.crc32(x)); return bytes(x).ljust(S, b"\0")
# eski zaxira GPT (endi Zamin bölimi içida) — nollanadi
wr(old_backup - nsec, bytes((nsec + 1) * S))
wr(el, bytes(ents).ljust(nsec * S, b"\0")); wr(1, hdr(1, total - 1, el))
wr(total - 1 - nsec, bytes(ents).ljust(nsec * S, b"\0")); wr(total - 1, hdr(total - 1, 1, total - 1 - nsec))
mbr = bytearray(rd(0)); struct.pack_into("<I", mbr, 458, min(total - 1, 0xFFFFFFFF)); wr(0, bytes(mbr))
os.fsync(fd); os.close(fd)
print(f"   Zamin bölimi: {'kengaytirildi' if grown else 'topilmadi (özgartirilmadi)'}; oxirgi LBA {new_last} ({total * S >> 30} GiB disk)")
PY
    sync
fi

cat <<EOI

  ✅ Tayyor. Zamin OS $DEV ga yozildi.

  Keyingi qadam: Netcup SCP → "Media" → Rescue tizimini öçiring va serverni qayta yoqing
  (yoki: reboot). Ekran — SCP dagi "Screen"/VNC konsoli; birinçi yuklanişda til va hisob
  sozlanadi. Batafsil: scripts/vps/README.md

EOI
