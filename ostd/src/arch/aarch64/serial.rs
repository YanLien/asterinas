// SPDX-License-Identifier: MPL-2.0

//! The console I/O via the PL011 UART.
//!
//! The QEMU `virt` machine exposes an ARM AMBA PL011 UART at physical address
//! 0x09000000. During early boot (before the kernel page table is activated)
//! the boot page table identity-maps this address, so the base is the physical
//! address. After [`init`] runs, the base is switched to the linear-mapped
//! virtual address so that output keeps working under the kernel page table.

use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Once;

use crate::{
    arch::io::io_mem::{read_once, write_once},
    mm::paddr_to_vaddr,
    sync::{LocalIrqDisabled, SpinLock},
};

/// The physical base address of the PL011 UART on the QEMU `virt` machine.
const PL011_PHYS: usize = 0x0900_0000;

/// The kernel linear-mapping base for a 48-bit virtual address space.
///
/// This mirrors `LINEAR_MAPPING_BASE_VADDR` in `ostd/src/mm/kspace/mod.rs`
/// (`0xFFFF_FFC0_0000_0000 << (48 - 39) == 0xFFFF_8000_0000_0000`).
const LINEAR_MAPPING_BASE: usize = 0xFFFF_8000_0000_0000;

// PL011 register offsets (byte addressed).
const UARTDR: usize = 0x000;
const UARTFR: usize = 0x018;
/// UARTFR bit 5: transmit FIFO full.
const FR_TXFF: u8 = 1 << 5;
/// UARTFR bit 4: receive FIFO empty.
const FR_RXFE: u8 = 1 << 4;

/// The primary serial port, which serves as an early console.
///
/// It is pre-initialized with the linear-mapped UART virtual address, which is
/// valid both under the boot page table (which linear-maps the low physical
/// region) and under the final kernel page table (whose linear mapping covers
/// all physical memory).
pub static SERIAL_PORT: Once<SpinLock<Pl011, LocalIrqDisabled>> =
    Once::initialized(SpinLock::new(Pl011::new()));

/// A minimal PL011 UART driver that writes bytes by polling.
pub struct Pl011 {
    base: AtomicUsize,
}

impl Pl011 {
    /// Creates a new PL011 driver backed by the linear-mapped UART base.
    const fn new() -> Self {
        Self {
            base: AtomicUsize::new(LINEAR_MAPPING_BASE + PL011_PHYS),
        }
    }
}

// SAFETY: The PL011 MMIO is accessed through volatile operations and the base
// pointer is fixed.
unsafe impl Send for Pl011 {}
unsafe impl Sync for Pl011 {}

impl fmt::Write for Pl011 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &byte in s.as_bytes() {
            self.send(byte);
        }
        Ok(())
    }
}

impl Pl011 {
    /// Sends a single byte, waiting while the transmit FIFO is full.
    pub fn send(&mut self, byte: u8) {
        let base = self.base.load(Ordering::Relaxed) as *mut u8;
        // SAFETY: `base + UARTFR` and `base + UARTDR` are valid PL011 registers.
        unsafe {
            while read_once(base.add(UARTFR)) & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            write_once(base.add(UARTDR), byte);
        }
    }

    /// Receives a single byte if one is available in the receive FIFO.
    pub fn recv(&mut self) -> Option<u8> {
        let base = self.base.load(Ordering::Relaxed) as *mut u8;
        // SAFETY: `base + UARTFR` and `base + UARTDR` are valid PL011 registers.
        unsafe {
            if read_once(base.add(UARTFR)) & FR_RXFE != 0 {
                None
            } else {
                Some(read_once(base.add(UARTDR)))
            }
        }
    }
}

/// Initializes the serial port by switching to the linear-mapped virtual base.
pub(crate) fn init() {
    let virtual_base = paddr_to_vaddr(PL011_PHYS);
    SERIAL_PORT
        .get()
        .unwrap()
        .lock()
        .base
        .store(virtual_base, Ordering::Relaxed);
}
