/* SPDX-License-Identifier: MPL-2.0 */

// The boot routine executed by application processors (APs) on AArch64.
//
// An AP is woken by PSCI `CPU_ON` at `ap_boot_start` (physical address) with
// the MMU off. It installs the boot page table (which identity-maps the low
// physical region and maps the kernel image at the high virtual address),
// enables the MMU, switches to the high virtual address, claims a CPU id,
// loads its stack and CPU-local storage, and enters Rust.

KERNEL_VMA_OFFSET = 0xFFFFFFFF00000000

PTE_VALID     = 0x1

.section ".ap_boot", "awx", @progbits

.balign 4096
.global ap_boot_start
ap_boot_start:
    // --- Set up MAIR_EL1, TCR_EL1, CPACR_EL1 (same as the BSP) -------------
    mrs x0, cpacr_el1
    orr x0, x0, #0x300000         // bits 21:20 = 0b11
    msr cpacr_el1, x0

    ldr x0, =0x00FF
    msr mair_el1, x0

    ldr x0, =0x5B5103510
    msr tcr_el1, x0

    // --- Load the boot page table root and set TTBR0/TTBR1 -----------------
    adr x0, __ap_boot_page_table_pointer
    ldr x0, [x0]
    msr ttbr0_el1, x0
    msr ttbr1_el1, x0
    isb

    // --- Enable the MMU -----------------------------------------------------
    ic iallu
    dsb sy
    tlbi vmalle1is
    dsb sy
    isb

    mrs x0, sctlr_el1
    orr x0, x0, #0x1              // M (bit 0)
    orr x0, x0, #0x4              // C (bit 2)
    orr x0, x0, #0x1000           // I (bit 12)
    msr sctlr_el1, x0
    isb

    // --- Switch to the high virtual address ---------------------------------
    ldr x0, =ap_boot_virtual
    br x0

// From here we are in the canonical high kernel virtual address space.

.balign 4
ap_boot_virtual:
    // Atomically claim a CPU id by incrementing __ap_boot_cpu_id_tail.
    adr x0, __ap_boot_cpu_id_tail
1:
    ldxr x1, [x0]
    add x2, x1, #1
    stxr w3, x2, [x0]
    cbnz w3, 1b
    // x1 now holds our CPU id (1, 2, ...).

    // Load the PerApRawInfo array pointer.
    adr x2, __ap_boot_info_array_pointer
    ldr x2, [x2]

    // Each PerApRawInfo is 16 bytes. Offset = (cpu_id - 1) * 16.
    sub x3, x1, #1
    lsl x3, x3, #4
    add x3, x3, x2

    // Load the stack top and CPU-local storage base.
    ldr x4, [x3]                  // stack_top
    ldr x5, [x3, #8]             // cpu_local

    mov sp, x4
    msr tpidr_el1, x5
    mov x29, xzr                  // clear the frame pointer

    // Enter Rust with the CPU id in x0.
    mov x0, x1
    ldr x1, =aarch64_ap_early_entry
    br x1

// Data filled in by the BSP before starting the APs.
.balign 8
.global __ap_boot_page_table_pointer
__ap_boot_page_table_pointer:
    .quad 0
.global __ap_boot_info_array_pointer
__ap_boot_info_array_pointer:
    .quad 0
// Atomically incremented when an AP starts. The BSP is CPU 0; the first AP
// becomes CPU 1.
.global __ap_boot_cpu_id_tail
__ap_boot_cpu_id_tail:
    .quad 1
