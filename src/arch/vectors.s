// Istisnolar (exception) vektor jadvali: 16 ta yozuv, har biri 0x80 bayt.
// Har bir kirishda barcha umumiy registrlarni `TrapFrame` ga saqlaymiz.

.macro SAVE_AND_CALL kind
    sub     sp, sp, #(34 * 8)
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
    stp     x30, x0,  [sp, #(30 * 8)]
    str     x1,       [sp, #(32 * 8)]
    mov     x0, sp
    mov     x1, #\kind
    bl      handle_exception
    b       restore_and_eret
.endm

.macro VECTOR kind
.balign 0x80
    SAVE_AND_CALL \kind
.endm

.section .text
.balign 0x800
.global exception_vectors
exception_vectors:
    // Joriy EL, SP_EL0
    VECTOR 0
    VECTOR 1
    VECTOR 2
    VECTOR 3
    // Joriy EL, SP_ELx
    VECTOR 4
    VECTOR 5
    VECTOR 6
    VECTOR 7
    // Pastki EL, AArch64
    VECTOR 8
    VECTOR 9
    VECTOR 10
    VECTOR 11
    // Pastki EL, AArch32
    VECTOR 12
    VECTOR 13
    VECTOR 14
    VECTOR 15

restore_and_eret:
    ldp     x30, x0,  [sp, #(30 * 8)]
    ldr     x1,       [sp, #(32 * 8)]
    msr     elr_el1, x0
    msr     spsr_el1, x1
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
