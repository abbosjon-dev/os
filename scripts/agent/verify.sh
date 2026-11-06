#!/usr/bin/env bash
# Zamin OS — mustaqil tekşiruv (supervisor "DONE" ga ishonmaydi, şuni işlatadi).
#   verify.sh quick — yiğiş: xato va ogohlantirish yöq
#   verify.sh full  — + QEMU da (grafikasiz) yuklaniş: barça self-test [ok], panic yöq
# Çiqiş kodi 0 — ötdi. Natija qisqa matn sifatida stdout ga.
set -uo pipefail
cd "$(dirname "$0")/../.."
MODE="${1:-quick}"
LOG_DIR=".agent/logs"; mkdir -p "$LOG_DIR"
BUILD_LOG="$LOG_DIR/verify-build.log"

if ! cargo build >"$BUILD_LOG" 2>&1; then
    echo "FAIL build: $(grep -m3 -E '^error' "$BUILD_LOG" | tr '\n' ' ')"; exit 1
fi
WARN=$(grep -cE '^warning' "$BUILD_LOG" || true)
if [ "$WARN" -gt 0 ]; then
    echo "FAIL warnings=$WARN: $(grep -m3 -A2 -E '^warning' "$BUILD_LOG" | tr '\n' ' ' | cut -c1-400)"; exit 1
fi
[ "$MODE" = "quick" ] && { echo "OK build (0 warnings)"; exit 0; }

# --- Yuklaniş sinovi: toza disk, faqat seriya port ---
KERNEL=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel
DISK=$(mktemp /tmp/zamin-verify-XXXX.img); truncate -s 128M "$DISK"
BOOT_LOG="$LOG_DIR/verify-boot.log"; : >"$BOOT_LOG"
timeout 150 qemu-system-aarch64 -machine virt -cpu max -m 512M -smp 4 -nographic \
    -drive "file=$DISK,if=none,format=raw,id=d" -device virtio-blk-device,drive=d \
    -device virtio-rng-device -kernel "$KERNEL" </dev/null >"$BOOT_LOG" 2>&1 &
QPID=$!
RESULT="FAIL boot: timeout (setup prompt never appeared)"
for _ in $(seq 1 140); do
    sleep 1
    if grep -qaiE 'panic|yiqildi' "$BOOT_LOG"; then
        RESULT="FAIL boot: $(grep -aiE -m2 'panic|yiqildi' "$BOOT_LOG" | tr -d '\r' | tr '\n' ' ' | cut -c1-300)"; break
    fi
    if grep -qaE 'Choose your language|Tilni tanlang|Выберите язык' "$BOOT_LOG"; then
        OKS=$(grep -acE '\[ok\]|\] .* OK$|— OK' "$BOOT_LOG")
        RESULT="OK build+boot (0 warnings, self-test lines: $OKS)"; break
    fi
    kill -0 $QPID 2>/dev/null || { RESULT="FAIL boot: qemu exited early"; break; }
done
kill $QPID 2>/dev/null; wait $QPID 2>/dev/null; rm -f "$DISK"
echo "$RESULT"
[[ "$RESULT" == OK* ]]
