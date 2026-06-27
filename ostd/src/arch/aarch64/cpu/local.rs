// SPDX-License-Identifier: MPL-2.0

//! Architecture dependent CPU-local information utilities.
//!
//! The CPU-local storage base is kept in `TPIDR_EL1`. It is an EL1 register that
//! is never modified after boot on a given CPU, so it remains stable per CPU and
//! is not clobbered by user-mode execution (which uses `TPIDR_EL0`).

pub(crate) fn get_base() -> u64 {
    let base: u64;
    // SAFETY: Reading `TPIDR_EL1` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, tpidr_el1",
            out(reg) base,
            options(preserves_flags, nostack),
        );
    }
    base
}
