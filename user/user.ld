/* Zamin OS foydalanuvçi dasturlari: foydalanuvçi yarmining boşi 0x40_0000_0000.
   Har bir bölim alohida sahifada — kod (RX) va maʼlumot (RW) aralaşmasin. */
ENTRY(_start)

SECTIONS
{
    . = 0x4000000000;
    .text : ALIGN(4096) {
        KEEP(*(.text.start))
        *(.text .text.*)
    }
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) }
    .data : ALIGN(4096) { *(.data .data.*) }
    .bss : ALIGN(16) { *(.bss .bss.*) *(COMMON) }

    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
