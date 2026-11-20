#!/usr/bin/env bash
# Zamin OS — VPS ga o'rnatish (Debian 12 / Ubuntu 22.04 / 24.04, x86_64 yoki arm64).
#
# Ishlatish (toza o'rnatilgan VPS da, root sifatida):
#   curl -fsSL https://raw.githubusercontent.com/abbosjon-dev/os/main/scripts/vps/ornat.sh -o ornat.sh
#   bash ornat.sh
#
# Natija: server yoqilishi bilan Zamin OS avtomatik ishga tushadi; ekran brauzerda
#   https://<IP>:6080/  (parol bilan) ochiladi. Zamin OS diski /opt/zamin/disk.img da
#   saqlanadi — qayta yuklanganda fayllar va hisoblar o'chmaydi.
# Ichida: minimal Linux + QEMU (ARM64 mashina) + Zamin OS yadrosi.
#
# Bayroqlar:  --ha            tasdiq so'ramasdan
#             --kernel FAYL   yadroni internetdan emas, shu fayldan olish
#             --port N        veb-ekran porti (standart 6080)
#             --olib-tashla   Zamin OS xizmatlarini o'chirish va olib tashlash
#             --no-systemd    (sinov uchun) xizmat o'rniga hozir ishga tushirish
set -euo pipefail

REPO_RAW="https://raw.githubusercontent.com/abbosjon-dev/os/main"
DIR=/opt/zamin
PORT=6080
HA=0; NEW_DISK=0; KERNEL_SRC=""; NOSYSTEMD=0; REMOVE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --ha) HA=1 ;;
    --kernel) KERNEL_SRC="$2"; shift ;;
    --port) PORT="$2"; shift ;;
    --olib-tashla) REMOVE=1 ;;
    --no-systemd) NOSYSTEMD=1 ;;
    *) echo "Noma'lum bayroq: $1"; exit 1 ;;
  esac
  shift
done

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mXATO:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "root sifatida ishga tushiring (sudo bash ornat.sh)"

if [ "$REMOVE" = 1 ]; then
  systemctl disable --now zamin-web zamin 2>/dev/null || true
  rm -f /etc/systemd/system/zamin.service /etc/systemd/system/zamin-web.service
  systemctl daemon-reload 2>/dev/null || true
  echo "Xizmatlar o'chirildi. Zamin OS diski saqlanib qoldi: $DIR (o'chirish: rm -rf $DIR)"
  exit 0
fi

command -v apt-get >/dev/null || die "faqat Debian/Ubuntu qo'llab-quvvatlanadi"
ARCH=$(uname -m)
MEM_MB=$(awk '/MemTotal/{print int($2/1024)}' /proc/meminfo)
CPUS=$(nproc)
case "$ARCH" in x86_64|aarch64) ;; *) die "arxitektura qo'llab-quvvatlanmaydi: $ARCH" ;; esac
[ "$MEM_MB" -ge 700 ] || die "xotira juda kam (${MEM_MB} MB). Kamida 1 GB, tavsiya 2 GB."

# Zamin OS ga beriladigan resurslar
if [ "$MEM_MB" -ge 1800 ]; then ZMEM=1024; else ZMEM=512; fi
ZCPU=$(( CPUS > 4 ? 4 : CPUS )); [ "$ZCPU" -ge 1 ] || ZCPU=1
ACCEL="-cpu max"
if [ "$ARCH" = aarch64 ] && [ -e /dev/kvm ]; then ACCEL="-accel kvm -cpu host"; fi

cat <<EOF

  Zamin OS o'rnatiladi
  ---------------------
  Server:      $ARCH, ${MEM_MB} MB xotira, ${CPUS} vCPU
  Zamin OS:    ${ZMEM} MB, ${ZCPU} yadro ($( [ "$ARCH" = x86_64 ] && echo "ARM64 emulyatsiya — biroz sekin" || echo "ARM64" ))
  Papka:       $DIR
  Veb-ekran:   https://<IP>:${PORT}/  (parol bilan)
  O'rnatiladi: qemu-system-arm, novnc, websockify, openssl (apt orqali)
  SSH (22-port) TEGILMAYDI — server boshqaruvi saqlanadi.

EOF
if [ "$HA" != 1 ]; then
  read -r -p "Davom etamizmi? (ha/yo'q): " J
  case "$J" in ha|Ha|HA|y|Y|yes) ;; *) echo "Bekor qilindi."; exit 0 ;; esac
fi

say "Paketlar o'rnatilmoqda"
export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -y -q --no-install-recommends qemu-system-arm novnc websockify openssl curl python3 ca-certificates socat

mkdir -p "$DIR"
chmod 700 "$DIR"

say "Zamin OS yadrosi olinmoqda"
if [ -n "$KERNEL_SRC" ]; then
  cp "$KERNEL_SRC" "$DIR/kernel.new"
else
  curl -fsSL "$REPO_RAW/dist/zamin-kernel" -o "$DIR/kernel.new"
  curl -fsSL "$REPO_RAW/dist/zamin-kernel.sha256" -o "$DIR/kernel.sha256"
  ( cd "$DIR" && echo "$(cut -d' ' -f1 kernel.sha256)  kernel.new" | sha256sum -c --quiet - ) \
    || die "yadro nazorat summasi mos kelmadi"
fi
head -c4 "$DIR/kernel.new" | grep -q $'\x7fELF' || die "yadro fayli ELF emas"
mv "$DIR/kernel.new" "$DIR/kernel"

if [ ! -f "$DIR/disk.img" ]; then
  say "Zamin OS diski yaratilmoqda (2 GB)"
  truncate -s 2G "$DIR/disk.img"; NEW_DISK=1
fi

if [ ! -f "$DIR/parol.txt" ]; then
  # VNC paroli 8 belgigacha; chalkash belgilar (0/O, 1/l/I) yo'q
  python3 -c "import secrets; a='ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789'; print(''.join(secrets.choice(a) for _ in range(8)))" > "$DIR/parol.txt"
  chmod 600 "$DIR/parol.txt"
fi
if [ ! -f "$DIR/cert.pem" ]; then
  say "TLS sertifikat yaratilmoqda"
  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=Zamin OS" \
    -keyout "$DIR/key.pem" -out "$DIR/cert.pem" >/dev/null 2>&1
  chmod 600 "$DIR/key.pem"
fi

# Veb sahifa: noVNC + bosh sahifa to'g'ridan-to'g'ri ekranga yo'naltiradi
WEB="$DIR/web"
rm -rf "$WEB"; mkdir -p "$WEB"
cp -r /usr/share/novnc/. "$WEB/"
cat > "$WEB/index.html" <<'EOF'
<!doctype html><meta charset="utf-8"><title>Zamin OS</title>
<meta http-equiv="refresh" content="0; url=vnc.html?autoconnect=1&resize=scale&reconnect=1&show_dot=1">
<a href="vnc.html?autoconnect=1&resize=scale">Zamin OS ekrani</a>
EOF

# QEMU ishga tushirish skripti
cat > "$DIR/run.sh" <<EOF
#!/usr/bin/env bash
# Zamin OS mashinasi (zamin.service orqali ishga tushadi)
set -e
rm -f /run/zamin-qmp.sock /run/zamin-konsol.sock
qemu-system-aarch64 -machine virt $ACCEL -smp $ZCPU -m ${ZMEM}M \\
  -device virtio-gpu-device -device virtio-keyboard-device -device virtio-tablet-device \\
  -drive file=$DIR/disk.img,if=none,format=raw,id=d0 -device virtio-blk-device,drive=d0 \\
  -device virtio-rng-device -netdev user,id=n0 -device virtio-net-device,netdev=n0 \\
  -display none -vnc 127.0.0.1:0,password=on -qmp unix:/run/zamin-qmp.sock,server,nowait \\
  -chardev socket,id=s0,path=/run/zamin-konsol.sock,server=on,wait=off,logfile=$DIR/konsol.log,logappend=on \\
  -serial chardev:s0 -audiodev none,id=snd0 -kernel $DIR/kernel &
QPID=\$!
# VNC parolini QMP orqali o'rnatish
python3 - <<'PY'
import json, socket, time
for _ in range(100):
    try:
        s = socket.socket(socket.AF_UNIX); s.connect("/run/zamin-qmp.sock"); break
    except OSError:
        time.sleep(0.1)
f = s.makefile("rw")
f.readline()
def cmd(c, a=None):
    f.write(json.dumps({"execute": c, **({"arguments": a} if a else {})}) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r:
            return r
cmd("qmp_capabilities")
r = cmd("change-vnc-password", {"password": open("$DIR/parol.txt").read().strip()})
assert "return" in r, r
PY
wait \$QPID
EOF
chmod 700 "$DIR/run.sh"

# SSH orqali Zamin OS konsoli (birinchi sozlash va terminal): zamin-konsol
cat > /usr/local/bin/zamin-konsol <<'EOF'
#!/usr/bin/env bash
# Zamin OS serial konsoli. Chiqish: Ctrl+]
[ -S /run/zamin-konsol.sock ] || { echo "Zamin OS ishlamayapti (systemctl status zamin)"; exit 1; }
echo "Zamin OS konsoli — chiqish: Ctrl+]   (Enter bosing)"
exec socat -,raw,echo=0,escape=0x1d UNIX-CONNECT:/run/zamin-konsol.sock
EOF
chmod 755 /usr/local/bin/zamin-konsol

WEBCMD="websockify --web $WEB --cert $DIR/cert.pem --key $DIR/key.pem --ssl-only 0.0.0.0:$PORT 127.0.0.1:5900"

if [ "$NOSYSTEMD" = 1 ]; then
  say "Sinov rejimi: xizmatlarsiz ishga tushirilmoqda"
  nohup "$DIR/run.sh" >"$DIR/run.log" 2>&1 &
  nohup $WEBCMD >"$DIR/web.log" 2>&1 &
else
  say "Xizmatlar sozlanmoqda (server yoqilganda avtomatik ishga tushadi)"
  cat > /etc/systemd/system/zamin.service <<EOF
[Unit]
Description=Zamin OS
After=network.target

[Service]
ExecStart=$DIR/run.sh
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
  cat > /etc/systemd/system/zamin-web.service <<EOF
[Unit]
Description=Zamin OS veb-ekrani (noVNC)
After=zamin.service

[Service]
ExecStart=$WEBCMD
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  systemctl enable --now zamin.service zamin-web.service
  systemctl restart zamin.service zamin-web.service
  if command -v ufw >/dev/null && ufw status | grep -q "Status: active"; then
    ufw allow 22/tcp >/dev/null; ufw allow "$PORT"/tcp >/dev/null
  fi
fi

IP=$(curl -fsS --max-time 5 https://api.ipify.org 2>/dev/null || hostname -I | awk '{print $1}')
cat <<EOF

  ✅ Zamin OS o'rnatildi va ishga tushdi.

  Ekran:   https://$IP:$PORT/
           (Brauzer "xavfsiz emas" desa — bu o'z sertifikatimiz: "Batafsil → Davom etish")
  Parol:   $(cat "$DIR/parol.txt")      (saqlangan: $DIR/parol.txt)

  Birinchi sozlash (til, ism, login, parol) — SSH da:  zamin-konsol
  (Zamin OS konsoli; chiqish: Ctrl+])
  Boshqaruv:  systemctl status zamin   |  systemctl restart zamin
  Konsol:     zamin-konsol   (jurnal: $DIR/konsol.log)
  Yangilash:  bash ornat.sh --ha       (disk va hisoblar saqlanadi)
  Olib tashlash: bash ornat.sh --olib-tashla

EOF

if [ "$NEW_DISK" = 1 ] && [ -t 0 ] && [ "$HA" != 1 ]; then
  read -r -p "Birinchi sozlashni hozir boshlaymizmi? (ha/yo'q): " J
  case "$J" in ha|Ha|HA|y|Y|yes) sleep 3; /usr/local/bin/zamin-konsol ;; esac
fi
