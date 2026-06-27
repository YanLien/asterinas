/* SPDX-License-Identifier: MPL-2.0 */

// The kernel is built with the soft-float target so that the compiler avoids
// using FP/SIMD registers, but the hardware (Cortex-A72) still provides them.
// Enable the FP extension for this file so the FPSIMD save/restore code
// assembles.
.arch_extension fp

.text

.global save_fpu_context
.type save_fpu_context, @function
save_fpu_context: // (ctx: *mut FpuState in x0)
    stp q0, q1, [x0]
    stp q2, q3, [x0, #0x20]
    stp q4, q5, [x0, #0x40]
    stp q6, q7, [x0, #0x60]
    stp q8, q9, [x0, #0x80]
    stp q10, q11, [x0, #0xa0]
    stp q12, q13, [x0, #0xc0]
    stp q14, q15, [x0, #0xe0]
    stp q16, q17, [x0, #0x100]
    stp q18, q19, [x0, #0x120]
    stp q20, q21, [x0, #0x140]
    stp q22, q23, [x0, #0x160]
    stp q24, q25, [x0, #0x180]
    stp q26, q27, [x0, #0x1a0]
    stp q28, q29, [x0, #0x1c0]
    stp q30, q31, [x0, #0x1e0]
    mrs x1, fpsr
    str w1, [x0, #0x200]
    mrs x1, fpcr
    str w1, [x0, #0x204]
    ret
.size save_fpu_context, .-save_fpu_context

.global load_fpu_context
.type load_fpu_context, @function
load_fpu_context: // (ctx: *const FpuState in x0)
    ldp q0, q1, [x0]
    ldp q2, q3, [x0, #0x20]
    ldp q4, q5, [x0, #0x40]
    ldp q6, q7, [x0, #0x60]
    ldp q8, q9, [x0, #0x80]
    ldp q10, q11, [x0, #0xa0]
    ldp q12, q13, [x0, #0xc0]
    ldp q14, q15, [x0, #0xe0]
    ldp q16, q17, [x0, #0x100]
    ldp q18, q19, [x0, #0x120]
    ldp q20, q21, [x0, #0x140]
    ldp q22, q23, [x0, #0x160]
    ldp q24, q25, [x0, #0x180]
    ldp q26, q27, [x0, #0x1a0]
    ldp q28, q29, [x0, #0x1c0]
    ldp q30, q31, [x0, #0x1e0]
    ldr w1, [x0, #0x200]
    msr fpsr, x1
    ldr w1, [x0, #0x204]
    msr fpcr, x1
    ret
.size load_fpu_context, .-load_fpu_context
