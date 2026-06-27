// SPDX-License-Identifier: MPL-2.0

//! RISC-V IOMMU fault/event queue.
//!
//! The fault queue is a circular buffer populated by the IOMMU hardware when a
//! translation or transaction fault occurs. Each fault record is 32 bytes
//! (4 × u64). Reference: RISC-V IOMMU Architecture Specification, ch. 3.2.

use core::hint::spin_loop;

use crate::mm::{FrameAllocOptions, HasPaddr, PAGE_SIZE, Segment, VmIo};

use super::registers::IommuRegisters;

/// Size of each fault record in bytes.
const RECORD_SIZE: usize = 32;

/// Header fields of a fault record (word0).
mod hdr {
    pub const CAUSE: u64 = 0xFFF; // [11:0]
    pub const DID: u64 = 0xFFFF_0000_0000_0000; // [63:40]
    pub const TTYP: u64 = 0xFC_0000_0000; // [39:34]
}

/// A 32-byte fault record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FaultRecord {
    pub hdr: u64,
    pub _reserved: u64,
    pub iotval: u64,
    pub iotval2: u64,
}

impl FaultRecord {
    /// Returns the fault cause code (bits [11:0] of the header).
    #[allow(dead_code)]
    pub fn cause(&self) -> u32 {
        (self.hdr & hdr::CAUSE) as u32
    }

    /// Returns the faulting device ID (bits [63:40] of the header).
    #[allow(dead_code)]
    pub fn device_id(&self) -> u16 {
        ((self.hdr & hdr::DID) >> 40) as u16
    }
}

/// The IOMMU fault queue.
pub(super) struct FaultQueue {
    #[allow(dead_code)]
    segment: Segment<()>,
    capacity: u32,
    mask: u32,
    /// Software-maintained head index; advances as records are consumed.
    head: u32,
}

impl FaultQueue {
    /// Allocates, programs and enables the fault queue.
    pub(super) fn new(regs: &IommuRegisters, desired_log2sz: u64) -> Self {
        let alloc_entries = 1usize << (desired_log2sz + 1);
        let n_pages = (alloc_entries * RECORD_SIZE).div_ceil(PAGE_SIZE).max(1);
        let segment = FrameAllocOptions::new()
            .zeroed(true)
            .alloc_segment(n_pages)
            .unwrap();

        let log2sz = regs.program_fqb(segment.paddr(), desired_log2sz);
        let capacity = 1u32 << (log2sz + 1);
        let mask = capacity - 1;

        // Reset the software-consumed head before enabling.
        regs.write_fqt(0);

        // Enable the queue.
        regs.enable_fqcsr();

        while regs.fq_busy() {
            spin_loop();
        }
        if !regs.fq_active() {
            crate::warn!("IOMMU fault queue failed to enable");
        }

        crate::debug!(
            "IOMMU fault queue enabled: {} entries at PPN {:#x}",
            capacity,
            segment.paddr() >> 12
        );

        Self {
            segment,
            capacity,
            mask,
            head: 0,
        }
    }

    /// Reads and acknowledges pending fault records.
    /// Returns the number of faults processed.
    #[allow(dead_code)]
    pub(super) fn process_faults(&mut self, regs: &IommuRegisters) -> usize {
        let mut count = 0;
        loop {
            let tail = regs.fqh();
            if (tail & self.mask) == self.head {
                break; // Queue empty (head == hardware tail means no new records)
            }

            // Read the fault record at the head offset.
            let offset = (self.head as usize) * RECORD_SIZE;
            let mut record = FaultRecord::default();
            record.hdr = self.segment.read_val(offset).unwrap();
            record._reserved = self.segment.read_val(offset + 8).unwrap();
            record.iotval = self.segment.read_val(offset + 16).unwrap();
            record.iotval2 = self.segment.read_val(offset + 24).unwrap();

            crate::warn!(
                "IOMMU fault: cause={:#x} device={:#x} iotval={:#x}",
                record.cause(),
                record.device_id(),
                record.iotval
            );

            // Advance the consumed head (acknowledge the record via FQT).
            self.head = (self.head + 1) & self.mask;
            regs.write_fqt(self.head);
            count += 1;
        }
        count
    }
}
