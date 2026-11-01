// Kontekst almaştiriş: `switch_context(old: *mut Context, new: *const Context)`.
// Faqat "callee-saved" registrlar (x19–x30) va SP saqlanadi — qolganlarini
// Rust kompilyatori çaqiruvdan oldin özi saqlab qöyadi.
// Context tuzilişi: x19..x28, x29, x30 (lr), sp — 13 ta söz.

.section .text
.global switch_context
switch_context:
    stp     x19, x20, [x0, #0]
    stp     x21, x22, [x0, #16]
    stp     x23, x24, [x0, #32]
    stp     x25, x26, [x0, #48]
    stp     x27, x28, [x0, #64]
    stp     x29, x30, [x0, #80]
    mov     x9, sp
    str     x9,       [x0, #96]

    ldp     x19, x20, [x1, #0]
    ldp     x21, x22, [x1, #16]
    ldp     x23, x24, [x1, #32]
    ldp     x25, x26, [x1, #48]
    ldp     x27, x28, [x1, #64]
    ldp     x29, x30, [x1, #80]
    ldr     x9,       [x1, #96]
    mov     sp, x9
    ret

// Yangi yadro oqimining boşlaniş nuqtasi.
// x19 = funksiya, x20 = argument. Uzilişlarni yoqib, funksiyani çaqiramiz;
// u qaytsa — oqim tugaydi.
.global kthread_trampoline
kthread_trampoline:
    msr     daifclr, #2
    mov     x0, x20
    blr     x19
    bl      kthread_exit
1:  b       1b
