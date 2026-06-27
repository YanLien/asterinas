// SPDX-License-Identifier: MPL-2.0

//! Inter-processor interrupts.

use spin::Once;

use crate::{cpu::PinCurrentCpu, irq::IrqLine};

/// The SGI INTID used for inter-processor interrupts.
///
/// Software Generated Interrupts (INTID 0..15) are per-CPU and used for IPIs.
pub(in crate::arch) const IPI_INTID: u32 = 0;

/// Hardware-specific, architecture-dependent CPU ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HwCpuId(u32);

impl HwCpuId {
    pub(crate) fn read_current(_guard: &dyn PinCurrentCpu) -> Self {
        // No races because of `_guard`.
        Self(crate::arch::boot::smp::get_current_cpu_id())
    }
}

pub(in crate::arch) static IPI_IRQ: Once<IrqLine> = Once::new();

/// Initializes the global IPI-related state and local state on the BSP.
///
/// # Safety
///
/// This function can only be called on the BSP and before any other
/// IPI-related function is called.
pub(in crate::arch) unsafe fn init_on_bsp() {
    let mut irq = IrqLine::alloc().unwrap();
    // SAFETY: This will be called upon an inter-processor interrupt.
    irq.on_active(|f| unsafe { crate::smp::do_inter_processor_call(f) });
    IPI_IRQ.call_once(|| irq);

    let irq_num = IPI_IRQ.get().unwrap().num();
    // SAFETY: This is called once during BSP init before any IPI can occur.
    unsafe {
        super::chip::register_and_enable(IPI_INTID, irq_num);
    }
}

/// Initializes the IPI-related state on this AP.
///
/// # Safety
///
/// This function can only be called before any other CPUs can send IPIs to
/// this application CPU.
pub(in crate::arch) unsafe fn init_on_ap() {
    let irq_num = IPI_IRQ.get().unwrap().num();
    // SAFETY: This is called once during AP init.
    unsafe {
        super::chip::register_and_enable(IPI_INTID, irq_num);
    }
}

/// Sends a general inter-processor interrupt (IPI) to the specified CPU.
pub(crate) fn send_ipi(hw_cpu_id: HwCpuId, _guard: &dyn PinCurrentCpu) {
    // Build the ICC_SGI1R_EL1 value to target a single CPU by its Aff0.
    // For QEMU `virt` all CPUs share Aff1=Aff2=Aff3=0, so the target list
    // (bits [15:0]) selects the destination directly.
    let target_bit = 1u64 << (hw_cpu_id.0 as u64);
    // SAFETY: Writing `ICC_SGI1R_EL1` generates an SGI to the target CPU.
    unsafe {
        core::arch::asm!(
            "msr icc_sgi1r_el1, {0}",
            in(reg) target_bit,
            options(preserves_flags, nostack),
        );
    }
}
