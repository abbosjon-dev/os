#!/bin/sh
# Zamin OS ni grafik rejimda işga tuşiradi (virtio-gpu bilan).
# Oynani köriş uçun: `-display gtk` yoki `-display sdl` (grafik muhit kerak).
# Serversiz (headless) sinov uçun: `-display none` + QMP `screendump`.
K="$1"; shift
DISK="${ZAMIN_DISK:-target/zamin-disk.img}"
[ -f "$DISK" ] || truncate -s 64M "$DISK"
exec qemu-system-aarch64 -machine virt -cpu max -smp 4 -m 512M \
  -device virtio-gpu-device -device virtio-keyboard-device -device virtio-tablet-device \
  -drive "file=$DISK,if=none,format=raw,id=s" -device virtio-blk-device,drive=s \
  -device virtio-rng-device \
  -netdev user,id=n -device virtio-net-device,netdev=n \
  -serial mon:stdio -display "${ZAMIN_DISPLAY:-gtk}" \
  -kernel "$K" "$@"
