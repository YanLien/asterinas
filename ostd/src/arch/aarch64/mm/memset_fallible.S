/* SPDX-License-Identifier: MPL-2.0 */

.text
.balign 4

// Fills `size` bytes in the memory pointed to by `dst` with the value `value`.
// This function works with exception handling and can recover from a page fault.
//
// Returns number of bytes that failed to set.
.global __memset_fallible
.type __memset_fallible, @function
__memset_fallible: // (dst: *mut u8 in x0, value: u8 in w1, size: usize in x2) -> usize in x0
    cbz x2, memset_exit
memset_loop:
memset_store:
    strb w1, [x0]
    add x0, x0, #1
    sub x2, x2, #1
    cbnz x2, memset_loop
memset_exit:
    mov x0, x2
    ret
.size __memset_fallible, .-__memset_fallible

.pushsection .ex_table, "a", @progbits
    .balign 8
    .quad memset_store
    .quad memset_exit
.popsection
