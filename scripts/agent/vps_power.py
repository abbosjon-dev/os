#!/usr/bin/env python3
# Zamin OS — muloyim öçiriş sinovi (Netcup SCP "Shutdown (ACPI)", `virsh shutdown`):
#   vps_power.py KERNEL LOGDIR
# Ikki yöl:
#   acpi — UEFI (BOOTAA64.EFI) + ACPI + GICv3: quvvat tugmasi DSDT dagi Generic Event Device
#          (ACPI0013) orqali; jurnalda 'Quvvat tugmasi: GED @0x…, GSIV N'.
#   dtb  — -kernel + DTB + GICv3: PL061 GPIO dagi gpio-keys (KEY_POWER, 3-oyoq).
# Har bir yöl: 1-yuklaniş — sozlaş ustasi (seriya port, English), faylga yoziş (yoz ...), so'ng
# QMP `system_powerdown`; jurnalda 'Quvvat tugmasi bosildi' va 'disk sinxronlandi', QEMU
# 15 soniya içida özi çiqadi (çiqiş kodi 0). 2-yuklaniş şu disk bilan: sozlamalar diskdan
# öqiladi, parol bilan kiriş → ZaminFS tiklanadi va yozilgan fayl butun.
# Çiqiş: 0 — ötdi ("OK ..."), 1 — xato ("FAIL ..."). UEFI firmware yöq bölsa acpi yöli SKIP.
import json, os, re, select, socket, subprocess, sys, tempfile, time

kernel, logdir = sys.argv[1:3]
here = os.path.dirname(os.path.abspath(__file__))
fw = next((f for f in ("/usr/share/qemu-efi-aarch64/QEMU_EFI.fd", "/usr/share/AAVMF/AAVMF_CODE.fd") if os.path.exists(f)), None)
MARK = "Zamin-quvvat-42"


class Vm:
    def __init__(self, path, tmp, sysimg, log):
        self.qmp = os.path.join(tmp, "q.sock")
        if os.path.exists(self.qmp):
            os.unlink(self.qmp)
        cmd = ["qemu-system-aarch64", "-machine", "virt,gic-version=3", "-cpu", "max", "-smp", "2", "-m", "1G"]
        if path == "acpi":
            esp = os.path.join(tmp, "esp.img")
            cmd += ["-bios", fw, "-drive", f"file={esp},if=none,format=raw,id=esp",
                    "-device", "virtio-blk-pci,drive=esp,bootindex=0"]
        else:
            cmd += ["-kernel", kernel]
        cmd += ["-drive", f"file={sysimg},if=none,format=raw,id=s", "-device", "virtio-blk-pci,drive=s",
                "-device", "virtio-rng-pci", "-nic", "user,model=virtio-net-pci",
                "-display", "none",
                "-qmp", f"unix:{self.qmp},server,nowait", "-nographic", "-monitor", "none", "-serial", "stdio"]
        self.logf = open(log, "wb")
        self.p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.buf = b""

    def pump(self, t=0.2):
        r, _, _ = select.select([self.p.stdout], [], [], t)
        if r:
            d = os.read(self.p.stdout.fileno(), 65536)
            self.buf += d; self.logf.write(d); self.logf.flush()
            return bool(d)
        return False

    def text(self):
        return self.buf.decode("utf8", "replace")

    def bad(self):
        m = re.search(r"(?i)panic|yiqildi", self.text())
        return m and self.text()[m.start():m.start() + 160].replace("\r", " ").replace("\n", " ")

    def wait(self, pat, t, since=0):
        t0 = time.time()
        while time.time() - t0 < t:
            self.pump()
            if self.bad() or self.p.poll() is not None:
                return None
            m = re.search(pat, self.text()[since:])
            if m:
                return m
        return None

    def send(self, s):
        for ch in s.encode():
            self.p.stdin.write(bytes([ch])); self.p.stdin.flush()
            time.sleep(0.02)

    def qmp_cmd(self, c):
        s = socket.socket(socket.AF_UNIX)
        s.connect(self.qmp)
        f = s.makefile("rw")
        f.readline()
        f.write(json.dumps({"execute": "qmp_capabilities"}) + "\n"); f.flush(); f.readline()
        f.write(json.dumps({"execute": c}) + "\n"); f.flush()
        s.settimeout(5)
        while True:
            r = json.loads(f.readline())
            if "return" in r or "error" in r:
                s.close()
                return r

    def kill(self):
        try:
            self.p.kill(); self.p.wait()
        except Exception:
            pass
        self.logf.close()


def run(path):
    tmp = tempfile.mkdtemp(prefix=f"zamin-pwr-{path}-")
    sysimg = os.path.join(tmp, "sys.img")
    subprocess.run(["truncate", "-s", "128M", sysimg], check=True)
    if path == "acpi":
        subprocess.run(["sh", os.path.join(here, "..", "efi", "mkesp.sh"), kernel, os.path.join(tmp, "esp.img")],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    tag = "UEFI + ACPI GED" if path == "acpi" else "-kernel + DTB PL061"
    log1, log2 = (os.path.join(logdir, f"verify-power-{path}-{k}.log") for k in (1, 2))
    vm = Vm(path, tmp, sysimg, log1)
    try:
        if not vm.wait(r"Choose your language", 120):
            return f"FAIL power ({tag}): til tanlaş ekrani çiqmadi ({vm.bad() or 'timeout'}; qarang {log1})"
        if path == "acpi":
            want = r"Quvvat tugmasi: GED @0x[0-9a-f]+, GSIV \d+ \(DSDT ACPI0013\)"
        else:
            want = r"Quvvat tugmasi: PL061 GPIO @0x[0-9a-f]+, oyoq 3 \(gpio-keys KEY_POWER\), GSIV \d+"
        m = re.search(want, vm.text())
        if not m:
            return f"FAIL power ({tag}): jurnalda '{want}' yöq"
        src = m.group(0)
        steps = [("3\r", r"full name"), ("Test User\r", r"sername|Login:"), ("test\r", r"Password:"),
                 ("pass1234\r", r"epeat"), ("pass1234\r", r"Settings saved to disk")]
        for s, w in steps:
            vm.send(s)
            if not vm.wait(w, 30):
                return f"FAIL power ({tag}): sozlaş ustasi ('{w}' kutildi; qarang {log1})"
        time.sleep(1.5)
        at = len(vm.text())
        vm.send(f"write ~/quvvat.txt {MARK}\r")
        vm.wait(r"\n", 5, at)
        time.sleep(0.3)
        t0 = time.time()
        vm.qmp_cmd("system_powerdown")
        while vm.p.poll() is None and time.time() - t0 < 15:
            vm.pump(0.1)
        while vm.pump(0.05):
            pass
        dt = time.time() - t0
        if vm.p.poll() is None:
            return f"FAIL power ({tag}): QEMU 15 s içida çiqmadi (qarang {log1})"
        rc = vm.p.returncode
        if rc != 0:
            return f"FAIL power ({tag}): QEMU çiqiş kodi {rc}"
        if bad := vm.bad():
            return f"FAIL power ({tag}): {bad}"
        if "Quvvat tugmasi bosildi" not in vm.text():
            return f"FAIL power ({tag}): jurnalda 'Quvvat tugmasi bosildi' yöq"
        m = re.search(r"Quvvat: disk sinxronlandi \(ZaminFS: (\d+) blok, sozlamalar: saqlandi", vm.text())
        if not m:
            return f"FAIL power ({tag}): jurnalda 'disk sinxronlandi' (ZaminFS + sozlamalar) yöq"
        blocks = m.group(1)
    finally:
        vm.kill()
    # 2-yuklaniş: şu disk
    vm = Vm(path, tmp, sysimg, log2)
    try:
        if not vm.wait(r"\[conf\] settings loaded from disk \(account: test\)", 120):
            return f"FAIL power ({tag}, 2-yuklaniş): sozlamalar diskdan öqilmadi ({vm.bad() or 'timeout'}; qarang {log2})"
        if not vm.wait(r"(?i)password:", 60):
            return f"FAIL power ({tag}, 2-yuklaniş): parol söralmadi"
        vm.send("pass1234\r")
        if not vm.wait(r"Files restored from disk: \d+ entries \(ZaminFS", 30):
            return f"FAIL power ({tag}, 2-yuklaniş): ZaminFS tiklanmadi (qarang {log2})"
        time.sleep(1.0)
        at = len(vm.text())
        vm.send("cat ~/quvvat.txt\r")
        if not vm.wait(re.escape(MARK) + r"\s*\r?\n", 15, at + len("cat ~/quvvat.txt") + 1):
            return f"FAIL power ({tag}, 2-yuklaniş): öçirişdan oldin yozilgan fayl diskda yöq (qarang {log2})"
    finally:
        vm.kill()
        subprocess.run(["rm", "-rf", tmp])
    return f"OK power ({tag}): {src}; system_powerdown → bosildi → disk sinxronlandi ({blocks} blok) → QEMU {dt:.1f} s da çiqdi (kod 0) → qayta yuklanişda sozlamalar, ZaminFS va fayl butun"


res = []
for path in ("dtb", "acpi"):
    if path == "acpi" and not fw:
        print("SKIP power (acpi): UEFI firmware yöq")
        continue
    r = run(path)
    print(r)
    if not r.startswith("OK"):
        sys.exit(1)
sys.exit(0)
