#!/usr/bin/env bash
# Zamin OS — mustaqil tekşiruv (supervisor "DONE" ga ishonmaydi, şuni işlatadi).
#   verify.sh quick — yiğiş: xato va ogohlantirish yöq
#   verify.sh full  — + QEMU da (grafikasiz) yuklaniş: barça self-test [ok], panic yöq;
#                     + brauzer sahifalari surati: yumaloq burçaklarda töq piksel yöq
#                     + pci sinovi (DTB + virtio-pci, GICv2/GICv3)
#                     + vps sinovi (UEFI firmware örnatilgan bölsa)
#   verify.sh vps   — yiğiş + UEFI yuklaniş (Netcup ARM VPS ga öxşaş): BOOTAA64.EFI FAT32
#                     diskda, virt,gic-version=3, QEMU_EFI.fd, virtio-*-pci qurilmalar —
#                     til tanlaş ekranigaça, panic yöq, PCI disk uzilişi işlaydi; GICv3 da
#                     SGI (barça yadrolar), seriya klaviatura uzilişi ("3" → English sozlaş),
#                     virtio-gpu/keyboard/tablet-pci, virtio-net/rng-pci;
#                     ACPI zaxira yöli (DTB yöq): RSDP/XSDT, MADT (GIC, CPU lar), GTDT (taymer),
#                     SPCR (UART), MCFG (ECAM) — GICv3/4 CPU va GICv2/2 CPU; + acpi=off (UEFI + DTB)
#   verify.sh pci   — yiğiş + -kernel (DTB ECAM) GICv2 va GICv3 da faqat virtio-*-pci qurilmalar
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

# --- VPS (UEFI) sinovi: ESP disk (EFI/BOOT/BOOTAA64.EFI) + tizim diski, faqat PCI qurilmalar ---
#   vps_test [GIC] [SMP] [MACHINE_QÖŞIMÇA] — standart: 3 4 "" (ACPI rejimi: EDK2 ACPI berganda
#   DTB yaşiriladi → yadro RSDP/XSDT, MADT, GTDT, SPCR, MCFG dan qurilma tavsifini yasaydi).
#   MACHINE_QÖŞIMÇA=",acpi=off" — firmware ACPI bermaydi, DTB beradi (UEFI + DTB yöli).
vps_test() {
    local g=${1:-3} smp=${2:-4} mx=${3:-}
    local tag="GICv$g, $smp CPU${mx:+, ${mx#,}}"
    local kernel=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel
    local fw="" f
    for f in /usr/share/qemu-efi-aarch64/QEMU_EFI.fd /usr/share/AAVMF/AAVMF_CODE.fd; do
        [ -f "$f" ] && { fw=$f; break; }
    done
    [ -n "$fw" ] || { echo "SKIP vps: UEFI firmware yöq (qemu-efi-aarch64)"; return 2; }
    local esp sys log=$LOG_DIR/verify-vps.log
    [ "$g$smp$mx" = 34 ] || log=$LOG_DIR/verify-vps-gic$g-smp$smp${mx//[^a-z]/}.log
    esp=$(mktemp /tmp/zamin-esp-XXXX.img); sys=$(mktemp /tmp/zamin-vps-XXXX.img)
    if ! sh scripts/efi/mkesp.sh "$kernel" "$esp" >"$LOG_DIR/verify-mkesp.log" 2>&1; then
        echo "FAIL vps ($tag): ESP yasalmadi ($(tail -1 "$LOG_DIR/verify-mkesp.log"))"; rm -f "$esp" "$sys"; return 1
    fi
    truncate -s 128M "$sys"; : >"$log"
    # stdin — FIFO: til tanlaş ekranida "3" yuboriladi (PL011 uzilişi GICv3 orqali)
    local fifo; fifo=$(mktemp -u /tmp/zamin-vps-in-XXXX); mkfifo "$fifo"
    sleep 200 >"$fifo" &
    local holder=$!
    timeout 150 qemu-system-aarch64 -machine virt,gic-version=$g$mx -cpu max -m 1G -smp $smp -nographic \
        -bios "$fw" \
        -drive "file=$esp,if=none,format=raw,id=esp" -device virtio-blk-pci,drive=esp,bootindex=0 \
        -drive "file=$sys,if=none,format=raw,id=sys" -device virtio-blk-pci,drive=sys \
        -device virtio-rng-pci -nic user,model=virtio-net-pci \
        -device virtio-gpu-pci -device virtio-keyboard-pci -device virtio-tablet-pci <"$fifo" >"$log" 2>&1 &
    local qpid=$! res="FAIL vps ($tag): timeout (til tanlaş ekrani çiqmadi, qarang $log)" sent=0
    for _ in $(seq 1 140); do
        sleep 1
        if grep -qaiE 'panic|yiqildi' "$log"; then
            res="FAIL vps ($tag): $(grep -aiE -m2 'panic|yiqildi' "$log" | tr -d '\r' | tr '\n' ' ' | cut -c1-300)"; break
        fi
        if [ $sent -eq 0 ] && grep -qaE 'Choose your language' "$log"; then
            printf '3\r' >"$fifo"; sent=1; continue
        fi
        if [ $sent -ge 1 ] && ! grep -qaE 'First-time setup' "$log"; then
            sent=$((sent + 1))
            [ $sent -gt 15 ] && { res="FAIL vps ($tag): seriya klaviatura uzilişi (GICv3) işlamadi — '3' qabul qilinmadi"; break; }
            continue
        fi
        if [ $sent -ge 1 ]; then
            if ! grep -qa 'UEFI: xotira xaritasi' "$log"; then res="FAIL vps ($tag): EFI yöli işlamadi (yadro boşqa yöl bilan yuklandi?)"
            elif ! grep -qaE "GICv$g" "$log"; then res="FAIL vps ($tag): GICv$g aniqlanmadi"
            elif [ -z "$mx" ] && ! grep -qaE "ACPI: RSDP v[0-9]+ → (XSDT|RSDT): .*checksum xatosi: 0" "$log"; then res="FAIL vps ($tag): ACPI RSDP/XSDT öqilmadi"
            elif [ -z "$mx" ] && ! grep -qaE "ACPI MADT: GICv$g, GICD 0x[0-9a-f]+, GIC[RC] 0x[0-9a-f]+ .* $smp ta CPU" "$log"; then res="FAIL vps ($tag): ACPI MADT (GIC, $smp CPU) notoʻgʻri"
            elif [ -z "$mx" ] && ! grep -qaE "ACPI GTDT: .*NS EL1 30" "$log"; then res="FAIL vps ($tag): ACPI GTDT taymer uzilişi öqilmadi"
            elif [ -z "$mx" ] && ! grep -qaE "ACPI SPCR: PL011 0x9000000, GSIV 33; MCFG: ECAM 0x[0-9a-f]+ " "$log"; then res="FAIL vps ($tag): ACPI SPCR/MCFG öqilmadi"
            elif [ -z "$mx" ] && ! grep -qaE "ACPI → DTB: taymer INTID 30, PSCI CPU_ON uçun $smp ta MPIDR" "$log"; then res="FAIL vps ($tag): ACPI dan yasalgan DTB (timer/cpus) notoʻgʻri"
            elif [ -n "$mx" ] && grep -qaE "DTB yo'q" "$log"; then res="FAIL vps ($tag): acpi=off da DTB topilmadi"
            elif ! grep -qaE "\[smp \] $smp ta" "$log"; then res="FAIL vps ($tag): $smp ta yadro işga tuşmadi"
            elif ! grep -qaE '\[disk\] virtio-blk' "$log"; then res="FAIL vps ($tag): virtio-blk-pci disk topilmadi"
            elif ! grep -qaE 'uziliş orqali tugadi' "$log"; then res="FAIL vps ($tag): PCI disk uzilişi sinovi ötmadi"
            elif ! grep -qaE "SGI: .*\\(GICv$g\\) — OK" "$log"; then res="FAIL vps ($tag): GICv$g SGI sinovi ötmadi"
            elif ! grep -qaE 'taymer: .* — OK' "$log"; then res="FAIL vps ($tag): taymer uzilişi sinovi ötmadi"
            elif ! grep -qaE 'kiriş: 2 ta virtio-input' "$log"; then res="FAIL vps ($tag): virtio-input-pci (klaviatura/planşet) topilmadi"
            elif ! grep -qaE '\[net \] virtio-net' "$log"; then res="FAIL vps ($tag): virtio-net-pci topilmadi"
            elif ! grep -qaE '\[gpu \] virtio-gpu' "$log"; then res="FAIL vps ($tag): virtio-gpu-pci topilmadi"
            elif ! grep -qaE 'manbalar: virtio-rng' "$log"; then res="FAIL vps ($tag): virtio-rng-pci topilmadi"
            else res="OK vps ($tag): UEFI (BOOTAA64.EFI) → $(if [ -z "$mx" ]; then echo "ACPI (MADT, GTDT, SPCR, MCFG)"; else echo DTB; fi) → GICv$g (taymer, SGI, seriya klaviatura), virtio-pci (blk, net, gpu, input), sozlaş ekrani ($(grep -acE '\[ok\]|— OK' "$log") self-test)"; fi
            break
        fi
        kill -0 $qpid 2>/dev/null || { res="FAIL vps ($tag): qemu erta çiqdi"; break; }
    done
    kill $qpid 2>/dev/null; wait $qpid 2>/dev/null; kill $holder 2>/dev/null; wait $holder 2>/dev/null
    rm -f "$esp" "$sys" "$fifo"
    echo "$res"
    [[ "$res" == OK* ]]
}
# Uç xil VPS sinovi: ACPI + GICv3 (Netcup ga öxşaş), ACPI + GICv2 (MADT GICC yöli, 2 CPU),
# UEFI + DTB (acpi=off). Çiqiş: 0 — ötdi, 1 — xato, 2 — firmware yöq (ötkazib yuborildi).
vps_all() {
    local rc=0
    vps_test 3 4 || rc=$?
    [ $rc -ne 0 ] && return $rc
    vps_test 2 2 || return 1
    vps_test 3 4 ",acpi=off" || return 1
}
# --- DTB yöli + virtio-pci: -kernel, GICv2 va GICv3, faqat PCI qurilmalar (ECAM DTB dagi
#     pci-host-ecam-generic dan, uzilişlar INTx/interrupt-map orqali) ---
pci_test() {
    local kernel=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel
    local g sys log res
    for g in 2 3; do
        sys=$(mktemp /tmp/zamin-pci-XXXX.img); truncate -s 128M "$sys"
        log=$LOG_DIR/verify-pci-gic$g.log; : >"$log"
        timeout 100 qemu-system-aarch64 -machine virt,gic-version=$g -cpu max -m 512M -smp 4 -nographic \
            -kernel "$kernel" -drive "file=$sys,if=none,format=raw,id=d" -device virtio-blk-pci,drive=d \
            -device virtio-rng-pci -nic user,model=virtio-net-pci \
            -device virtio-gpu-pci -device virtio-keyboard-pci -device virtio-tablet-pci </dev/null >"$log" 2>&1 &
        local qpid=$!
        res="FAIL pci (GICv$g): timeout (qarang $log)"
        for _ in $(seq 1 90); do
            sleep 1
            if grep -qaiE 'panic|yiqildi' "$log"; then
                res="FAIL pci (GICv$g): $(grep -aiE -m2 'panic|yiqildi' "$log" | tr -d '\r' | tr '\n' ' ' | cut -c1-300)"; break
            fi
            if grep -qaE 'Choose your language' "$log"; then
                if ! grep -qaE 'PCIe: virtio-pci qurilmalari: .*blk' "$log"; then res="FAIL pci (GICv$g): ECAM sanaş (DTB) işlamadi"
                elif ! grep -qaE '\[disk\] virtio-blk' "$log"; then res="FAIL pci (GICv$g): virtio-blk-pci topilmadi"
                elif ! grep -qaE 'uziliş orqali tugadi' "$log"; then res="FAIL pci (GICv$g): INTx disk uzilişi ötmadi"
                elif ! grep -qaE '\[net \] virtio-net' "$log"; then res="FAIL pci (GICv$g): virtio-net-pci topilmadi"
                elif ! grep -qaE '\[gpu \] virtio-gpu' "$log"; then res="FAIL pci (GICv$g): virtio-gpu-pci topilmadi"
                elif ! grep -qaE 'kiriş: 2 ta virtio-input' "$log"; then res="FAIL pci (GICv$g): virtio-input-pci topilmadi"
                elif ! grep -qaE 'manbalar: virtio-rng' "$log"; then res="FAIL pci (GICv$g): virtio-rng-pci topilmadi"
                else res="OK"; fi
                break
            fi
            kill -0 $qpid 2>/dev/null || { res="FAIL pci (GICv$g): qemu erta çiqdi"; break; }
        done
        kill $qpid 2>/dev/null; wait $qpid 2>/dev/null; rm -f "$sys"
        [ "$res" = OK ] || { echo "$res"; return 1; }
    done
    echo "OK pci: -kernel + DTB ECAM, GICv2 va GICv3 — virtio-pci (blk+INTx, net, gpu, input, rng)"
}
if [ "$MODE" = "pci" ]; then
    pci_test; exit $?
fi
if [ "$MODE" = "vps" ]; then
    vps_all; rc=$?; [ $rc -eq 2 ] && exit 0; exit $rc
fi

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

# --- DTB + virtio-pci (GICv2/GICv3) ---
pci_test || exit 1

# --- VPS (UEFI) yuklaniş ---
vps_all; rc=$?
[ $rc -eq 1 ] && exit 1
exit 0
