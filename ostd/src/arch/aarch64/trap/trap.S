/* SPDX-License-Identifier: MPL-2.0 */

.text

// ---------------------------------------------------------------------------
// Exception vector table (pointed to by VBAR_EL1).
// ---------------------------------------------------------------------------

.macro VECTOR_ENTRY label
    .balign 0x80
    b \label
.endm

// The TrapFrame layout (kernel traps): general regs (31*8=248) at offset 0,
// sp at 248, pc (ELR) at 256, pstate (SPSR) at 264. Frame size 288 (16-aligned).
.set TF_SP,      248
.set TF_PC,      256
.set TF_PSTATE,  264
.set TF_SIZE,    288

// The RawUserContext layout used by run_user and the EL0 vector.
.set UC_SP,      248
.set UC_PC,      256
.set UC_PSTATE,  264
.set UC_TP,      272
.set UC_ESR,     280
.set UC_FAR,     288
.set UC_IS_IRQ,  296

// Temporary stack space for the EL0 vector (31 general regs = 248 bytes).
.set EL0_TMP,    256

.balign 0x800
.global exception_vector
exception_vector:
    // Current EL with SP_EL0
    VECTOR_ENTRY vector_panic     // sync
    VECTOR_ENTRY vector_panic     // irq
    VECTOR_ENTRY vector_panic     // fiq
    VECTOR_ENTRY vector_panic     // serror
    // Current EL with SP_ELx (SPx)
    VECTOR_ENTRY el1_sync         // sync
    VECTOR_ENTRY el1_irq          // irq
    VECTOR_ENTRY vector_panic     // fiq
    VECTOR_ENTRY vector_panic     // serror
    // Lower EL, AArch64, with SP_EL0
    VECTOR_ENTRY el0_sync         // sync
    VECTOR_ENTRY el0_irq          // irq
    VECTOR_ENTRY vector_panic     // fiq
    VECTOR_ENTRY vector_panic     // serror
    // Lower EL, AArch64, with SP_ELx
    VECTOR_ENTRY el0_sync         // sync
    VECTOR_ENTRY el0_irq          // irq
    VECTOR_ENTRY vector_panic     // fiq
    VECTOR_ENTRY vector_panic     // serror

vector_panic:
    b vector_panic

// ---------------------------------------------------------------------------
// Saves general registers x0..x30 to [sp]. Clobbers nothing else.
// ---------------------------------------------------------------------------
.macro SAVE_GP
    stp x0, x1, [sp]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
.endm

.macro RESTORE_GP
    ldp x0, x1, [sp]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
.endm

// ---------------------------------------------------------------------------
// Synchronous exception from the kernel (current EL, SPx).
// ---------------------------------------------------------------------------
.balign 4
el1_sync:
    sub sp, sp, #TF_SIZE
    SAVE_GP
    add x0, sp, #TF_SIZE
    str x0, [sp, #TF_SP]
    mrs x0, elr_el1
    str x0, [sp, #TF_PC]
    mrs x0, spsr_el1
    str x0, [sp, #TF_PSTATE]
    mov x0, sp
    bl trap_handler
    b el1_return

// ---------------------------------------------------------------------------
// Interrupt from the kernel (current EL, SPx).
// ---------------------------------------------------------------------------
.balign 4
el1_irq:
    sub sp, sp, #TF_SIZE
    SAVE_GP
    add x0, sp, #TF_SIZE
    str x0, [sp, #TF_SP]
    mrs x0, elr_el1
    str x0, [sp, #TF_PC]
    mrs x0, spsr_el1
    str x0, [sp, #TF_PSTATE]
    mov x0, sp
    bl irq_handler

el1_return:
    RESTORE_GP
    ldr x0, [sp, #TF_PC]
    ldr x1, [sp, #TF_PSTATE]
    msr elr_el1, x0
    msr spsr_el1, x1
    add sp, sp, #TF_SIZE
    eret

// ---------------------------------------------------------------------------
// Saves the user context into the active RawUserContext and returns from
// `run_user`. The `irq_flag` argument selects whether the trap is reported as
// an interrupt (1) or a synchronous exception (0).
// ---------------------------------------------------------------------------
.macro EL0_HANDLER name, irq_flag
.balign 4
\name:
    sub sp, sp, #EL0_TMP
    SAVE_GP
    // Read the exception metadata and the user's system registers.
    mrs x0, esr_el1
    mrs x1, far_el1
    mrs x2, sp_el0
    mrs x3, elr_el1
    mrs x4, spsr_el1
    mrs x5, tpidr_el0
    // Recover the user context pointer from the global.
    adrp x6, CURRENT_USER_CONTEXT
    add  x6, x6, :lo12:CURRENT_USER_CONTEXT
    ldr  x6, [x6]
    // Copy the 31 saved general registers from the temp area into the context.
    mov x7, sp
    mov x8, x6
    mov x9, #31
1:
    ldr x10, [x7], #8
    str x10, [x8], #8
    subs x9, x9, #1
    b.ne 1b
    // Store the saved system registers.
    str x2, [x6, #UC_SP]
    str x3, [x6, #UC_PC]
    str x4, [x6, #UC_PSTATE]
    str x5, [x6, #UC_TP]
    str x0, [x6, #UC_ESR]
    str x1, [x6, #UC_FAR]
    .if \irq_flag
    mov x10, #1
    str x10, [x6, #UC_IS_IRQ]
    .else
    str xzr, [x6, #UC_IS_IRQ]
    .endif
    add sp, sp, #EL0_TMP
    b run_user_return
.endm

EL0_HANDLER el0_sync, 0
EL0_HANDLER el0_irq, 1

// ---------------------------------------------------------------------------
// Enter user mode from a RawUserContext. x0 = &mut RawUserContext.
// On any EL0 trap, control returns to `run_user_return` below.
// ---------------------------------------------------------------------------
.balign 4
.global run_user
.type run_user, @function
run_user:
    // Save the kernel callee-saved registers (x19-x30).
    stp x29, x30, [sp, #-16]!
    stp x27, x28, [sp, #-16]!
    stp x25, x26, [sp, #-16]!
    stp x23, x24, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    stp x19, x20, [sp, #-16]!

    // Record the active user context pointer for the EL0 vector.
    adrp x1, CURRENT_USER_CONTEXT
    add  x1, x1, :lo12:CURRENT_USER_CONTEXT
    str  x0, [x1]

    // Load the user's system registers from the context and program them.
    ldr x9, [x0, #UC_SP]
    ldr x10, [x0, #UC_PC]
    ldr x11, [x0, #UC_PSTATE]
    ldr x12, [x0, #UC_TP]
    msr sp_el0, x9
    msr elr_el1, x10
    msr spsr_el1, x11
    msr tpidr_el0, x12

    // Load user general registers x1..x30, then x0 last.
    ldp x1, x2, [x0, #8]
    ldp x3, x4, [x0, #24]
    ldp x5, x6, [x0, #40]
    ldp x7, x8, [x0, #56]
    ldp x9, x10, [x0, #72]
    ldp x11, x12, [x0, #88]
    ldp x13, x14, [x0, #104]
    ldp x15, x16, [x0, #120]
    ldp x17, x18, [x0, #136]
    ldp x19, x20, [x0, #152]
    ldp x21, x22, [x0, #168]
    ldp x23, x24, [x0, #184]
    ldp x25, x26, [x0, #200]
    ldp x27, x28, [x0, #216]
    ldr x29, [x0, #232]
    ldr x30, [x0, #240]
    ldr x0, [x0]

    eret

// The EL0 trap vectors branch here after saving the user context and popping
// the temporary save area. SP points at the callee-saved frame pushed by
// `run_user`. Restore it and return into the Rust `run()` caller.
.balign 4
run_user_return:
    ldp x19, x20, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x23, x24, [sp], #16
    ldp x25, x26, [sp], #16
    ldp x27, x28, [sp], #16
    ldp x29, x30, [sp], #16
    ret
.size run_user, .-run_user
