#!/usr/bin/env python3
"""Zamin OS VPS obrazini QEMU da (UEFI + GICv3 + virtio-pci) yuklab, ekran suratini oladi.

    surat.py <obraz.img> <çiqiş.png>

Obraz özgartiriladi (sozlaş ustasi seriya port orqali tugatiladi: English, test/pass1234),
şuning uçun nusxasida işlating. QMP screendump → PNG (Pillow).
"""
import json, os, pty, select, socket, subprocess, sys, tempfile, time
from PIL import Image

IMG, OUT = sys.argv[1], sys.argv[2]
tmp = tempfile.mkdtemp()
QMP = os.path.join(tmp, "qmp.sock"); PPM = os.path.join(tmp, "s.ppm")
fw = ["-bios", "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd"]
if not os.path.exists(fw[1]):
    vars_ = os.path.join(tmp, "vars.fd")
    subprocess.run(["cp", "/usr/share/AAVMF/AAVMF_VARS.fd", vars_], check=True)
    fw = ["-drive", "if=pflash,format=raw,readonly=on,file=/usr/share/AAVMF/AAVMF_CODE.fd",
          "-drive", f"if=pflash,format=raw,file={vars_}"]
cmd = ["qemu-system-aarch64", "-machine", "virt,gic-version=3", "-cpu", "max", "-smp", "2", "-m", "1G", *fw,
       "-drive", f"file={IMG},if=none,format=raw,id=d", "-device", "virtio-blk-pci,drive=d",
       "-nic", "user,model=virtio-net-pci", "-device", "virtio-gpu-pci", "-device", "virtio-keyboard-pci",
       "-device", "virtio-tablet-pci", "-device", "virtio-rng-pci", "-display", "none",
       "-qmp", f"unix:{QMP},server,nowait", "-serial", "mon:stdio"]
pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd); os._exit(1)
buf = b""
def pump(t=0.2):
    global buf
    r, _, _ = select.select([fd], [], [], t)
    if r:
        try: buf += os.read(fd, 65536)
        except OSError: pass
def waitfor(subs, t=60):
    s = time.time()
    while time.time() - s < t:
        pump()
        if any(x in buf[-1500:].decode("latin1") for x in subs): return True
    return False
def send(x): os.write(fd, x.encode()); time.sleep(0.2)
ok = waitfor(["Choose your language"], 120)
waitfor(["DHCP: IP"], 30)
if ok:
    send("3\r"); waitfor(["full name"], 25)
    send("Test User\r"); waitfor(["sername", "Login:"], 25)
    send("test\r"); waitfor(["Password:"], 25)
    send("pass1234\r"); waitfor(["epeat"], 25)
    send("pass1234\r")
for _ in range(80): pump(0.25)
qs = socket.socket(socket.AF_UNIX); qs.connect(QMP)
qf = qs.makefile("rw"); qf.readline()
def q(c):
    qf.write(json.dumps(c) + "\n"); qf.flush()
    while True:
        r = json.loads(qf.readline())
        if "return" in r or "error" in r: return r
q({"execute": "qmp_capabilities"})
q({"execute": "screendump", "arguments": {"filename": PPM}})
time.sleep(1)
Image.open(PPM).save(OUT, optimize=True)
q({"execute": "quit"})
os.waitpid(pid, 0)
print(f"surat: {OUT} ({'sozlaş tugadi' if ok else 'DIQQAT: til tanlaş ekrani çiqmadi'})")
