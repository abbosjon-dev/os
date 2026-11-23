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
    mov     x20, x0                 // DTB manzili (EFI yuklovçidan; -kernel da 0)
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
    mrs     x0, id_aa64pfr0_el1     // GICv3 tizim registrlari bormi?
    ubfx    x0, x0, #24, #4
    cbz     x0, no_gicv3
    mov     x0, #0xf                // ICC_SRE_EL2: SRE + EL1 ga ruxsat
    msr     S3_4_C12_C9_5, x0
    isb
no_gicv3:
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
2:  mov     x0, x20
    bl      kernel_main

3:  mov     x0, x19
    bl      secondary_main

park:
    wfe
    b       park

// UEFI kiriş nuqtasi (PE AddressOfEntryPoint, qarang: scripts/efi/mkefi.py):
// x0 = ImageHandle, x1 = SystemTable — töğridan-töğri Rust `efi_main` ga.
.global efi_entry
efi_entry:
    b       efi_main

// EFI yuklovçidan ötiş: x0 = DTB, [x1, x2) — keşdan xotiraga tozalanadigan soha (yadro tasviri).
// Uzilişlar öçiriladi, maʼlumot keşi tozalanadi, MMU va keşlar öçiriladi, keyin `_start`.
.section .text
.global efi_handoff
efi_handoff:
    msr     daifset, #0xf
    mrs     x3, ctr_el0
    ubfx    x3, x3, #16, #4         // DminLine: log2(söz soni)
    mov     x4, #4
    lsl     x4, x4, x3              // keş qatori (bayt)
    sub     x5, x4, #1
    bic     x1, x1, x5
1:  dc      civac, x1
    add     x1, x1, x4
    cmp     x1, x2
    b.lo    1b
    dsb     sy
    mrs     x3, CurrentEL
    cmp     x3, #8
    b.eq    2f
    mrs     x3, sctlr_el1
    bic     x3, x3, #1              // M
    bic     x3, x3, #(1 << 2)       // C
    bic     x3, x3, #(1 << 12)      // I
    msr     sctlr_el1, x3
    isb
    ic      iallu
    tlbi    vmalle1
    b       3f
2:  mrs     x3, sctlr_el2
    bic     x3, x3, #1
    bic     x3, x3, #(1 << 2)
    bic     x3, x3, #(1 << 12)
    msr     sctlr_el2, x3
    isb
    ic      iallu
    tlbi    alle2
3:  dsb     sy
    isb
    b       _start
