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

    /* Yuklaniş steklari (har bir yadroning PID 0 vazifasi): 4 ta 64 KiB oyna,
       64 KiB ga tekislangan; har birida faqat yuqori 32 KiB işlatiladi. Stek
       körsatkiçining 15-biti 0 bölib qolsa — stek töldi (qarang: arch/vectors.s). */
    . = ALIGN(0x10000);
    __boot_stacks = .;
    . += 4 * 0x10000;

    /* Favqulodda steklar (har bir yadroga): yadro steki tölganda xato şu yerda işlanadi. */
    . = ALIGN(0x10000);
    __emergency_stacks = .;
    . += 4 * 0x10000;

    __kernel_end = .;

    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
