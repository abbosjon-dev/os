#!/bin/sh
# Zamin OS — UEFI yuklaş diski: yadro (ELF) → BOOTAA64.EFI → FAT32 obraz (EFI/BOOT/BOOTAA64.EFI).
#   scripts/efi/mkesp.sh [yadro.elf] [çiqiş.img]
# Faqat Python standart kutubxonasi (mtools/mkfs kerak emas).
set -e
cd "$(dirname "$0")/../.."
KERNEL="${1:-${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel}"
OUT="${2:-target/zamin-esp.img}"
EFI="$(dirname "$OUT")/BOOTAA64.EFI"
python3 scripts/efi/mkefi.py "$KERNEL" "$EFI"
python3 scripts/efi/mkfat.py "$OUT" 64 "EFI/BOOT/BOOTAA64.EFI=$EFI"
