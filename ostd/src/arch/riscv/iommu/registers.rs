// SPDX-License-Identifier: MPL-2.0

//! RISC-V IOMMU MMIO register interface.
//!
//! Register layout follows the RISC-V IOMMU Architecture Specification
//! (chapter 5), as mirrored by the Linux `drivers/iommu/riscv/iommu-bits.h`.
//! Reference: <https://github.com/riscv-non-isa/riscv-iommu>

use crate::io::{IoMem, Sensitive};

/// MMIO register offsets (chapter 5 of the spec).
mod offset {
    pub const CAPABILITIES: usize = 0x0000; // 64-bit
    pub const FCTL: usize = 0x0008; // 32-bit
    pub const DDTP: usize = 0x0010; // 64-bit
    pub const CQB: usize = 0x0018; // 64-bit
    pub const CQH: usize = 0x0020; // 32-bit
    pub const CQT: usize = 0x0024; // 32-bit
    pub const FQB: usize = 0x0028; // 64-bit
    pub const FQH: usize = 0x0030; // 32-bit
    pub const FQT: usize = 0x0034; // 32-bit
    pub const PQB: usize = 0x0038; // 64-bit
    pub const PQH: usize = 0x0040; // 32-bit
    pub const PQT: usize = 0x0044; // 32-bit
    pub const CQCSR: usize = 0x0048; // 32-bit
    pub const FQCSR: usize = 0x004C; // 32-bit
    pub const PQCSR: usize = 0x0050; // 32-bit
    pub const IPSR: usize = 0x0054; // 32-bit
    pub const ICVEC: usize = 0x02F8; // 64-bit
}

/// CAPABILITIES bits.
mod caps {
    pub const VERSION: u64 = 0xFF; // [7:0]
    pub const SV39: u64 = 1 << 9;
    pub const SV48: u64 = 1 << 10;
    pub const SV57: u64 = 1 << 11;
    pub const SVPBMT: u64 = 1 << 15;
    pub const SV39X4: u64 = 1 << 17;
    pub const SV48X4: u64 = 1 << 18;
    pub const SV57X4: u64 = 1 << 19;
    pub const MSI_FLAT: u64 = 1 << 22;
    pub const END: u64 = 1 << 27;
}

/// Queue CSR bits (identical layout for CQCSR/FQCSR/PQCSR).
mod qcsr {
    pub const QUEUE_ENABLE: u32 = 1 << 0;
    pub const QUEUE_INTR_ENABLE: u32 = 1 << 1;
    pub const QUEUE_MEM_FAULT: u32 = 1 << 8;
    pub const QUEUE_OVERFLOW: u32 = 1 << 9;
    pub const QUEUE_ACTIVE: u32 = 1 << 16;
    pub const QUEUE_BUSY: u32 = 1 << 17;
    // Command-queue specific error bits (only meaningful for CQCSR).
    pub const CMD_TO: u32 = 1 << 9; // overlaps with QUEUE_OVERFLOW
    pub const CMD_ILL: u32 = 1 << 10;
}

/// DDTP bit layout.
mod ddtp {
    pub const MODE: u64 = 0xF; // [3:0]
    pub const BUSY: u64 = 1 << 4;
    pub const PPN_FIELD: u64 = ((1u64 << 44) - 1) << 10; // [53:10]
}

/// DDTP mode values.
pub mod ddtp_mode {
    pub const OFF: u64 = 0;
    pub const BARE: u64 = 1;
    pub const LVL1: u64 = 2;
    pub const LVL2: u64 = 3;
    pub const LVL3: u64 = 4;
}

/// Encodes a physical byte address into the PPN field layout used by queue
/// base registers and DDTP ([53:10]).
pub(crate) fn phys_to_ppn(pa: usize) -> u64 {
    ((pa as u64) >> 2) & ddtp::PPN_FIELD
}

/// Encodes a physical byte address into the ATP PPN field layout
/// ([43:0]) used by IOSATP/IOHGATP context fields.
pub(crate) fn atp_ppn(pa: usize) -> u64 {
    ((pa as u64) >> 12) & ((1u64 << 44) - 1)
}

/// The IOMMU MMIO register interface.
pub(super) struct IommuRegisters {
    io_mem: IoMem<Sensitive>,
}

impl IommuRegisters {
    /// Creates a new register interface from the IOMMU's MMIO region.
    pub(super) fn new(io_mem: IoMem<Sensitive>) -> Self {
        Self { io_mem }
    }

    fn read64(&self, offset: usize) -> u64 {
        // SAFETY: Offsets are fixed and within the reserved MMIO window.
        unsafe { self.io_mem.read_once::<u64>(offset) }
    }

    fn read32(&self, offset: usize) -> u32 {
        // SAFETY: Offsets are fixed and within the reserved MMIO window.
        unsafe { self.io_mem.read_once::<u32>(offset) }
    }

    pub(super) fn write64(&self, offset: usize, val: u64) {
        // SAFETY: Offsets are fixed and within the reserved MMIO window.
        unsafe { self.io_mem.write_once(offset, &val) }
    }

    pub(super) fn write32(&self, offset: usize, val: u32) {
        // SAFETY: Offsets are fixed and within the reserved MMIO window.
        unsafe { self.io_mem.write_once(offset, &val) }
    }

    // ---- Capabilities ----

    fn capabilities(&self) -> u64 {
        self.read64(offset::CAPABILITIES)
    }

    /// Returns the spec version as `(major, minor)` from CAPABILITIES [7:0].
    pub(super) fn version(&self) -> (u8, u8) {
        let v = self.capabilities() & caps::VERSION;
        let minor = (v & 0xF) as u8;
        let major = ((v >> 4) & 0xF) as u8;
        (major, minor)
    }

    /// Returns true if first-stage IOSATP Sv48 translation is supported.
    pub(super) fn supports_sv48(&self) -> bool {
        self.capabilities() & caps::SV48 != 0
    }

    /// Returns true if the Device Context uses the extended format (64 bytes).
    pub(super) fn extended_dc_format(&self) -> bool {
        self.capabilities() & caps::MSI_FLAT != 0
    }

    // ---- DDTP (Device Directory Table Pointer) ----

    /// Reads the DDTP register.
    pub(super) fn ddtp(&self) -> u64 {
        self.read64(offset::DDTP)
    }

    /// Returns the DDTP mode field ([3:0]).
    pub(super) fn ddtp_mode(&self) -> u64 {
        self.ddtp() & ddtp::MODE
    }

    /// Returns true if the DDTP BUSY bit is set.
    pub(super) fn ddtp_busy(&self) -> bool {
        self.ddtp() & ddtp::BUSY != 0
    }

    /// Writes the DDTP register with `mode` and the PPN of the DDT root page.
    pub(super) fn write_ddtp(&self, mode: u64, ddt_paddr: usize) {
        let val = (mode & ddtp::MODE) | phys_to_ppn(ddt_paddr);
        self.write64(offset::DDTP, val);
    }

    // ---- Command Queue ----

    /// Returns the hardware-supported queue depth exponent via WARL.
    /// Writes the maximum log2sz field and reads back the accepted value.
    fn queue_log2sz_max(&self, base_offset: usize) -> u64 {
        // Probe with all bits of the LOG2SZ field set.
        self.write64(base_offset, 0x1F);
        let readback = self.read64(base_offset);
        readback & 0x1F
    }

    /// Programs a queue base register with the PPN of the queue memory and the
    /// depth exponent. `log2sz` is the accepted depth exponent (entries =
    /// 2^(log2sz+1)).
    fn write_queue_base(&self, base_offset: usize, queue_paddr: usize, log2sz: u64) -> u64 {
        let val = phys_to_ppn(queue_paddr) | (log2sz & 0x1F);
        self.write64(base_offset, val);
        self.read64(base_offset)
    }

    /// Programs the command queue base. Returns the accepted depth exponent.
    pub(super) fn program_cqb(&self, queue_paddr: usize, desired_log2sz: u64) -> u64 {
        let max = self.queue_log2sz_max(offset::CQB);
        let log2sz = desired_log2sz.min(max);
        let _ = self.write_queue_base(offset::CQB, queue_paddr, log2sz);
        log2sz
    }

    pub(super) fn cqh(&self) -> u32 {
        self.read32(offset::CQH)
    }

    pub(super) fn write_cqt(&self, tail: u32) {
        self.write32(offset::CQT, tail);
    }

    /// Writes the command queue CSR to enable/disable the queue, then returns
    /// the latest CSR value after polling BUSY to clear.
    /// Enables the command queue (ENABLE | interrupt-enable | clear memory-fault).
    pub(super) fn enable_cqcsr(&self) {
        self.write32(offset::CQCSR, qcsr::QUEUE_ENABLE | qcsr::QUEUE_INTR_ENABLE | qcsr::QUEUE_MEM_FAULT);
    }

    pub(super) fn set_cqcsr(&self, val: u32) {
        self.write32(offset::CQCSR, val);
    }

    pub(super) fn cqcsr(&self) -> u32 {
        self.read32(offset::CQCSR)
    }

    pub(super) fn cq_active(&self) -> bool {
        self.cqcsr() & qcsr::QUEUE_ACTIVE != 0
    }

    pub(super) fn cq_busy(&self) -> bool {
        self.cqcsr() & qcsr::QUEUE_BUSY != 0
    }

    pub(super) fn cq_error(&self) -> bool {
        let s = self.cqcsr();
        s & (qcsr::QUEUE_MEM_FAULT | qcsr::CMD_TO | qcsr::CMD_ILL) != 0
    }

    // ---- Fault Queue ----

    /// Programs the fault queue base. Returns the accepted depth exponent.
    pub(super) fn program_fqb(&self, queue_paddr: usize, desired_log2sz: u64) -> u64 {
        let max = self.queue_log2sz_max(offset::FQB);
        let log2sz = desired_log2sz.min(max);
        let _ = self.write_queue_base(offset::FQB, queue_paddr, log2sz);
        log2sz
    }

    pub(super) fn fqh(&self) -> u32 {
        self.read32(offset::FQH)
    }

    pub(super) fn write_fqt(&self, tail: u32) {
        self.write32(offset::FQT, tail);
    }

    /// Enables the fault queue (ENABLE | interrupt-enable | clear memory-fault).
    pub(super) fn enable_fqcsr(&self) {
        self.write32(offset::FQCSR, qcsr::QUEUE_ENABLE | qcsr::QUEUE_INTR_ENABLE | qcsr::QUEUE_MEM_FAULT);
    }

    pub(super) fn set_fqcsr(&self, val: u32) {
        self.write32(offset::FQCSR, val);
    }

    pub(super) fn fqcsr(&self) -> u32 {
        self.read32(offset::FQCSR)
    }

    pub(super) fn fq_active(&self) -> bool {
        self.fqcsr() & qcsr::QUEUE_ACTIVE != 0
    }

    pub(super) fn fq_busy(&self) -> bool {
        self.fqcsr() & qcsr::QUEUE_BUSY != 0
    }

    // ---- Interrupts ----

    /// Clears the interrupt pending status bits by writing them back.
    #[allow(dead_code)]
    pub(super) fn clear_ipsr(&self, bits: u32) {
        self.write32(offset::IPSR, bits);
    }
}
