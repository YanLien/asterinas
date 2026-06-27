/* SPDX-License-Identifier: MPL-2.0 */

.text
.balign 4

// Copies `size` bytes from `src` to `dst`. This function works with exception
// handling and can recover from a page fault. The source range must not overlap
// with the destination range (in virtual address level).
//
// Returns number of bytes that failed to copy.
//
// On Cortex-A72 (no FEAT_PAN) EL1 can access user mappings directly, so plain
// loads and stores are used and recover through the exception table.
.global __memcpy_fallible
.type __memcpy_fallible, @function
__memcpy_fallible: // (dst: *mut u8 in x0, src: *const u8 in x1, size: usize in x2) -> usize in x0
    cbz x2, memcpy_exit
memcpy_loop:
memcpy_load:
    ldrb w3, [x1]
memcpy_store:
    strb w3, [x0]
    add x0, x0, #1
    add x1, x1, #1
    sub x2, x2, #1
    cbnz x2, memcpy_loop
memcpy_exit:
    mov x0, x2
    ret
.size __memcpy_fallible, .-__memcpy_fallible

.pushsection .ex_table, "a", @progbits
    .balign 8
    .quad memcpy_load
    .quad memcpy_exit
    .quad memcpy_store
    .quad memcpy_exit
.popsection
