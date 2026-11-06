// Istisnolar (exception) vektor jadvali: 16 ta yozuv, har biri 0x80 bayt.
// Har bir kirişda barça umumiy registrlarni va foydalanuvçi stekini (SP_EL0)
// `TrapFrame` ga saqlaymiz. Kadr hajmi: 34 * 8 = 272 bayt.

// Kadrni (sp allaqaçon kamaytirilgan) töldirib, Rust işlovçisini çaqiradi.
.macro SAVE_AND_CALL kind
    stp     x0, x1,   [sp, #(0 * 8)]
    stp     x2, x3,   [sp, #(2 * 8)]
    stp     x4, x5,   [sp, #(4 * 8)]
    stp     x6, x7,   [sp, #(6 * 8)]
    stp     x8, x9,   [sp, #(8 * 8)]
    stp     x10, x11, [sp, #(10 * 8)]
    stp     x12, x13, [sp, #(12 * 8)]
    stp     x14, x15, [sp, #(14 * 8)]
    stp     x16, x17, [sp, #(16 * 8)]
    stp     x18, x19, [sp, #(18 * 8)]
    stp     x20, x21, [sp, #(20 * 8)]
    stp     x22, x23, [sp, #(22 * 8)]
    stp     x24, x25, [sp, #(24 * 8)]
    stp     x26, x27, [sp, #(26 * 8)]
    stp     x28, x29, [sp, #(28 * 8)]
    mrs     x0, elr_el1
    mrs     x1, spsr_el1
    mrs     x2, sp_el0
    stp     x30, x0,  [sp, #(30 * 8)]
    stp     x1, x2,   [sp, #(32 * 8)]
    mov     x0, sp
    mov     x1, #\kind
    bl      handle_exception
    b       restore_and_eret
.endm

// Oddiy kiriş: kadr uçun joy ajratib, umumiy kodga ötadi.
.macro VECTOR kind
.balign 0x80
    sub     sp, sp, #(34 * 8)
    b       save_\kind
.endm

// Yadro rejimidagi kiriş (SP_EL1): avval stek töliş tekşiriladi.
// Yadro steklari 64 KiB ga tekislangan oynalarda turadi (60 KiB stek), eng
// pastki 4 KiB — xaritalanmagan qörğon. Demak SP ning 12..15-bitlari hammasi
// 0 bölsa — stek töldi. (Bayroqlarni özgartiriş xavfsiz: PSTATE SPSR da saqlangan.)
// Registrlarni buzmasdan tekşiriş uçun SP va x0 almaştirib işlatiladi.
.macro VECTOR_CHECKED kind
.balign 0x80
    sub     sp, sp, #(34 * 8)
    add     sp, sp, x0          // sp' = sp + x0
    sub     x0, sp, x0          // x0 = sp
    tst     x0, #0xf000         // oynaning pastki 4 KiB i?
    b.eq    1f                  // ha → stek töldi
    sub     x0, sp, x0          // x0 tiklandi
    sub     sp, sp, x0          // sp tiklandi
    b       save_\kind
1:  sub     x0, sp, x0
    sub     sp, sp, x0
    b       stack_overflow
.endm

.section .text
.balign 0x800
.global exception_vectors
exception_vectors:
    // Joriy EL, SP_EL0 (işlatilmaydi)
    VECTOR 0
    VECTOR 1
    VECTOR 2
    VECTOR 3
    // Joriy EL, SP_ELx — yadro kodi
    VECTOR_CHECKED 4
    VECTOR_CHECKED 5
    VECTOR_CHECKED 6
    VECTOR_CHECKED 7
    // Pastki EL, AArch64 — foydalanuvçi dasturlari (stek böş, tekşiruv kerak emas)
    VECTOR 8
    VECTOR 9
    VECTOR 10
    VECTOR 11
    // Pastki EL, AArch32 (qöllab-quvvatlanmaydi)
    VECTOR 12
    VECTOR 13
    VECTOR 14
    VECTOR 15

.irp kind, 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
save_\kind:
    SAVE_AND_CALL \kind
.endr

// Yadro steki töldi: şu yadroning favqulodda stekiga ötib, xatoni qayd qilamiz (tur 16).
// x0 vaqtinça TPIDR_EL1 da saqlanadi (yadro bu registrni boşqa işlatmaydi).
// Stek tepasi: __emergency_stacks + (yadro + 1) * 64 KiB.
stack_overflow:
    msr     tpidr_el1, x0
    mrs     x0, mpidr_el1
    and     x0, x0, #3
    add     x0, x0, #1
    lsl     x0, x0, #16
    mov     sp, x0
    ldr     x0, =__emergency_stacks
    add     sp, sp, x0
    mrs     x0, tpidr_el1
    sub     sp, sp, #(34 * 8)
    SAVE_AND_CALL 16

// Yangi foydalanuvçi jarayoni ham şu yerdan boşlanadi: uning yadro stekida
// tayyorlangan kadr bor (ELR = kiriş nuqtasi, SPSR = EL0, SP_EL0 = stek).
.global restore_and_eret
restore_and_eret:
    ldp     x30, x0,  [sp, #(30 * 8)]
    ldp     x1, x2,   [sp, #(32 * 8)]
    msr     elr_el1, x0
    msr     spsr_el1, x1
    msr     sp_el0, x2
    ldp     x0, x1,   [sp, #(0 * 8)]
    ldp     x2, x3,   [sp, #(2 * 8)]
    ldp     x4, x5,   [sp, #(4 * 8)]
    ldp     x6, x7,   [sp, #(6 * 8)]
    ldp     x8, x9,   [sp, #(8 * 8)]
    ldp     x10, x11, [sp, #(10 * 8)]
    ldp     x12, x13, [sp, #(12 * 8)]
    ldp     x14, x15, [sp, #(14 * 8)]
    ldp     x16, x17, [sp, #(16 * 8)]
    ldp     x18, x19, [sp, #(18 * 8)]
    ldp     x20, x21, [sp, #(20 * 8)]
    ldp     x22, x23, [sp, #(22 * 8)]
    ldp     x24, x25, [sp, #(24 * 8)]
    ldp     x26, x27, [sp, #(26 * 8)]
    ldp     x28, x29, [sp, #(28 * 8)]
    add     sp, sp, #(34 * 8)
    eret
