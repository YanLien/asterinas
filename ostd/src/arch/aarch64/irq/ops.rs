// SPDX-License-Identifier: MPL-2.0

//! Interrupt operations.

/// The IRQ/fiq mask bits in `PSTATE` (`DAIF`), shifted to their field position.
/// Bit 7 = IRQ, bit 6 = FIQ.
const DAIF_IRQ_FIQ: usize = 0b11 << 6;

// FIXME: Mark these as unsafe. See
// <https://github.com/asterinas/asterinas/issues/1120#issuecomment-2748696592>.
pub(crate) fn enable_local() {
    // SAFETY: Clearing the IRQ mask in `PSTATE` only enables interrupts, which
    // the caller is responsible for being safe to do.
    unsafe {
        core::arch::asm!(
            "msr daifclr, #0xf",
            options(preserves_flags, nostack),
        );
    }
}

pub(crate) fn disable_local() {
    // SAFETY: Setting the IRQ mask in `PSTATE` only disables interrupts.
    unsafe {
        core::arch::asm!(
            "msr daifset, #0xf",
            options(preserves_flags, nostack),
        );
    }
}

/// Enables local IRQs and halts the CPU to wait for interrupts.
///
/// This method guarantees that no interrupts can occur in the middle.
pub(crate) fn enable_local_and_halt() {
    // SAFETY: `wfi` is safe even with interrupts disabled. Enabling interrupts
    // afterwards is the caller's responsibility.
    unsafe {
        core::arch::asm!("wfi", options(preserves_flags, nostack));
    }
    enable_local();
}

/// Disables local IRQs and halts the CPU forever.
pub(crate) fn disable_local_and_halt() -> ! {
    disable_local();
    loop {
        // SAFETY: `wfi` is always safe.
        unsafe {
            core::arch::asm!("wfi", options(preserves_flags, nostack));
        }
    }
}

pub(crate) fn is_local_enabled() -> bool {
    let daif: usize;
    // SAFETY: Reading `DAIF` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, daif",
            out(reg) daif,
            options(preserves_flags, nostack),
        );
    }
    (daif & DAIF_IRQ_FIQ) == 0
}
