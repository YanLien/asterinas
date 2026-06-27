/* SPDX-License-Identifier: MPL-2.0 */

// The boot routine executed by the bootstrap processor (BSP) on AArch64.
//
// On QEMU `virt` the kernel is loaded at 0x40080000 and entered with:
//   x0 = physical address of the flattened device tree (dtb).
//
// The boot code may run at EL1 or EL2; it drops to EL1 if necessary, builds
// minimal identity (TTBR0) and kernel (TTBR1) page tables, enables the MMU,
// switches to the high kernel virtual address, sets up the CPU-local storage
// pointer and jumps to the Rust entry `aarch64_boot`.

KERNEL_LMA        = 0x40080000
KERNEL_VMA_OFFSET = 0xFFFFFFFF00000000

// Page-table descriptor bits.
PTE_VALID     = 0x1
PTE_TABLE     = 0x3       // valid + descriptor-type bit (table/page)
PTE_BLOCK_AF  = 0x701     // valid(block) | SH=inner | AF, AttrIndx=0 (Normal WB)

.section ".boot", "awx", @progbits

.balign 16
.global _start
_start:
    // Preserve x0-x3 (boot registers) across the boot setup (the EL2 drop
    // and the MMU enable sequence clobber x0/x1).
    mov x20, x0
    mov x21, x1
    mov x22, x2
    mov x23, x3

    // If entered at EL2, drop to EL1.
    mrs x1, CurrentEL
    lsr x1, x1, #2
    cmp x1, #2
    b.eq el2_to_el1

el1_start:
    // --- Enable FP/SIMD access (CPACR_EL1.FPEN = 0b11) ---------------------
    mrs x0, cpacr_el1
    orr x0, x0, #0x300000         // bits 21:20 = 0b11
    msr cpacr_el1, x0

    // --- MAIR_EL1: Attr0 = Normal WB (0xFF), Attr1 = Device-nGnRnE (0x00) --
    ldr x0, =0x00FF
    msr mair_el1, x0

    // --- TCR_EL1: 48-bit VA for both TTBR0 and TTBR1, 4KB granule ----------
    // T0SZ=16 IRGN0=WB ORGN0=WB SH0=inner TG0=4K
    // T1SZ=16 IRGN1=WB ORGN1=WB SH1=inner TG1=4K IPS=48-bit
    ldr x0, =0x5B5103510
    msr tcr_el1, x0

    // --- Build the boot page tables -----------------------------------------
    adr x0, boot_l0
    adr x1, boot_l1_low
    orr x2, x1, #PTE_TABLE
    str x2, [x0]                  // boot_l0[0]    -> boot_l1_low (identity)
    adr x1, boot_l1_linear
    orr x2, x1, #PTE_TABLE
    str x2, [x0, #2048]           // boot_l0[256]  -> boot_l1_linear (linear map)
    adr x1, boot_l1_high
    orr x2, x1, #PTE_TABLE
    str x2, [x0, #4088]           // boot_l0[511]  -> boot_l1_high (kernel VA)

    adr x1, boot_l1_high
    ldr x2, =0x40000701           // block: phys 0x40000000, Normal WB, AF
    str x2, [x1, #4072]           // boot_l1_high[509] maps KERNEL_VMA's 1 GiB

    // --- Load TTBR0_EL1 and TTBR1_EL1 with the same root -------------------
    adr x0, boot_l0
    msr ttbr0_el1, x0
    msr ttbr1_el1, x0
    isb

    // --- Flush caches and TLB before enabling the MMU ----------------------
    ic iallu
    dsb sy
    tlbi vmalle1is
    dsb sy
    isb

    // --- Enable the MMU (SCTLR_EL1.M=1, C=1, I=1) --------------------------
    mrs x0, sctlr_el1
    orr x0, x0, #0x1              // M (bit 0)
    orr x0, x0, #0x4              // C (bit 2)
    orr x0, x0, #0x1000           // I (bit 12)
    msr sctlr_el1, x0
    isb

    // --- Jump to the high kernel virtual address ----------------------------
    ldr x0, =bsp_boot_virt
    br x0

// We are now executing at the canonical high kernel virtual address.

.balign 4
bsp_boot_virt:
    // Switch to the boot stack at the high kernel virtual address.
    ldr x0, =boot_stack_top
    ldr x1, =KERNEL_VMA_OFFSET
    add sp, x0, x1

    // Set the CPU-local storage base in TPIDR_EL1.
    ldr x0, =__cpu_local_start
    msr tpidr_el1, x0

    // Install the exception vector early so that any trap during early
    // initialization is caught by the trap handler instead of looping on the
    // reset vector at VBAR=0.
    ldr x0, =exception_vector
    msr vbar_el1, x0
    isb

    // Restore x0-x3 (boot registers) and enter Rust.
    mov x0, x20
    mov x1, x21
    mov x2, x22
    mov x3, x23
    ldr x4, =aarch64_boot
    br x4

// Drop from EL2 to EL1.
.balign 4
el2_to_el1:
    // Configure HCR_EL2.RW so EL1 is AArch64.
    mov x0, #0x80000000
    msr hcr_el2, x0

    // Set SPSR_EL2 to EL1h with all DAIF interrupts masked.
    ldr x0, =0x3c5
    msr spsr_el2, x0

    // Return into el1_start at EL1.
    adr x0, el1_start
    msr elr_el2, x0
    eret

// ---------------------------------------------------------------------------
// Boot page tables (in the .boot section so they live at physical addresses).
// ---------------------------------------------------------------------------
.balign 4096
boot_l0:
    .fill 512, 8, 0

.balign 4096
boot_l1_low:
    // Identity-map the first 16 GiB of the physical address space as 1 GiB
    // Normal-WB blocks (covers the kernel image, RAM and the device tree on
    // the QEMU `virt` machine).
    .set i, 0
    .rept 16
        .quad (PTE_BLOCK_AF) | (i * 0x40000000)
        .set i, i + 1
    .endr
    .fill 496, 8, 0

.balign 4096
boot_l1_linear:
    // Linear-map the first 16 GiB of the physical address space at the kernel's
    // linear-mapping base (0xFFFF_8000_0000_0000 for a 48-bit VA), so that
    // paddr_to_vaddr() works before the kernel page table is activated. The
    // device tree, RAM and the PL011 serial port are all reached this way.
    .set i, 0
    .rept 16
        .quad (PTE_BLOCK_AF) | (i * 0x40000000)
        .set i, i + 1
    .endr
    .fill (512 - 16), 8, 0

.balign 4096
boot_l1_high:
    // Filled at runtime; maps the kernel image at its linked KERNEL_VMA.
    .fill 512, 8, 0

.section ".boot.stack", "aw", @nobits

boot_stack_bottom:
    .balign 4096
    .skip 0x800000  // 8 MiB boot stack
boot_stack_top:
