#!/usr/bin/env bash
# Zamin OS — mustaqil tekşiruv (supervisor "DONE" ga ishonmaydi, şuni işlatadi).
#   verify.sh quick — yiğiş: xato va ogohlantirish yöq
#   verify.sh full  — + QEMU da (grafikasiz) yuklaniş: barça self-test [ok], panic yöq;
#                     + brauzer sahifalari surati: yumaloq burçaklarda töq piksel yöq
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
[[ "$RESULT" == OK* ]] || exit 1

# --- Yumaloq burçaklar regressiyasi: brauzer sahifalari suratga olinadi va
#     check_corners.py dan ötkaziladi ("qora tişçalar" qaytmasligi uçun) ---
CDIR="$LOG_DIR/corners"; mkdir -p "$CDIR" /tmp/claude-0; rm -f "$CDIR"/*.png
# brauzer (dock) → kattalaştiriş → zamin://matn (boşi, yarim aylantirilgan) → bosh → zamin://bezak → bosh → zamin://overflow
M="m,1275,795;w,1"
CLICKS="354,756;w,2;221,85;w,1;300,585;w,2;$M;snap,matn;m,640,400;wd,8;$M;snap,matn2;93,120;w,1;300,611;w,2;$M;snap,bezak;93,120;w,1;300,637;w,2;$M" \
    timeout 240 python3 scripts/agent/shot.py "$KERNEL" "$CDIR/overflow.png" fresh >"$CDIR/shot.log" 2>&1
SHOTS=("$CDIR/overflow_matn.png" "$CDIR/overflow_matn2.png" "$CDIR/overflow_bezak.png" "$CDIR/overflow.png")
for f in "${SHOTS[@]}"; do
    [ -s "$f" ] || { echo "FAIL burçaklar: surat olinmadi ($f, qarang $CDIR/shot.log)"; exit 1; }
done
NBAD=0
for f in "${SHOTS[@]}"; do
    if ! python3 scripts/agent/check_corners.py "$f" >"$f.txt" 2>&1; then
        NBAD=$((NBAD + 1)); echo "  $(basename "$f"): $(grep -m2 -E 'NUQSON|burchaklar' "$f.txt" | tr '\n' ' ')"
    fi
done
[ "$NBAD" -eq 0 ] || { echo "FAIL burçaklar: $NBAD surat nuqsonli (qarang $CDIR)"; exit 1; }
# o'z-o'zini sinaş: burçak yaqiniga bitta qora piksel — tekşiruvçi 1 qaytarişi şart
python3 - "$CDIR/overflow_bezak.png" "$CDIR/selftest.png" <<'PY' || { echo "FAIL burçaklar: o'z-o'zini sinaş surati yasalmadi"; exit 1; }
import subprocess, sys
from PIL import Image
out = subprocess.run([sys.executable, "scripts/agent/check_corners.py", sys.argv[1], "-v"], capture_output=True, text=True).stdout
ln = next(l for l in out.splitlines() if l.startswith("BURCHAK"))
cx, cy, sx, sy = map(int, ln.split()[1:5])
im = Image.open(sys.argv[1]).convert("RGB")
im.putpixel((cx + 2 * sx, cy + 2 * sy), (0, 0, 0))
im.save(sys.argv[2])
PY
if python3 scripts/agent/check_corners.py "$CDIR/selftest.png" >"$CDIR/selftest.txt" 2>&1; then
    echo "FAIL burçaklar: qora pikselli surat tekşiruvdan ötib ketdi (check_corners.py buzilgan)"; exit 1
fi
echo "[ok] burchaklar: ${#SHOTS[@]} surat toza"
