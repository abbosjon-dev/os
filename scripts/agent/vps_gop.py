#!/usr/bin/env python3
# Zamin OS — VPS GOP zaxira ekran sinovi: UEFI + ACPI + GICv3, virtio-gpu YÖQ, -device ramfb
# (EDK2 GOP chiziqli framebufferini beradi). Kiriş — qemu-xhci + usb-kbd + usb-tablet (Netcup VNC).
#   vps_gop.py KERNEL LOG PNG
# Tekşiruvlar: jurnalda 'GOP: WxH, format …' va '[gpu ] UEFI GOP: WxH', virtio-gpu yöq; QMP
# screendump ölçami GOP ga teng va bir rangli emas (piksellar xilma-xilligi); USB klaviaturadan
# '3' + Enter sozlaş ekranini özgartiradi (GUI GOP ekranida işlaydi).
# Çiqiş: 0 — ötdi (stdout: "OK ..."), 1 — xato ("FAIL ..."), 2 — UEFI firmware yöq.
import json, os, re, select, socket, subprocess, sys, tempfile, time

kernel, log_path, png = sys.argv[1:4]
fw = next((f for f in ("/usr/share/qemu-efi-aarch64/QEMU_EFI.fd", "/usr/share/AAVMF/AAVMF_CODE.fd") if os.path.exists(f)), None)
if not fw:
    print("SKIP vps-gop: UEFI firmware yöq"); sys.exit(2)
tmp = tempfile.mkdtemp(prefix="zamin-gop-")
esp, sysimg, qmp, ppm = (os.path.join(tmp, n) for n in ("esp.img", "sys.img", "q.sock", "s.ppm"))
here = os.path.dirname(os.path.abspath(__file__))
subprocess.run(["sh", os.path.join(here, "..", "efi", "mkesp.sh"), kernel, esp], check=True,
               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
subprocess.run(["truncate", "-s", "128M", sysimg], check=True)
cmd = ["qemu-system-aarch64", "-machine", "virt,gic-version=3", "-cpu", "max", "-smp", "4", "-m", "1G", "-bios", fw,
       "-drive", f"file={esp},if=none,format=raw,id=esp", "-device", "virtio-blk-pci,drive=esp,bootindex=0",
       "-drive", f"file={sysimg},if=none,format=raw,id=s", "-device", "virtio-blk-pci,drive=s",
       "-device", "ramfb", "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0",
       "-device", "usb-tablet,bus=xhci.0", "-display", "none",
       "-device", "virtio-rng-pci", "-nic", "user,model=virtio-net-pci",
       "-qmp", f"unix:{qmp},server,nowait", "-nographic", "-monitor", "none", "-serial", "stdio"]
logf = open(log_path, "wb")
p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
buf = b""


def pump(t=0.2):
    global buf
    r, _, _ = select.select([p.stdout], [], [], t)
    if r:
        d = os.read(p.stdout.fileno(), 65536)
        buf += d; logf.write(d); logf.flush()


def text():
    return buf.decode("utf8", "replace")


def waitfor(s, t):
    t0 = time.time()
    while time.time() - t0 < t:
        pump()
        if any(x in text() for x in ("panic", "yiqildi")) or p.poll() is not None:
            return False
        if s in text():
            return True
    return False


def done(res):
    try:
        p.kill(); p.wait()
    except Exception:
        pass
    logf.close()
    for f in (esp, sysimg, qmp, ppm):
        try: os.remove(f)
        except OSError: pass
    try: os.rmdir(tmp)
    except OSError: pass
    print(res)
    sys.exit(0 if res.startswith("OK") else 1)


if not waitfor("Choose your language", 150):
    bad = [l for l in text().splitlines() if "panic" in l or "yiqildi" in l][:2]
    done(f"FAIL vps-gop: til tanlaş ekrani çiqmadi ({' '.join(bad)[:200]}; qarang {log_path})")
g = re.search(r"GOP: (\d+)x(\d+), format (RGBX|BGRX)", text())
if not g:
    done(f"FAIL vps-gop: jurnalda 'GOP: WxH, format …' yöq (qarang {log_path})")
w, h = int(g.group(1)), int(g.group(2))
if f"[gpu ] UEFI GOP: {w}x{h}" not in text():
    done("FAIL vps-gop: grafik ekran GOP framebufferida işga tuşmadi ('[gpu ] UEFI GOP' yöq)")
if "virtio-gpu" in text():
    done("FAIL vps-gop: virtio-gpu topildi — sinov GOP yölini tekşirmayapti")
if not re.search(r"ACPI MADT: GICv3", text()):
    done("FAIL vps-gop: ACPI + GICv3 yöli işlatilmadi")
if "[ok] USB HID" not in text():
    done("FAIL vps-gop: '[ok] USB HID' self-test yöq")
time.sleep(2.5)  # GUI til tanlaş ekrani çizilsin
qs = socket.socket(socket.AF_UNIX)
for _ in range(50):
    try: qs.connect(qmp); break
    except OSError: time.sleep(0.2)
qf = qs.makefile("rw"); qf.readline()


def q(c, a=None):
    d = {"execute": c}
    if a is not None: d["arguments"] = a
    qf.write(json.dumps(d) + "\n"); qf.flush()
    while True:
        r = json.loads(qf.readline())
        if "return" in r or "error" in r:
            return r


q("qmp_capabilities")
r = q("screendump", {"filename": ppm})
if "error" in r:
    done(f"FAIL vps-gop: QMP screendump: {r['error']}")
time.sleep(1)
try:
    from PIL import Image
    im = Image.open(ppm).convert("RGB")
    im.save(png)
except Exception as e:
    done(f"FAIL vps-gop: surat olinmadi ({e})")
if im.size != (w, h):
    done(f"FAIL vps-gop: surat ölçami {im.size[0]}x{im.size[1]}, GOP {w}x{h}")
cols = im.getcolors(1 << 24)
ncol = len(cols)
top = max(c for c, _ in cols) / (w * h)
if ncol < 64 or top > 0.9:
    done(f"FAIL vps-gop: ekran bir rangli ({ncol} rang, eng köp rang {top:.0%}) — GOP ga çizilmadi")
for k in ("3", "ret"):
    r = q("send-key", {"keys": [{"type": "qcode", "data": k}]})
    if "error" in r: done(f"FAIL vps-gop: QMP send-key: {r['error']}")
    time.sleep(0.3)
if not waitfor("First-time setup", 15):
    done("FAIL vps-gop: USB klaviaturadan '3' + Enter sozlaş ekranini özgartirmadi")
oks = len(re.findall(r"\[ok\]|— OK", text()))
done(f"OK vps-gop (GICv3, ACPI, ramfb, virtio-gpu yöq): GOP {w}x{h} {g.group(3)} → til tanlaş ekrani ({ncol} rang), "
     f"USB '3'+Enter qabul qilindi, {oks} self-test, surat {png}")
