/* QEMU virt: RAM 0x4000_0000 dan boşlanadi.
   Birinçi 2 MiB DTB uçun böş qoldiriladi (QEMU uni RAM boşiga qöyadi,
   joy yetmasa — umuman qöymaydi). */
ENTRY(_start)

SECTIONS
{
    . = 0x40200000;
    __kernel_start = .;

    /* Har bir bölim alohida sahifalarda: MMU ularga turliça ruxsat beradi (W^X):
       .text — faqat öqiş + bajariş, .rodata — faqat öqiş, qolgani — öqiş/yoziş, bajarib bölmaydi. */
    __text_start = .;
    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }
    . = ALIGN(4096);
    __text_end = .;

    __rodata_start = .;
    .rodata : { *(.rodata .rodata.*) }
    . = ALIGN(4096);
    __rodata_end = .;

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
