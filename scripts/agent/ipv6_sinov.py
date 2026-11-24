#!/usr/bin/env python3
"""IPv6 sinovi: QEMU `-netdev user,ipv4=on,ipv6=on` (faqat ipv6=on — IPv4 öçadi) (slirp: RA prefiksi fec0::/64, router fe80::2, xost fec0::2).

   ipv6_sinov.py KERNEL LOG

Tekşiriladi: self-test `[ok] IPv6:`, jurnalda link-local (EUI-64) va SLAAC manzili `fec0::…`
(MAC dan), seriya qobiqda `ping fec0::2` javob oladi (NDP + ICMPv6 echo), eski IPv4
`ping 10.0.2.2` ham ötadi. Çiqiş kodi 0 — ötdi; natija bir qator matn (OK/FAIL) stdout ga.
"""
import os
import re
import subprocess
import sys
import tempfile
import time

kernel, log_path = sys.argv[1], sys.argv[2]
MAC = "52:54:00:12:34:56"
disk = tempfile.NamedTemporaryFile(prefix="zamin-ipv6-", suffix=".img", delete=False).name
os.truncate(disk, 128 << 20)
log = open(log_path, "wb")
q = subprocess.Popen(
    ["qemu-system-aarch64", "-machine", "virt", "-cpu", "max", "-m", "512M", "-smp", "2", "-nographic",
     "-drive", f"file={disk},if=none,format=raw,id=d", "-device", "virtio-blk-device,drive=d",
     "-device", "virtio-rng-device",
     "-netdev", "user,id=n,ipv4=on,ipv6=on", "-device", f"virtio-net-device,netdev=n,mac={MAC}",
     "-kernel", kernel],
    stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)


def text():
    with open(log_path, "rb") as f:
        return f.read().decode("utf-8", "replace")


def fail(msg):
    q.kill()
    q.wait()
    os.unlink(disk)
    print(f"FAIL ipv6: {msg} (qarang {log_path})")
    sys.exit(1)


def wait(pattern, timeout=60, since=0):
    end = time.time() + timeout
    while time.time() < end:
        t = text()
        if re.search(r"panic|yiqildi", t, re.I):
            fail("panic: " + re.search(r".*(panic|yiqildi).*", t, re.I).group(0)[:200])
        m = re.search(pattern, t[since:])
        if m:
            return m
        if q.poll() is not None:
            fail("qemu çiqib ketdi")
        time.sleep(0.3)
    fail(f"kutilgan matn çiqmadi: {pattern!r}")


def send(s):
    for ch in s.encode():
        q.stdin.write(bytes([ch]))
        q.stdin.flush()
        time.sleep(0.004)


PROMPT = r"@zamin:[^\n]*> "


def cmd(line, timeout=30):
    start = len(text())
    send(line + "\r")
    wait(PROMPT, timeout, since=start + len(line))
    out = text()[start:]
    return [l.rstrip("\r") for l in out.replace("\r\n", "\n").split("\n")][1:-1]


wait(r"\[ok\] IPv6: ", 120)
ll = wait(r"IPv6: link-local (fe80::[0-9a-f:]+), solicited-node (ff02::1:ff[0-9a-f:]+)", 60)
if ll.group(1) != "fe80::5054:ff:fe12:3456" or ll.group(2) != "ff02::1:ff12:3456":
    fail(f"link-local/solicited-node notöğri: {ll.group(1)}, {ll.group(2)}")
sl = wait(r"IPv6: SLAAC (fec0::[0-9a-f:]+)/64, şlyuz (fe80::[0-9a-f:]+) \(RA\)", 60)
if sl.group(1) != "fec0::5054:ff:fe12:3456":
    fail(f"SLAAC manzili EUI-64 emas: {sl.group(1)}")
wait(r"Choose your language", 120)
send("3\r"); wait(r"full name")
send("Test User\r"); wait(r"sername|Login:")
send("test\r"); wait(r"Password:")
send("pass1234\r"); wait(r"epeat")
send("pass1234\r")
wait(PROMPT, 60)

out6 = cmd("ping fec0::2", 40)
got6 = next((l for l in out6 if re.search(r"\d/4 ", l)), "")
n6 = int(got6.strip()[0]) if got6 else 0
if n6 < 3:
    fail(f"ping fec0::2: {n6}/4 javob ({out6!r})")
outll = cmd(f"ping {sl.group(2)}", 40)
gotll = next((l for l in outll if re.search(r"\d/4 ", l)), "")
nll = int(gotll.strip()[0]) if gotll else 0
if nll < 3:
    fail(f"ping {sl.group(2)} (link-local şlyuz): {nll}/4 javob ({outll!r})")
bad = cmd("ping fec0::zz")
if not any("notöğri" in l or "invalid" in l for l in bad):
    fail(f"buzuq IPv6 manzil rad etilmadi ({bad!r})")
out4 = cmd("ping 10.0.2.2", 40)
got4 = next((l for l in out4 if re.search(r"\d/4 ", l)), "")
n4 = int(got4.strip()[0]) if got4 else 0
if n4 < 3:
    fail(f"IPv4 ping 10.0.2.2: {n4}/4 javob ({out4!r})")

q.kill()
q.wait()
os.unlink(disk)
print(f"OK ipv6: link-local {ll.group(1)}, SLAAC {sl.group(1)}/64 (RA, şlyuz {sl.group(2)}), "
      f"ping fec0::2 {n6}/4, ping {sl.group(2)} {nll}/4, IPv4 ping 10.0.2.2 {n4}/4")
