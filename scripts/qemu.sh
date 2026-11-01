#!/bin/sh
# Zamin OS ni QEMU da işga tuşiradi (`cargo run` şu skriptni çaqiradi).
# Disk obrazi (sozlamalar uçun) birinçi marta avtomatik yaratiladi.
#
#   cargo run                        — odatiy işga tuşiriş
#   cargo run -- -append til=en      — yuklaniş xabarlari tili (tizim hali sozlanmagan bölsa)
#   ZAMIN_DISK=boşqa.img cargo run   — boşqa disk obrazi
set -e
KERNEL="$1"
shift
DISK="${ZAMIN_DISK:-target/zamin-disk.img}"
if [ ! -f "$DISK" ]; then
    mkdir -p "$(dirname "$DISK")"
    truncate -s 32M "$DISK"
fi
exec qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 512M -smp 4 -nographic \
    -drive "file=$DISK,if=none,format=raw,id=zdisk" -device virtio-blk-device,drive=zdisk \
    -kernel "$KERNEL" "$@"
