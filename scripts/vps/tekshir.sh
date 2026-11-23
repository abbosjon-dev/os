#!/usr/bin/env bash
# Zamin OS — VPS tekshiruvi (FAQAT O'QIYDI, hech narsani o'zgartirmaydi).
# VPS da ishga tushiring:  bash tekshir.sh
# Chiqqan natijani to'liq nusxalab yuboring. Parol yoki kalitlar chiqmaydi.
echo "== Zamin OS VPS tekshiruvi =="
echo "arxitektura: $(uname -m)"
echo "yadro: $(uname -r)"
[ -r /etc/os-release ] && echo "tizim: $(. /etc/os-release; echo "$PRETTY_NAME")"
if [ -d /sys/firmware/efi ]; then echo "yuklash: UEFI"; else echo "yuklash: BIOS/legacy"; fi
echo "virtualizatsiya: $(systemd-detect-virt 2>/dev/null || echo "noma'lum")"
echo "ishlab chiqaruvchi: $(cat /sys/class/dmi/id/sys_vendor 2>/dev/null) $(cat /sys/class/dmi/id/product_name 2>/dev/null)"
echo "protsessor: $(grep -m1 -E 'model name|CPU implementer' /proc/cpuinfo | cut -d: -f2 | xargs) ($(nproc) yadro)"
echo "xotira: $(awk '/MemTotal/{printf "%d MB", $2/1024}' /proc/meminfo)"
echo "-- disklar:"
lsblk -d -o NAME,SIZE,TYPE,TRAN,MODEL 2>/dev/null
for d in /sys/block/*; do n=$(basename "$d"); case $n in loop*|ram*) continue;; esac
  drv=$(basename "$(readlink -f "$d/device/driver" 2>/dev/null)" 2>/dev/null); echo "  $n drayver: ${drv:-?}"; done
echo "-- tarmoq:"
for i in /sys/class/net/*; do n=$(basename "$i"); [ "$n" = lo ] && continue
  echo "  $n drayver: $(basename "$(readlink -f "$i/device/driver" 2>/dev/null)" 2>/dev/null)"; done
echo "-- virtio qurilmalar:"
ls /sys/bus/virtio/devices 2>/dev/null | while read v; do
  echo "  $v id=$(cat /sys/bus/virtio/devices/$v/device) bus=$(basename "$(readlink -f /sys/bus/virtio/devices/$v/..)")"; done
echo "-- PCI:"
command -v lspci >/dev/null && lspci -nn 2>/dev/null | head -25 || ls /sys/bus/pci/devices 2>/dev/null | head -25
echo "-- USB kontroller va kiritish qurilmalari (VNC klaviatura/sichqoncha):"
if command -v lspci >/dev/null; then
  lspci -nn 2>/dev/null | grep -iE "usb|0c03" | sed 's/^/  /' || echo "  USB kontroller yo'q"
else
  for d in /sys/bus/pci/devices/*; do
    case $(cat "$d/class" 2>/dev/null) in 0x0c03*) echo "  $(basename "$d") class=$(cat "$d/class") $(cat "$d/vendor"):$(cat "$d/device") ($(basename "$(readlink -f "$d/driver" 2>/dev/null)" 2>/dev/null))";; esac
  done
fi
command -v lsusb >/dev/null && lsusb 2>/dev/null | sed 's/^/  /'
grep -h -E '^N: Name=' /proc/bus/input/devices 2>/dev/null | sed 's/^N: Name=/  kiritish: /'
echo "-- uzilish kontrolleri (ARM):"
grep -m3 -iE "gic" /proc/interrupts 2>/dev/null | awk '{print "  "$NF, $(NF-1)}' | sort -u
dmesg 2>/dev/null | grep -m2 -iE "GICv[234]|gic-v3|gic,v" || true
echo "-- konsol / ekran:"
cat /proc/cmdline
ls /sys/class/drm 2>/dev/null | tr '\n' ' '; echo
echo "-- ACPI/DT: $( [ -d /sys/firmware/acpi/tables ] && echo ACPI ) $( [ -d /sys/firmware/devicetree/base ] && echo DeviceTree )"
echo "== tugadi =="
