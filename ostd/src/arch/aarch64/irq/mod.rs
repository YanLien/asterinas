// SPDX-License-Identifier: MPL-2.0

//! Interrupts.

pub(super) mod chip;
pub(super) mod ipi;
mod ops;
mod remapping;

pub use chip::{IRQ_CHIP, InterruptSourceInFdt, IrqChip, MappedIrqLine};
pub(crate) use ipi::{HwCpuId, send_ipi};
pub(crate) use ops::{
    disable_local, disable_local_and_halt, enable_local, enable_local_and_halt, is_local_enabled,
};
pub(crate) use remapping::IrqRemapping;

pub(crate) const IRQ_NUM_MIN: u8 = 0;
pub(crate) const IRQ_NUM_MAX: u8 = 255;

/// An IRQ line with the information needed to acknowledge the interrupt on
/// hardware.
///
/// The `irq_num` is the software IRQ number (the index of the allocated
/// [`IrqLine`]), not the GIC INTID. The GIC INTID is translated to this number
/// by the [`IrqChip`] when the interrupt is claimed.
pub(crate) struct HwIrqLine {
    irq_num: u8,
}

impl HwIrqLine {
    pub(super) fn new(irq_num: u8) -> Self {
        Self { irq_num }
    }

    pub(crate) fn irq_num(&self) -> u8 {
        self.irq_num
    }

    pub(crate) fn ack(&self) {
        // On AArch64 the GIC interrupt is acknowledged (IAR) and completed
        // (EOIR) centrally in the trap handler, so there is nothing to do here.
    }
}
