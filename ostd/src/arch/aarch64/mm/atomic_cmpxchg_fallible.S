/* SPDX-License-Identifier: MPL-2.0 */

.text
.balign 4

// Atomically compares and exchanges a 32-bit integer value. This function works
// with exception handling and can recover from a page fault.
//
// Returns the previous value or `!0u64` if failed to update.
.global __atomic_cmpxchg_fallible
.type __atomic_cmpxchg_fallible, @function
__atomic_cmpxchg_fallible: // (ptr: *mut u32 in x0, old_val: u32 in w1, new_val: u32 in w2) -> u64 in x0
cmpxchg_loop:
cmpxchg_load:
    ldxr w3, [x0]
    cmp w3, w1
    b.ne cmpxchg_done
cmpxchg_store:
    stxr w4, w2, [x0]
    cbnz w4, cmpxchg_loop
    // Success: w3 already holds the loaded value, which equals old_val.
cmpxchg_done:
    mov x0, x3
    ret
cmpxchg_fault:
    mov x0, #-1
    ret
.size __atomic_cmpxchg_fallible, .-__atomic_cmpxchg_fallible

.pushsection .ex_table, "a", @progbits
    .balign 8
    .quad cmpxchg_load
    .quad cmpxchg_fault
    .quad cmpxchg_store
    .quad cmpxchg_fault
.popsection
