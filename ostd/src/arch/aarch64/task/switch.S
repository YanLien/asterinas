/* SPDX-License-Identifier: MPL-2.0 */

.text

.global context_switch
.type context_switch, @function
context_switch: // (nxt: *const TaskContext in x0, cur: *mut TaskContext in x1)
    // Save the current callee-saved context into *cur (x1). SP cannot be a
    // store source on AArch64, so route it through x9.
    mov x9, sp
    str x29, [x1, #88]
    str lr,  [x1, #96]
    str x9,  [x1]
    stp x19, x20, [x1, #8]
    stp x21, x22, [x1, #24]
    stp x23, x24, [x1, #40]
    stp x25, x26, [x1, #56]
    stp x27, x28, [x1, #72]
    // Fall through to restore from *nxt.

.global first_context_switch
.type first_context_switch, @function
first_context_switch: // (nxt: *const TaskContext in x0)
    // Restore the next context from *nxt (x0). Load the saved SP into x9 first
    // and install it after restoring the callee-saved registers.
    ldr x9,  [x0]
    ldr x29, [x0, #88]
    ldr lr,  [x0, #96]
    ldp x19, x20, [x0, #8]
    ldp x21, x22, [x0, #24]
    ldp x23, x24, [x0, #40]
    ldp x25, x26, [x0, #56]
    ldp x27, x28, [x0, #72]
    mov sp, x9

    ret
.size context_switch, .-context_switch
.size first_context_switch, .-first_context_switch

.global kernel_task_entry_wrapper
.type kernel_task_entry_wrapper, @function
kernel_task_entry_wrapper:
    .cfi_startproc
    .cfi_undefined lr // mark return address as undefined to indicate end of call stack
    bl kernel_task_entry
    .cfi_endproc
.size kernel_task_entry_wrapper, .-kernel_task_entry_wrapper
