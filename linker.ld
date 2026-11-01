/* QEMU virt: RAM 0x4000_0000 dan boşlanadi.
   Birinchi 2 MiB DTB uchun bo'sh qoldiriladi (QEMU uni RAM boshiga qo'yadi,
   joy yetmasa — umuman qöymaydi). */
ENTRY(_start)

SECTIONS
{
    . = 0x40200000;
    __kernel_start = .;

    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    .rodata : ALIGN(16) { *(.rodata .rodata.*) }
    .data   : ALIGN(16) { *(.data .data.*) }

    .bss (NOLOAD) : ALIGN(16) {
        __bss_start = .;
        *(.bss .bss.*) *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }

    /* Har bir yadro (core) uçun 64 KiB stek; hozirça faqat core 0 işlaydi. */
    . = ALIGN(4096);
    __stack_bottom = .;
    . += 0x10000;
    __stack_top = .;

    __kernel_end = .;

    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
