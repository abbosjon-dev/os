#!/usr/bin/env python3
"""Zamin OS — FAT32 disk obrazi (taşqi vositalarsiz, mtools/mkfs kerak emas).

    mkfat.py <obraz.img> <hajm_MiB> <diskdagi/yöl=mahalliy_fayl> ...
    masalan: mkfat.py esp.img 64 EFI/BOOT/BOOTAA64.EFI=target/BOOTAA64.EFI

Bölimlar jadvalisiz ("superfloppy") FAT32: UEFI uni töğridan-töğri taniydi va
olinadigan disk sifatida EFI/BOOT/BOOTAA64.EFI ni işga tuşiradi. Nomlar 8.3 (katta harf).
"""
import struct
import sys

SECTOR = 512
SPC = 1               # klasterdagi sektorlar
RESERVED = 32
NFATS = 2
EOC = 0x0FFFFFFF


def short_name(n):
    n = n.upper()
    base, _, ext = n.partition(".")
    assert 0 < len(base) <= 8 and len(ext) <= 3, f"8.3 nom emas: {n}"
    return (base.ljust(8) + ext.ljust(3)).encode()


def main():
    out, size_mib, *items = sys.argv[1:]
    total = int(size_mib) * 1024 * 1024 // SECTOR
    # FAT hajmi: klasterlar soni * 4 bayt
    clusters_guess = total // SPC
    fat_sectors = (clusters_guess * 4 + SECTOR - 1) // SECTOR
    data_start = RESERVED + NFATS * fat_sectors
    nclusters = (total - data_start) // SPC
    assert nclusters >= 65525, "FAT32 uçun disk juda kiçik (kamida ~33 MiB)"
    csize = SPC * SECTOR
    fat = [0x0FFFFFF8, 0xFFFFFFFF]
    data = {}  # klaster -> bayt

    def alloc_chain(content):
        n = max(1, (len(content) + csize - 1) // csize)
        first = len(fat)
        for i in range(n):
            fat.append(first + i + 1 if i + 1 < n else EOC)
            data[first + i] = content[i * csize:(i + 1) * csize]
        return first

    # Daraxt: {nom: dict | bytes}
    tree = {}
    for it in items:
        path, local = it.split("=", 1)
        parts = path.strip("/").split("/")
        d = tree
        for p in parts[:-1]:
            d = d.setdefault(p, {})
        d[parts[-1]] = open(local, "rb").read()

    def entry(name11, attr, cl, size):
        return struct.pack("<11sBBBHHHHHHHI", name11, attr, 0, 0, 0, 0x5a21, 0, cl >> 16, 0, 0x5a21, cl & 0xffff, size)

    def build_dir(d, self_cl, parent_cl):
        ents = b""
        if self_cl is not None:
            ents += entry(b".          ", 0x10, self_cl, 0) + entry(b"..         ", 0x10, parent_cl, 0)
        for name, v in sorted(d.items()):
            if isinstance(v, dict):
                # avval joy ajratamiz (öz klaster raqamini bilişi kerak)
                n_ent = 2 + len(v)
                nclu = max(1, (n_ent * 32 + csize - 1) // csize)
                first = len(fat)
                for i in range(nclu):
                    fat.append(first + i + 1 if i + 1 < nclu else EOC)
                content = build_dir(v, first, self_cl or 0)
                content += bytes(nclu * csize - len(content))
                for i in range(nclu):
                    data[first + i] = content[i * csize:(i + 1) * csize]
                ents += entry(short_name(name), 0x10, first, 0)
            else:
                cl = alloc_chain(v) if v else 0
                ents += entry(short_name(name), 0x20, cl, len(v))
        return ents

    # ildiz: 2-klaster
    root_n = max(1, (32 * (len(tree) + 1) + csize - 1) // csize)
    for i in range(root_n):
        fat.append(len(fat) + 1 if i + 1 < root_n else EOC)
    root = entry(b"ZAMIN-EFI  ", 0x08, 0, 0) + build_dir(tree, None, 0)
    root += bytes(root_n * csize - len(root))
    for i in range(root_n):
        data[2 + i] = root[i * csize:(i + 1) * csize]
    assert len(fat) - 2 <= nclusters, "fayllar diskka sigʻmaydi"

    bs = bytearray(SECTOR)
    bs[0:3] = b"\xEB\x58\x90"
    bs[3:11] = b"ZAMINOS "
    struct.pack_into("<HBHBHHBHHHII", bs, 11, SECTOR, SPC, RESERVED, NFATS, 0, 0, 0xF8, 0, 63, 255, 0, total)
    struct.pack_into("<IHHIHH", bs, 36, fat_sectors, 0, 0, 2, 1, 6)
    struct.pack_into("<BBBI11s8s", bs, 64, 0x80, 0, 0x29, 0x5a4d494e, b"ZAMIN-EFI  ", b"FAT32   ")
    bs[510:512] = b"\x55\xAA"
    fsinfo = bytearray(SECTOR)
    struct.pack_into("<I", fsinfo, 0, 0x41615252)
    struct.pack_into("<III", fsinfo, 484, 0x61417272, nclusters - (len(fat) - 2), len(fat))
    fsinfo[510:512] = b"\x55\xAA"

    fat_bytes = struct.pack(f"<{len(fat)}I", *fat)
    with open(out, "wb") as f:
        f.truncate(total * SECTOR)
        for off in (0, 6):  # asosiy va zaxira yuklaş sektori
            f.seek(off * SECTOR); f.write(bs); f.write(fsinfo)
        for k in range(NFATS):
            f.seek((RESERVED + k * fat_sectors) * SECTOR); f.write(fat_bytes)
        for cl, chunk in data.items():
            f.seek((data_start + (cl - 2) * SPC) * SECTOR); f.write(chunk)
    print(f"{out}: FAT32 {size_mib} MiB, {len(fat) - 2} klaster band")


if __name__ == "__main__":
    main()
