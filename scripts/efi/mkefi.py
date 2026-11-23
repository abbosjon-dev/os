#!/usr/bin/env python3
"""Zamin OS — ELF yadrodan UEFI ilovasi (PE32+/COFF, BOOTAA64.EFI) yasaydi.

    mkefi.py <kernel.elf> <BOOTAA64.EFI>

Faqat Python standart kutubxonasi. Tasvir:
  ImageBase = __kernel_start - 64 KiB (PE sarlavhasi şu yerda), bitta ".text" bölimi
  RVA 0x10000 dan (= __kernel_start) __kernel_end gaça: kod, rodata, data; qolgani
  (.bss, steklar, bootinfo, DTB joyi) firmware tomonidan nol bilan töldiriladi.
  Relokatsiyalar yöq (IMAGE_FILE_RELOCS_STRIPPED) — firmware tasvirni aynan
  ImageBase ga yuklaydi, şuning uçun yadro bağlangan manzilida işlaydi.
  Kiriş nuqtasi — `efi_entry` (arch/boot.s).
"""
import struct
import sys

HDR = 0x10000          # sarlavha uçun joy (SectionAlignment ga karrali)
FILE_ALIGN = 0x200
SECT_ALIGN = 0x1000


def elf_info(data):
    assert data[:4] == b"\x7fELF" and data[4] == 2, "64-bit ELF emas"
    e_phoff, e_shoff = struct.unpack_from("<QQ", data, 0x20)
    e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHHHH", data, 0x36)
    segs = []
    for i in range(e_phnum):
        p_type, _flags, off, vaddr, _paddr, filesz, memsz, _al = struct.unpack_from("<IIQQQQQQ", data, e_phoff + i * e_phentsize)
        if p_type == 1 and memsz:
            segs.append((vaddr, off, filesz, memsz))
    syms = {}
    for i in range(e_shnum):
        sh = struct.unpack_from("<IIQQQQIIQQ", data, e_shoff + i * e_shentsize)
        if sh[1] != 2:  # SHT_SYMTAB
            continue
        strtab = struct.unpack_from("<IIQQQQIIQQ", data, e_shoff + sh[6] * e_shentsize)
        so = strtab[4]
        for j in range(sh[5] // 24):
            name, _info, _o, _shn, value, _size = struct.unpack_from("<IBBHQQ", data, sh[4] + j * 24)
            end = data.index(b"\0", so + name)
            syms[data[so + name:end].decode(errors="replace")] = value
    return segs, syms


def align(x, a):
    return (x + a - 1) // a * a


def main():
    elf = open(sys.argv[1], "rb").read()
    segs, syms = elf_info(elf)
    kstart, kend, entry = syms["__kernel_start"], syms["__kernel_end"], syms["efi_entry"]
    assert kstart % 0x10000 == 0
    base = kstart - HDR
    # Fayldagi tasvir: __kernel_start dan oxirgi fayl-baytli segmentgaça
    file_end = max(v + fs for v, _o, fs, _m in segs if fs)
    img = bytearray(file_end - kstart)
    for v, o, fs, _m in segs:
        img[v - kstart:v - kstart + fs] = elf[o:o + fs]
    raw = align(len(img), FILE_ALIGN)
    img += bytes(raw - len(img))
    vsize = align(kend - kstart, SECT_ALIGN)
    size_of_image = HDR + vsize
    headers_size = FILE_ALIGN

    # DOS sarlavhasi
    dos = bytearray(0x40)
    dos[0:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3c, 0x40)
    pe = b"PE\0\0"
    # COFF: AArch64, 1 bölim, OptionalHeader 240 bayt (PE32+ va 16 kataloq)
    chars = 0x0001 | 0x0002 | 0x0020  # RELOCS_STRIPPED | EXECUTABLE | LARGE_ADDRESS_AWARE
    coff = struct.pack("<HHIIIHH", 0xAA64, 1, 0, 0, 0, 240, chars)
    opt = struct.pack(
        "<HBBIIIII",
        0x20B, 1, 0,          # PE32+, linker versiyasi
        raw, 0, 0,             # SizeOfCode, SizeOfInitializedData, SizeOfUninitializedData
        entry - base,          # AddressOfEntryPoint
        HDR,                   # BaseOfCode
    )
    opt += struct.pack(
        "<QIIHHHHHHIIIIHHQQQQII",
        base, SECT_ALIGN, FILE_ALIGN,
        0, 0, 0, 0, 0, 0,      # OS/Image/Subsystem versiyalari
        0,                     # Win32VersionValue
        size_of_image, headers_size,
        0,                     # CheckSum
        10,                    # Subsystem: EFI_APPLICATION
        0,                     # DllCharacteristics
        0x10000, 0x10000, 0x10000, 0, # stek/heap rezerv/commit
        0,                     # LoaderFlags
        16,                    # NumberOfRvaAndSizes
    )
    opt += bytes(16 * 8)       # barça kataloglar böş (relokatsiya ham)
    assert len(opt) == 240, len(opt)
    # Bölim: kod + maʼlumot, RWX
    sec = struct.pack("<8sIIIIIIHHI", b".text", vsize, HDR, raw, headers_size, 0, 0, 0, 0,
                      0x00000020 | 0x00000040 | 0x20000000 | 0x40000000 | 0x80000000)
    hdr = bytearray(dos + pe + coff + opt + sec)
    assert len(hdr) <= headers_size
    hdr += bytes(headers_size - len(hdr))
    with open(sys.argv[2], "wb") as f:
        f.write(hdr)
        f.write(img)
    print(f"{sys.argv[2]}: ImageBase {base:#x}, kiriş {entry:#x}, tasvir {size_of_image // 1024} KiB, fayl {(len(hdr) + len(img)) // 1024} KiB")


if __name__ == "__main__":
    main()
