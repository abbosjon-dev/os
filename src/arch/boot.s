// Zamin OS — AArch64 boot kodi.
// QEMU yadroni EL1 yoki EL2 da boşlaydi. Biz:
//   1) faqat core 0 ni qoldiramiz, qolganlarini uxlatamiz,
//   2) EL2 bölsak — EL1 ga tuşamiz,
//   3) stek örnatamiz, .bss ni nollaymiz, Rust `kernel_main` ga ötamiz.

.section .text.boot
.global _start
_start:
    mrs     x1, mpidr_el1
    and     x1, x1, #0xff
    cbnz    x1, park

    mrs     x0, CurrentEL
    lsr     x0, x0, #2
    cmp     x0, #2
    b.ne    el1_entry

    // --- EL2 -> EL1 ---
    mov     x0, #(1 << 31)          // HCR_EL2.RW = 1: EL1 AArch64 rejimida
    msr     hcr_el2, x0
    mov     x0, #3                  // EL1 ga fizik taymer va hisoblagiçni ruxsat
    msr     cnthctl_el2, x0
    msr     cntvoff_el2, xzr
    mov     x0, #0x0800
    movk    x0, #0x30d0, lsl #16    // SCTLR_EL1: MMU öçiq, RES1 bitlar
    msr     sctlr_el1, x0
    mov     x0, #0x3c5              // SPSR: DAIF maskalangan, EL1h
    msr     spsr_el2, x0
    adr     x0, el1_entry
    msr     elr_el2, x0
    eret

el1_entry:
    ldr     x0, =__stack_top
    mov     sp, x0

    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
1:  cmp     x0, x1
    b.hs    2f
    str     xzr, [x0], #8
    b       1b
2:
    // FP/SIMD ni yoqamiz (Rust kompilyatori ulardan foydalanişi mumkin)
    mov     x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    bl      kernel_main

park:
    wfe
    b       park
