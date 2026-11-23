#!/usr/bin/env python3
# Zamin OS — VPS USB kiriş sinovi (Netcup VNC ga öxşaş): UEFI + ACPI + GICv3, virtio-*-pci,
# lekin klaviatura/planşet virtio-input ÖRNIGA qemu-xhci + usb-kbd + usb-tablet.
#   vps_usb.py KERNEL LOG PNG
# Til tanlaş ekraniga QMP send-key ("3", Enter) va input-send-event (abs X/Y + çap tugma)
# yuboriladi. Seriya portga hiç narsa yozilmaydi — sozlaş faqat USB klaviaturadan özgarişi kerak.
# Çiqiş: 0 — ötdi (stdout: "OK ..."), 1 — xato ("FAIL ..."), 2 — UEFI firmware yöq.
import json, os, re, select, socket, subprocess, sys, tempfile, time

kernel, log_path, png = sys.argv[1:4]
fw = next((f for f in ("/usr/share/qemu-efi-aarch64/QEMU_EFI.fd", "/usr/share/AAVMF/AAVMF_CODE.fd") if os.path.exists(f)), None)
if not fw:
    print("SKIP vps-usb: UEFI firmware yöq"); sys.exit(2)
tmp = tempfile.mkdtemp(prefix="zamin-usb-")
esp, sysimg, qmp, ppm = (os.path.join(tmp, n) for n in ("esp.img", "sys.img", "q.sock", "s.ppm"))
here = os.path.dirname(os.path.abspath(__file__))
subprocess.run(["sh", os.path.join(here, "..", "efi", "mkesp.sh"), kernel, esp], check=True,
               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
subprocess.run(["truncate", "-s", "128M", sysimg], check=True)
cmd = ["qemu-system-aarch64", "-machine", "virt,gic-version=3", "-cpu", "max", "-smp", "4", "-m", "1G", "-bios", fw,
       "-drive", f"file={esp},if=none,format=raw,id=esp", "-device", "virtio-blk-pci,drive=esp,bootindex=0",
       "-drive", f"file={sysimg},if=none,format=raw,id=s", "-device", "virtio-blk-pci,drive=s",
       "-device", "virtio-gpu-pci", "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0",
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
    done(f"FAIL vps-usb: til tanlaş ekrani çiqmadi ({' '.join(bad)[:200]}; qarang {log_path})")
m = re.search(r"xHCI: (\d+) port, usb-kbd, usb-tablet", text())
if not m:
    done(f"FAIL vps-usb: jurnalda 'xHCI: N port, usb-kbd, usb-tablet' yöq (qarang {log_path})")
if "[ok] USB HID" not in text():
    done("FAIL vps-usb: '[ok] USB HID' self-test yöq")
if "kiriş: 0 ta virtio-input" not in text():
    done("FAIL vps-usb: virtio-input topildi — sinov USB yölini tekşirmayapti")
if "First-time setup" in text():
    done("FAIL vps-usb: sozlaş tugma yuborilmasdan özgardi")
time.sleep(2)  # GUI kutiş ekrani çizilsin
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
for k in ("3", "ret"):
    r = q("send-key", {"keys": [{"type": "qcode", "data": k}]})
    if "error" in r: done(f"FAIL vps-usb: QMP send-key: {r['error']}")
    time.sleep(0.3)
if not waitfor("First-time setup", 15):
    done("FAIL vps-usb: USB klaviaturadan '3' + Enter sozlaş ekranini özgartirmadi")
# siçqonça: absolyut (0..32767) → ekranning 1/2, 1/4 nuqtasi, çap tugma
q("input-send-event", {"events": [{"type": "abs", "data": {"axis": "x", "value": 16384}},
                                  {"type": "abs", "data": {"axis": "y", "value": 8192}}]})
time.sleep(0.3)
q("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]}); time.sleep(0.2)
q("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})
if not waitfor("siçqonça bosişi qabul qilindi", 10):
    done("FAIL vps-usb: USB planşet bosişi qabul qilinmadi")
mc = re.search(r"bosişi qabul qilindi \((\d+), (\d+)\)", text())
gm = re.search(r"virtio-gpu: (\d+)x(\d+)", text())
if mc and gm:
    x, y, w, h = map(int, (*mc.groups(), *gm.groups()))
    if abs(x - w // 2) > 2 or abs(y - h // 4) > 2:
        done(f"FAIL vps-usb: planşet koordinatasi notoʻgʻri ({x}, {y}), kutilgan ({w // 2}, {h // 4})")
time.sleep(1.5)
q("screendump", {"filename": ppm})
time.sleep(1)
try:
    from PIL import Image
    Image.open(ppm).save(png)
except Exception as e:
    done(f"FAIL vps-usb: surat olinmadi ({e})")
done(f"OK vps-usb (GICv3, ACPI): {m.group(0)} → send-key '3'+Enter sozlaşni özgartirdi, planşet bosişi ({mc.group(1)}, {mc.group(2)}) qabul qilindi, surat {png}")
