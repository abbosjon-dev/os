#!/usr/bin/env python3
"""Zamin OS — GPT disk obrazi (taşqi vositalarsiz: sgdisk/parted kerak emas).

    gpt.py <obraz.img> <hajm_MiB> <esp.img>

Natija (sektor 512 bayt):
  LBA 0         himoya MBR (tur 0xEE)
  LBA 1..33     asosiy GPT sarlavhasi + 128 ta bölim yozuvi
  1-bölim       ESP (EFI tizim bölimi, FAT32) — <esp.img> mazmuni, 1 MiB dan boşlab
  2-bölim       Zamin maʼlumot bölimi (tur 7A616D69-6E00-4F53-9A4D-494E44534B31) —
                ESP dan keyin diskning oxirigaça; böş (nollar) — Zamin OS birinçi
                yuklanişda sozlaş ustasini oçadi va ZaminFS ni şu bölimda yaratadi
  oxirgi 33     zaxira GPT
Fayl siyrak (sparse) yasaladi: 2 GB obraz diskda faqat ESP hajmiça joy oladi.
"""
import os
import struct
import sys
import uuid
import zlib

SECTOR = 512
ESP_TYPE = uuid.UUID("C12A7328-F81F-11D2-BA4B-00A0C93EC93B")
ZAMIN_TYPE = uuid.UUID("7A616D69-6E00-4F53-9A4D-494E44534B31")
N_ENTRIES = 128
ENTRY_SIZE = 128
ENTRY_SECTORS = N_ENTRIES * ENTRY_SIZE // SECTOR  # 32
ALIGN = 2048  # 1 MiB


def entry(ty, first, last, name):
    return struct.pack("<16s16sQQQ72s", ty.bytes_le, uuid.uuid4().bytes_le, first, last, 0,
                       name.encode("utf-16-le").ljust(72, b"\0"))


def header(current, backup, first_usable, last_usable, disk_guid, entries_lba, entries_crc):
    def pack(crc):
        return struct.pack("<8sIIIIQQQQ16sQIII", b"EFI PART", 0x00010000, 92, crc, 0, current, backup,
                           first_usable, last_usable, disk_guid, entries_lba, N_ENTRIES, ENTRY_SIZE,
                           entries_crc)
    h = pack(zlib.crc32(pack(0)) & 0xffffffff)
    return h.ljust(SECTOR, b"\0")


def main():
    out, size_mib, esp_path = sys.argv[1:4]
    total = int(size_mib) * 1024 * 1024 // SECTOR
    esp = open(esp_path, "rb").read()
    esp_sectors = (len(esp) + SECTOR - 1) // SECTOR
    first_usable = 2 + ENTRY_SECTORS
    last_usable = total - 1 - ENTRY_SECTORS
    esp_first = ALIGN
    esp_last = esp_first + esp_sectors - 1
    z_first = (esp_last + 1 + ALIGN - 1) // ALIGN * ALIGN
    z_last = last_usable
    assert z_last - z_first > 64 * 1024 * 1024 // SECTOR, "obraz juda kiçik (Zamin bölimi < 64 MiB)"

    entries = entry(ESP_TYPE, esp_first, esp_last, "EFI system partition")
    entries += entry(ZAMIN_TYPE, z_first, z_last, "Zamin OS")
    entries = entries.ljust(N_ENTRIES * ENTRY_SIZE, b"\0")
    ecrc = zlib.crc32(entries) & 0xffffffff
    disk_guid = uuid.uuid4().bytes_le

    mbr = bytearray(SECTOR)
    # bitta bölim: tur 0xEE, LBA 1 dan diskning oxirigaça (32 bitga sinmasa — 0xFFFFFFFF)
    mbr[446:462] = struct.pack("<BBBBBBBBII", 0, 0, 2, 0, 0xEE, 0xFF, 0xFF, 0xFF, 1,
                               min(total - 1, 0xFFFFFFFF))
    mbr[510:512] = b"\x55\xaa"

    with open(out, "wb") as f:
        f.truncate(total * SECTOR)
        f.seek(0); f.write(mbr)
        f.write(header(1, total - 1, first_usable, last_usable, disk_guid, 2, ecrc))
        f.write(entries)
        f.seek(esp_first * SECTOR); f.write(esp)
        # Zamin bölimining boşi aniq nollar bölsin (dd bilan eski disk ustiga yozilganda ham)
        f.seek(z_first * SECTOR); f.write(bytes(1024 * 1024))
        f.seek((total - 1 - ENTRY_SECTORS) * SECTOR); f.write(entries)
        f.write(header(total - 1, 1, first_usable, last_usable, disk_guid, total - 1 - ENTRY_SECTORS, ecrc))
    print(f"GPT: {out} — {int(size_mib)} MiB; ESP LBA {esp_first}..{esp_last}, "
          f"Zamin LBA {z_first}..{z_last} ({(z_last - z_first + 1) * SECTOR >> 20} MiB)")


if __name__ == "__main__":
    main()
