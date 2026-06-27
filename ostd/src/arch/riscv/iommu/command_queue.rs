// SPDX-License-Identifier: MPL-2.0

//! RISC-V IOMMU command queue.
//!
//! The command queue is a circular buffer used to submit IOTLB invalidation,
//! device-directory invalidation, and fence (sync) commands to the IOMMU.
//!
//! Reference: RISC-V IOMMU Architecture Specification, ch. 3.1.

use core::hint::spin_loop;

use crate::mm::{FrameAllocOptions, HasPaddr, PAGE_SIZE, Segment, VmIo};

use super::registers::IommuRegisters;

/// Size of each command descriptor in bytes (2 × u64).
const CMD_SIZE: usize = 16;

/// Command opcodes (OPCODE occupies dword0 bits [6:0], FUNC bits [9:7]).
mod opcode {
    pub const IOTINVAL: u64 = 1; // IOTLB invalidation
    pub const IOFENCE: u64 = 2; // Command-queue fence / sync
    pub const IODIR: u64 = 3; // Device-directory invalidation
}

/// IOTINVAL functions.
mod iotinv_func {
    pub const VMA: u64 = 0; // Invalidate first-stage VMA entries
    pub const GVMA: u64 = 1; // Invalidate second-stage entries
}

/// A 16-byte (2 × u64) command descriptor.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Command {
    pub dword0: u64,
    pub dword1: u64,
}

impl Command {
    /// IOTLB invalidation of all first-stage entries for the given PSCID.
    /// IOTINVAL.VMA with AV=0, GV=0, PSCV=1, PSCID=pscid.
    pub fn iotlb_inval_pscid(pscid: u64) -> Self {
        // PSCV = bit 32, PSCID field = bits [31:12].
        let dword0 = (opcode::IOTINVAL)
            | (iotinv_func::VMA << 7)
            | ((pscid & 0xFFFFF) << 12)
            | (1u64 << 32);
        Self {
            dword0,
            dword1: 0,
        }
    }

    /// Device-directory invalidation of all device contexts.
    /// IODIR.INVAL_DDT with DV=0 (all device IDs).
    pub fn ddt_inval_all() -> Self {
        Self {
            dword0: opcode::IODIR | (0 << 7), // FUNC_INVAL_DDT = 0
            dword1: 0,
        }
    }

    /// A command-queue fence. The IOMMU completes this command only after all
    /// previously submitted commands have been executed.
    /// IOFENCE.C with AV=0 (no poll address, no interrupt).
    pub fn iofence() -> Self {
        Self {
            dword0: opcode::IOFENCE | (0 << 7), // FUNC_C = 0
            dword1: 0,
        }
    }
}

/// The IOMMU command queue.
pub(super) struct CommandQueue {
    /// Allocated memory segment for the queue.
    segment: Segment<()>,
    /// Number of entries (power of two).
    capacity: u32,
    /// Mask for index wrapping (`capacity - 1`).
    mask: u32,
    /// Current tail index (software-maintained).
    tail: u32,
}

impl CommandQueue {
    /// Allocates, programs and enables the command queue.
    ///
    /// `desired_log2sz` is the requested depth exponent (entries =
    /// 2^(log2sz+1)); the hardware may report a smaller supported maximum.
    pub(super) fn new(regs: &IommuRegisters, desired_log2sz: u64) -> Self {
        // Allocate memory for the largest depth we would accept.
        let alloc_entries = 1usize << (desired_log2sz + 1);
        let n_pages = (alloc_entries * CMD_SIZE).div_ceil(PAGE_SIZE).max(1);
        let segment = FrameAllocOptions::new()
            .zeroed(true)
            .alloc_segment(n_pages)
            .unwrap();

        // Program CQB. The hardware may accept a smaller depth exponent.
        let log2sz = regs.program_cqb(segment.paddr(), desired_log2sz);
        let capacity = 1u32 << (log2sz + 1);
        let mask = capacity - 1;

        // Reset the tail before enabling the queue.
        regs.write_cqt(0);

        // Enable the queue.
        regs.enable_cqcsr();

        // Poll BUSY to clear, then require ACTIVE with no error.
        while regs.cq_busy() {
            spin_loop();
        }
        if !regs.cq_active() || regs.cq_error() {
            panic!(
                "IOMMU command queue failed to enable (active={}, error={})",
                regs.cq_active(),
                regs.cq_error()
            );
        }

        crate::debug!(
            "IOMMU command queue enabled: {} entries at PPN {:#x}",
            capacity,
            segment.paddr() >> 12
        );

        Self {
            segment,
            capacity,
            mask,
            tail: 0,
        }
    }

    /// Submits a command to the queue and advances the tail, blocking until
    /// there is free space.
    pub(super) fn submit(&mut self, regs: &IommuRegisters, cmd: Command) {
        loop {
            let head = regs.cqh();
            let next_tail = (self.tail + 1) & self.mask;
            if next_tail == (head & self.mask) {
                // Queue is full; wait for the hardware to drain.
                spin_loop();
                continue;
            }
            break;
        }

        // Write the command at the current tail offset.
        let offset = (self.tail as usize) * CMD_SIZE;
        self.segment.write_val(offset, &cmd.dword0).unwrap();
        self.segment.write_val(offset + 8, &cmd.dword1).unwrap();

        // Advance the tail (doorbell).
        self.tail = (self.tail + 1) & self.mask;
        regs.write_cqt(self.tail);
    }

    /// Submits an IOFENCE.C command and spins until the hardware head advances
    /// past it, ensuring all previously submitted commands have completed.
    pub(super) fn drain(&mut self, regs: &IommuRegisters) {
        self.submit(regs, Command::iofence());
        // The tail has advanced past the fence; wait for the head to catch up.
        // For a single-entry wait, the head must equal the tail.
        loop {
            if (regs.cqh() & self.mask) == self.tail {
                break;
            }
            spin_loop();
        }
    }
}
