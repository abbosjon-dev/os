#!/usr/bin/env bash
# Zamin OS — mustaqil tekşiruv (supervisor "DONE" ga ishonmaydi, şuni işlatadi).
#   verify.sh quick — yiğiş: xato va ogohlantirish yöq
#   verify.sh full  — + QEMU da (grafikasiz) yuklaniş: barça self-test [ok], panic yöq;
#                     + brauzer sahifalari surati: yumaloq burçaklarda töq piksel yöq
#                     + pci sinovi (DTB + virtio-pci, GICv2/GICv3)
#                     + vps sinovi (UEFI firmware örnatilgan bölsa)
#                     + obraz sinovi (scripts/vps/obraz.sh GPT obrazi, UEFI)
#   verify.sh vps   — yiğiş + UEFI yuklaniş (Netcup ARM VPS ga öxşaş): BOOTAA64.EFI FAT32
#                     diskda, virt,gic-version=3, QEMU_EFI.fd, virtio-*-pci qurilmalar —
#                     til tanlaş ekranigaça, panic yöq, PCI disk uzilişi işlaydi; GICv3 da
#                     SGI (barça yadrolar), seriya klaviatura uzilişi ("3" → English sozlaş),
#                     virtio-gpu/keyboard/tablet-pci, virtio-net/rng-pci;
#                     ACPI zaxira yöli (DTB yöq): RSDP/XSDT, MADT (GIC, CPU lar), GTDT (taymer),
#                     SPCR (UART), MCFG (ECAM) — GICv3/4 CPU va GICv2/2 CPU; + acpi=off (UEFI + DTB);
#                     + USB kiriş: qemu-xhci + usb-kbd + usb-tablet (QMP send-key / input-send-event)
#                     + -cpu neoverse-n1 (Netcup Ampere: RNDR/PAuth/BTI/MTE yöq, ID_AA64* hisoboti)
#                     + GOP zaxira ekran: virtio-gpu yöq, -device ramfb → UEFI GOP framebuffer
#                       (jurnalda 'GOP: WxH, format …', screendump bir rangli emas, docs/vps-gop.png)
#   verify.sh obraz — yiğiş + scripts/vps/obraz.sh: bitta GPT disk (ESP + Zamin bölimi), UEFI
#                     (QEMU_EFI.fd -bios yoki AAVMF pflash), GICv3, faqat virtio-*-pci: GPT bölimi
#                     topiladi, grafik ekran (virtio-gpu), DHCP; sozlaş ustasi oxirigaça → sozlamalar
#                     Zamin bölimiga yoziladi (MBR/GPT butun) → qayta yuklanişda diskdan öqiladi
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
#   4-parametr CPU (standart max): neoverse-n1 — Netcup ARM (Ampere Altra) ga öxşaş, v8.2:
#   RNDR/PAuth/BTI/MTE yöq — yadro 'max' imkoniyatlariga yaşirin tayanmasligi tekşiriladi.
vps_test() {
    local g=${1:-3} smp=${2:-4} mx=${3:-} cpu=${4:-max}
    local tag="GICv$g, $smp CPU${mx:+, ${mx#,}}"
    [ "$cpu" = max ] || tag="$tag, -cpu $cpu"
    local kernel=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel
    local fw="" f
    for f in /usr/share/qemu-efi-aarch64/QEMU_EFI.fd /usr/share/AAVMF/AAVMF_CODE.fd; do
        [ -f "$f" ] && { fw=$f; break; }
    done
    [ -n "$fw" ] || { echo "SKIP vps: UEFI firmware yöq (qemu-efi-aarch64)"; return 2; }
    local esp sys log=$LOG_DIR/verify-vps.log
    [ "$g$smp$mx$cpu" = 34max ] || log=$LOG_DIR/verify-vps-gic$g-smp$smp${mx//[^a-z]/}${cpu/#max/}.log
    esp=$(mktemp /tmp/zamin-esp-XXXX.img); sys=$(mktemp /tmp/zamin-vps-XXXX.img)
    if ! sh scripts/efi/mkesp.sh "$kernel" "$esp" >"$LOG_DIR/verify-mkesp.log" 2>&1; then
        echo "FAIL vps ($tag): ESP yasalmadi ($(tail -1 "$LOG_DIR/verify-mkesp.log"))"; rm -f "$esp" "$sys"; return 1
    fi
    truncate -s 128M "$sys"; : >"$log"
    # stdin — FIFO: til tanlaş ekranida "3" yuboriladi (PL011 uzilişi GICv3 orqali)
    local fifo; fifo=$(mktemp -u /tmp/zamin-vps-in-XXXX); mkfifo "$fifo"
    sleep 200 >"$fifo" &
    local holder=$!
    timeout 150 qemu-system-aarch64 -machine virt,gic-version=$g$mx -cpu "$cpu" -m 1G -smp $smp -nographic \
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
            elif ! grep -qaE '  CPU: MIDR 0x[0-9a-f]+ .*ID_AA64: PAN' "$log"; then res="FAIL vps ($tag): ID_AA64* hisoboti yöq"
            elif [ "$cpu" = neoverse-n1 ] && ! grep -qaE 'CPU: MIDR 0x414fd0c[0-9a-f] \(Neoverse-N1\).*RNDR yöq, PAuth yöq, BTI yöq, MTE yöq' "$log"; then res="FAIL vps ($tag): Neoverse-N1 ID_AA64* imkoniyatlari kutilganidek emas"
            elif [ "$cpu" = neoverse-n1 ] && ! grep -qaE '\[ok\]' "$log"; then res="FAIL vps ($tag): self-test [ok] yöq"
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
    vps_test 3 4 "" neoverse-n1 || return 1
    vps_usb_test || return 1
    vps_gop_test || return 1
}
# GOP zaxira ekran: virtio-gpu YÖQ, -device ramfb (EDK2 GOP) — surat docs/vps-gop.png
vps_gop_test() {
    local kernel=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel res rc
    res=$(timeout 220 python3 scripts/agent/vps_gop.py "$kernel" "$LOG_DIR/verify-vps-gop.log" docs/vps-gop.png 2>&1); rc=$?
    echo "${res:-FAIL vps-gop: timeout (qarang $LOG_DIR/verify-vps-gop.log)}"
    [ $rc -eq 2 ] && return 0
    [ $rc -eq 0 ]
}
# USB kiriş (Netcup VNC): virtio-keyboard/tablet-pci ÖRNIGA qemu-xhci + usb-kbd + usb-tablet;
# til tanlaş ekraniga QMP send-key va input-send-event (abs) — surat docs/vps-usb.png
vps_usb_test() {
    local kernel=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/debug/kernel res rc
    res=$(timeout 220 python3 scripts/agent/vps_usb.py "$kernel" "$LOG_DIR/verify-vps-usb.log" docs/vps-usb.png 2>&1); rc=$?
    echo "${res:-FAIL vps-usb: timeout (qarang $LOG_DIR/verify-vps-usb.log)}"
    [ $rc -eq 2 ] && return 0
    [ $rc -eq 0 ]
}
# --- VPS disk obrazi (scripts/vps/obraz.sh): bitta GPT disk — ESP + Zamin bölimi ---
#   Netcup VPS 500 ARM ga öxşaş: virt,gic-version=3, UEFI, virtio-*-pci. Ikki marta yuklanadi:
#   1) toza obraz → GPT bölimi, grafika, DHCP, sozlaş ustasi (seriya port orqali) oxirigaça;
#   2) obraz tuzilmasi tekşiriladi (MBR 0xEE, GPT sarlavhasi, bölim boşida ZAMINDSK) va
#      qayta yuklanişda sozlamalar diskdan öqiladi.
obraz_boot() { # obraz_boot IMG LOG FIFO — QEMU ni fonda işga tuşiradi, PID → QPID
    local img=$1 log=$2 fifo=$3 fwargs
    if [ -f /usr/share/qemu-efi-aarch64/QEMU_EFI.fd ]; then
        fwargs=(-bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd)
    else
        cp /usr/share/AAVMF/AAVMF_VARS.fd "$img.vars"
        fwargs=(-drive if=pflash,format=raw,readonly=on,file=/usr/share/AAVMF/AAVMF_CODE.fd
                -drive "if=pflash,format=raw,file=$img.vars")
    fi
    : >"$log"
    timeout 200 qemu-system-aarch64 -machine virt,gic-version=3 -cpu max -m 1G -smp 2 -nographic "${fwargs[@]}" \
        -drive "file=$img,if=none,format=raw,id=d" -device virtio-blk-pci,drive=d \
        -nic user,model=virtio-net-pci -device virtio-gpu-pci -device virtio-keyboard-pci \
        -device virtio-tablet-pci -device virtio-rng-pci <"$fifo" >"$log" 2>&1 &
    QPID=$!
}
obraz_wait() { # obraz_wait LOG REGEX SEKUND — 0: topildi, 1: panic/qemu çiqdi/timeout
    local i
    for i in $(seq 1 "$3"); do
        grep -qaiE 'panic|yiqildi' "$1" && return 1
        grep -qaE "$2" "$1" && return 0
        kill -0 "$QPID" 2>/dev/null || return 1
        sleep 1
    done
    return 1
}
obraz_test() {
    [ -f /usr/share/qemu-efi-aarch64/QEMU_EFI.fd ] || [ -f /usr/share/AAVMF/AAVMF_CODE.fd ] \
        || { echo "SKIP obraz: UEFI firmware yöq"; return 2; }
    local img log1=$LOG_DIR/verify-obraz-1.log log2=$LOG_DIR/verify-obraz-2.log fifo res="" holder
    img=$(mktemp /tmp/zamin-obraz-XXXX.img)
    if ! bash scripts/vps/obraz.sh --hajm 2G "$img" >"$LOG_DIR/verify-obraz-mk.log" 2>&1; then
        echo "FAIL obraz: obraz.sh xato ($(tail -1 "$LOG_DIR/verify-obraz-mk.log"))"; rm -f "$img"*; return 1
    fi
    fifo=$(mktemp -u /tmp/zamin-obraz-in-XXXX); mkfifo "$fifo"
    sleep 400 >"$fifo" & holder=$!
    obraz_boot "$img" "$log1" "$fifo"
    local fail="FAIL obraz (1-yuklaniş)"
    if ! obraz_wait "$log1" 'Choose your language' 120; then res="$fail: til tanlaş ekrani çiqmadi ($(grep -aiE -m1 'panic|yiqildi' "$log1" | tr -d '\r' | cut -c1-200); qarang $log1)"
    elif ! grep -qa 'UEFI: xotira xaritasi' "$log1"; then res="$fail: UEFI (BOOTAA64.EFI) yöli işlamadi"
    elif ! grep -qaE 'GICv3' "$log1"; then res="$fail: GICv3 aniqlanmadi"
    elif ! grep -qaE '\[disk\] GPT: .* #2 \(LBA 133120' "$log1"; then res="$fail: GPT dagi Zamin bölimi topilmadi"
    elif ! grep -qaE '\[gpu \] virtio-gpu: [0-9]+x[0-9]+' "$log1"; then res="$fail: grafik ekran (virtio-gpu-pci) işlamadi"
    elif ! obraz_wait "$log1" '\[net \] DHCP: IP 10\.0\.2\.15' 30; then res="$fail: DHCP (virtio-net-pci) işlamadi"
    else
        printf '3\r' >"$fifo"
        if ! obraz_wait "$log1" 'full name' 30; then res="$fail: sozlaş ustasi ('3' qabul qilinmadi)"
        else
            printf 'Test User\r' >"$fifo"; obraz_wait "$log1" 'sername|Login:' 30
            printf 'test\r' >"$fifo"; obraz_wait "$log1" 'Password:' 30
            printf 'pass1234\r' >"$fifo"; obraz_wait "$log1" 'epeat' 30
            printf 'pass1234\r' >"$fifo"
            sleep 2
            # sozlamalar diskka yozilgunça (bölim boşida ZAMINDSK) kutamiz
            local k
            for k in $(seq 1 60); do
                python3 - "$img" <<'PY' && break
import sys
f = open(sys.argv[1], "rb"); f.seek(133120 * 512)
sys.exit(0 if f.read(8) == b"ZAMINDSK" else 1)
PY
                sleep 1
            done
        fi
    fi
    kill $QPID 2>/dev/null; wait $QPID 2>/dev/null
    if [ -z "$res" ]; then
        if ! python3 - "$img" >"$LOG_DIR/verify-obraz-gpt.log" 2>&1 <<'PY'
import struct, sys, zlib
f = open(sys.argv[1], "rb"); d = lambda lba, n=1: (f.seek(lba * 512), f.read(512 * n))[1]
mbr = d(0); assert mbr[450] == 0xEE and mbr[510:] == b"\x55\xaa", "himoya MBR buzilgan"
total = f.seek(0, 2) // 512
for lba in (1, total - 1):
    h = d(lba); assert h[:8] == b"EFI PART", f"GPT sarlavhasi (LBA {lba}) buzilgan"
    hc = bytearray(h[:92]); hc[16:20] = bytes(4)
    assert zlib.crc32(hc) == struct.unpack_from("<I", h, 16)[0], f"GPT CRC (LBA {lba}) xato"
    el = struct.unpack_from("<Q", h, 72)[0]
    assert zlib.crc32(d(el, 32)) == struct.unpack_from("<I", h, 88)[0], "bölimlar jadvali buzilgan"
assert d(2048)[82:90] == b"FAT32   ", "ESP (FAT32) buzilgan"
assert d(133120)[:8] == b"ZAMINDSK", "sozlamalar Zamin bölimiga yozilmadi"
print("ok")
PY
        then res="FAIL obraz: obraz tuzilmasi: $(tail -1 "$LOG_DIR/verify-obraz-gpt.log")"
        else
            obraz_boot "$img" "$log2" "$fifo"
            if ! obraz_wait "$log2" '\[conf\] settings loaded from disk \(account: test\)' 120; then
                res="FAIL obraz (2-yuklaniş): sozlamalar Zamin bölimidan öqilmadi (qarang $log2)"
            else
                res="OK obraz: GPT (ESP + Zamin bölimi) → UEFI → GICv3, virtio-pci: grafika, DHCP, sozlaş → bölimga yozildi (MBR/GPT/ESP butun) → qayta yuklanişda öqildi"
            fi
            kill $QPID 2>/dev/null; wait $QPID 2>/dev/null
        fi
    fi
    kill $holder 2>/dev/null; wait $holder 2>/dev/null
    rm -f "$img" "$img.vars" "$img.sha256" "$fifo"
    echo "$res"
    [[ "$res" == OK* ]]
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
if [ "$MODE" = "obraz" ]; then
    obraz_test; rc=$?; [ $rc -eq 2 ] && exit 0; exit $rc
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
#   Ekran ölçamlari: 1280x800 (QEMU standarti), 1024x768, 1920x1080 (Netcup VNC) — virtio-gpu
#   xres/yres (yadro GET_DISPLAY_INFO orqali oladi), hamda VPS yöli: UEFI + ACPI + GICv3 +
#   virtio-gpu-pci (1920x1080). Koordinatalar ("@browser", "@max", ...) zgeom.py dan.
CDIR="$LOG_DIR/corners"; mkdir -p "$CDIR" /tmp/claude-0; rm -f "$CDIR"/*.png "$CDIR"/*.txt
M="m,@park;w,1"
# 1280x800: brauzer (dok) → kattalaştiriş → zamin://matn (boşi, yarim aylantirilgan) → zamin://bezak → zamin://overflow
CLICKS="@browser;w,2;@max;w,1;u,zamin://matn;$M;snap,matn;m,@center;wd,8;$M;snap,matn2;u,zamin://bezak;$M;snap,bezak;u,zamin://overflow;$M" \
    timeout 300 python3 scripts/agent/shot.py "$KERNEL" "$CDIR/overflow.png" fresh >"$CDIR/shot.log" 2>&1
SHOTS=("$CDIR/overflow_matn.png" "$CDIR/overflow_matn2.png" "$CDIR/overflow_bezak.png" "$CDIR/overflow.png")
# boşqa ölçamlar va VPS yöli: zamin://matn (boşi va yarim aylantirilgan)
CM="@browser;w,2;@max;w,1;u,zamin://matn;$M;snap,top;m,@center;wd,8;$M"
VPS_FW=""
for f in /usr/share/qemu-efi-aarch64/QEMU_EFI.fd /usr/share/AAVMF/AAVMF_CODE.fd; do [ -f "$f" ] && { VPS_FW=$f; break; }; done
for cfg in "1024x768" "1920x1080" "1920x1080 --vps"; do
    sz=${cfg%% *}; tag=matn-$sz; [[ "$cfg" == *--vps ]] && { [ -n "$VPS_FW" ] || continue; tag=vps-$sz; }
    # shellcheck disable=SC2086
    CLICKS="$CM" timeout 420 python3 scripts/agent/shot.py "$KERNEL" "$CDIR/$tag.png" fresh --size $cfg >"$CDIR/shot-$tag.log" 2>&1
    SHOTS+=("$CDIR/${tag}_top.png" "$CDIR/$tag.png")
done
for f in "${SHOTS[@]}"; do
    [ -s "$f" ] || { echo "FAIL burçaklar: surat olinmadi ($f, qarang $CDIR/shot*.log)"; exit 1; }
done
# surat ölçami sorovga mos kelişi şart (yadro boşqa ölçamga qaytib ketmasin)
for f in "${SHOTS[@]}"; do
    want=1280x800; [[ "$f" =~ -([0-9]+x[0-9]+) ]] && want=${BASH_REMATCH[1]}
    got=$(python3 -c "from PIL import Image; import sys; print('%dx%d' % Image.open(sys.argv[1]).size)" "$f")
    [ "$got" = "$want" ] || { echo "FAIL burçaklar: $(basename "$f") ölçami $got (kutilgan $want)"; exit 1; }
done
[ -f "$CDIR/vps-1920x1080.png.log" ] && ! grep -qaE "ACPI MADT: GICv3" "$CDIR/vps-1920x1080.png.log" && {
    echo "FAIL burçaklar: VPS surati ACPI + GICv3 yölida olinmadi"; exit 1; }
NBAD=0
for f in "${SHOTS[@]}"; do
    if ! python3 scripts/agent/check_corners.py "$f" >"$f.txt" 2>&1; then
        NBAD=$((NBAD + 1)); echo "  $(basename "$f"): $(grep -m2 -E 'NUQSON|burchaklar' "$f.txt" | tr '\n' ' ')"
    fi
done
[ "$NBAD" -eq 0 ] || { echo "FAIL burçaklar: $NBAD surat nuqsonli (qarang $CDIR)"; exit 1; }
[ -s "$CDIR/matn-1920x1080_top.png" ] && cp "$CDIR/matn-1920x1080_top.png" docs/web-corners-1920.png
# o'z-o'zini sinaş: burçak yaqiniga bitta qora piksel — tekşiruvçi 1 qaytarişi şart (1280 va 1920 suratlarda)
for src in "$CDIR/overflow_bezak.png" "$CDIR/matn-1920x1080_top.png"; do
    [ -s "$src" ] || continue
    python3 - "$src" "$CDIR/selftest.png" <<'PY' || { echo "FAIL burçaklar: o'z-o'zini sinaş surati yasalmadi"; exit 1; }
import subprocess, sys
from PIL import Image
out = subprocess.run([sys.executable, "scripts/agent/check_corners.py", sys.argv[1], "-v"], capture_output=True, text=True).stdout
ln = next(l for l in out.splitlines() if l.startswith("BURCHAK"))
cx, cy, sx, sy = map(int, ln.split()[1:5])
im = Image.open(sys.argv[1]).convert("RGB")
im.putpixel((cx + 2 * sx, cy + 2 * sy), (0, 0, 0))
im.save(sys.argv[2])
PY
    python3 scripts/agent/check_corners.py "$CDIR/selftest.png" >"$CDIR/selftest.txt" 2>&1; st=$?
    [ $st -eq 1 ] || { echo "FAIL burçaklar: qora pikselli surat ($(basename "$src")) tekşiruvdan ötib ketdi (kod $st, check_corners.py buzilgan)"; exit 1; }
done
echo "[ok] burchaklar: ${#SHOTS[@]} surat toza (1280x800, 1024x768, 1920x1080$([ -n "$VPS_FW" ] && echo ", VPS UEFI+ACPI+GICv3 1920x1080"))"

# --- DTB + virtio-pci (GICv2/GICv3) ---
pci_test || exit 1

# --- VPS (UEFI) yuklaniş ---
vps_all; rc=$?
[ $rc -eq 1 ] && exit 1

# --- VPS disk obrazi (GPT, bitta disk) ---
obraz_test; rc=$?
[ $rc -eq 1 ] && exit 1
exit 0
