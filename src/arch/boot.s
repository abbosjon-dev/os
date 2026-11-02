// Zamin OS — AArch64 boot kodi.
// QEMU yadroni EL1 yoki EL2 da boşlaydi. Biz:
//   1) `_start` — faqat birinçi yadro (core 0); qolganlari uxlaydi va keyin
//      PSCI CPU_ON orqali `secondary_entry` dan uyğotiladi (x0 = yadro raqami),
//   2) EL2 bölsak — EL1 ga tuşamiz,
//   3) har bir yadro öz stekini örnatadi (__boot_stacks + (n+1) * 64 KiB),
//   4) birinçi yadro .bss ni nollab `kernel_main` ga, qolganlari `secondary_main` ga ötadi.
// x19 = yadro raqami (eret orqali ham saqlanadi).

.section .text.boot
.global _start
_start:
    mrs     x1, mpidr_el1
    and     x1, x1, #0xff
    cbnz    x1, park
    mov     x19, #0
    b       common_entry

.global secondary_entry
secondary_entry:
    mov     x19, x0

common_entry:
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
    // Stek: har bir yadroga 64 KiB oyna (yuqori yarmi işlatiladi, qarang: vectors.s)
    ldr     x0, =__boot_stacks
    add     x1, x19, #1
    lsl     x1, x1, #16
    add     x0, x0, x1
    mov     sp, x0

    // FP/SIMD ni yoqamiz (foydalanuvçi dasturlari uçun)
    mov     x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    cbnz    x19, 3f
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
1:  cmp     x0, x1
    b.hs    2f
    str     xzr, [x0], #8
    b       1b
2:  bl      kernel_main

3:  mov     x0, x19
    bl      secondary_main

park:
    wfe
    b       park
