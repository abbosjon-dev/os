#!/usr/bin/env python3
"""Seriya qobiq sinovi: terminal fayl buyruqlari (nusxa, köçir, qidir, bosh, oxir, hajm).

   seriya_qobiq.py KERNEL LOG

Toza disk bilan QEMU (grafikasiz) işga tuşiriladi, seriya port orqali sozlaş ustasi
(English) ötiladi, terminalda `til uz` → fayl buyruqlari yuboriladi va natijalar tekşiriladi.
Çiqiş kodi 0 — ötdi; natija bir qator matn (OK/FAIL) stdout ga.
"""
import os
import re
import subprocess
import sys
import tempfile
import time

kernel, log_path = sys.argv[1], sys.argv[2]
disk = tempfile.NamedTemporaryFile(prefix="zamin-qobiq-", suffix=".img", delete=False).name
os.truncate(disk, 128 << 20)
log = open(log_path, "wb")
q = subprocess.Popen(
    ["qemu-system-aarch64", "-machine", "virt", "-cpu", "max", "-m", "512M", "-smp", "2", "-nographic",
     "-drive", f"file={disk},if=none,format=raw,id=d", "-device", "virtio-blk-device,drive=d",
     "-device", "virtio-rng-device", "-kernel", kernel],
    stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)


def text():
    with open(log_path, "rb") as f:
        return f.read().decode("utf-8", "replace")


def fail(msg):
    q.kill()
    q.wait()
    os.unlink(disk)
    print(f"FAIL seriya qobiq: {msg} (qarang {log_path})")
    sys.exit(1)


def wait(pattern, timeout=60, since=0):
    end = time.time() + timeout
    while time.time() < end:
        t = text()
        if re.search(r"panic|yiqildi", t, re.I):
            fail("panic: " + re.search(r".*(panic|yiqildi).*", t, re.I).group(0)[:200])
        if re.search(pattern, t[since:]):
            return
        if q.poll() is not None:
            fail("qemu çiqib ketdi")
        time.sleep(0.3)
    fail(f"kutilgan matn çiqmadi: {pattern!r}")


def send(s):
    # Seriya portga belgima-belgi (FIFO tölib ketmasin)
    for ch in s.encode():
        q.stdin.write(bytes([ch]))
        q.stdin.flush()
        time.sleep(0.004)


PROMPT = r"@zamin:[^\n]*> "


def cmd(line, timeout=30):
    """Buyruqni yuboradi va keyingi taklifni kutadi. Natija: buyruq çiqişi (qatorlar)."""
    start = len(text())
    send(line + "\r")
    wait(PROMPT, timeout, since=start + len(line))
    out = text()[start:]
    lines = [l.rstrip("\r") for l in out.replace("\r\n", "\n").split("\n")]
    return lines[1:-1]  # 1-qator — kiritilgan buyruq aksi, oxirgisi — yangi taklif


wait(r"Choose your language", 120)
send("3\r"); wait(r"full name")
send("Test User\r"); wait(r"sername|Login:")
send("test\r"); wait(r"Password:")
send("pass1234\r"); wait(r"epeat")
send("pass1234\r")
wait(PROMPT, 60)

cmd("til uz")
a = "Salom zamin OS\nikkinçi zamin qatori\nuçinçi qator\n"
cmd("yoz /tmp/a.txt Salom zamin OS")
cmd("aks ikkinçi zamin qatori >> /tmp/a.txt")
cmd("aks uçinçi qator >> /tmp/a.txt")
checks = []


def check(name, got, want):
    if got != want:
        fail(f"{name}: kutilgan {want!r}, olindi {got!r}")
    checks.append(name)


out = cmd("nusxa /tmp/a.txt /tmp/b.txt")
check("nusxa", out, [f"  nusxalandi: /tmp/a.txt → /tmp/b.txt ({len(a.encode())} B)"])
check("öqi (nusxa)", cmd("öqi /tmp/b.txt"), a.splitlines())
check("qidir -n", cmd("qidir -n zamin /tmp/b.txt"), ["1:Salom zamin OS", "2:ikkinçi zamin qatori"])
check("qidir -i", cmd("qidir -i SALOM /tmp/b.txt"), ["Salom zamin OS"])
check("bosh -n 1", cmd("bosh -n 1 /tmp/b.txt"), ["Salom zamin OS"])
check("oxir -n 1", cmd("oxir -n 1 /tmp/b.txt"), ["uçinçi qator"])
check("oxir -2", cmd("oxir -2 /tmp/b.txt"), ["ikkinçi zamin qatori", "uçinçi qator"])
words = len(a.split())
check("hajm", cmd("hajm /tmp/b.txt"), [f"  3 qator, {words} söz, {len(a.encode())} bayt  /tmp/b.txt"])
check("köçir", cmd("köçir /tmp/b.txt /tmp/c.txt"), ["  köçirildi: /tmp/b.txt → /tmp/c.txt"])
gone = cmd("öqi /tmp/b.txt")
if len(gone) != 1 or not gone[0].startswith("  /tmp/b.txt: "):
    fail(f"köçir: manba fayl qoldi ({gone!r})")
checks.append("köçir: manba yöq")
check("öqi (köçirilgan)", cmd("öqi /tmp/c.txt"), a.splitlines())
check("kochir (alias) + papkaga", cmd("kochir /tmp/c.txt ~"), ["  köçirildi: /tmp/c.txt → ~/c.txt"])
# ruxsatlar: tizim papkasiga yoziş va köçiriş taqiqlangan
den = cmd("nusxa /tmp/a.txt /tizim/x.txt") + cmd("köçir /tizim/versiya /tmp/v")
if len(den) != 2 or any("ruxsat" not in l.lower() for l in den):
    fail(f"ruxsatlar hurmat qilinmadi: {den!r}")
checks.append("ruxsatlar")
use = cmd("qidir zamin")
if len(use) != 1 or "qidir [-i] [-n]" not in use[0]:
    fail(f"qidir: foydalaniş matni çiqmadi ({use!r})")
checks.append("foydalaniş matni")

q.kill()
q.wait()
os.unlink(disk)
print(f"OK seriya qobiq: {len(checks)} tekşiruv (nusxa, qidir -n/-i, bosh/oxir -n, hajm, köçir, ruxsatlar)")
