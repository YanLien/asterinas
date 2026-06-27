// SPDX-License-Identifier: MPL-2.0

//! Power management via PSCI.

use crate::power::{ExitCode, inject_poweroff_handler, inject_restart_handler};

// PSCI function identifiers (32-bit calling convention).
const PSCI_VERSION: u64 = 0x8400_0000;
const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;
// PSCI function identifiers (64-bit calling convention).
const PSCI_CPU_ON_64: u64 = 0xC400_0003;
const PSCI_CPU_OFF_32: u64 = 0x8400_0002;

/// Issues a PSCI call using the conduit configured in the device tree
/// (HVC by default on QEMU `virt`).
///
/// Returns the result code from PSCI (in `x0`).
pub(super) fn psci_call(function: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let (ret, conduit_hvc): (u64, u64);
    conduit_hvc = if super::boot::psci_conduit_is_hvc() {
        1
    } else {
        0
    };

    // SAFETY: PSCI calls are safe to issue at EL1; they are handled by the
    // hypervisor/firmware. The caller is responsible for the semantics of the
    // specific PSCI function.
    unsafe {
        if conduit_hvc != 0 {
            core::arch::asm!(
                "hvc #0",
                inout("x0") function => ret,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nostack),
            );
        } else {
            core::arch::asm!(
                "smc #0",
                inout("x0") function => ret,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nostack),
            );
        }
    }
    let _ = PSCI_VERSION;
    let _ = PSCI_CPU_ON_64;
    let _ = PSCI_CPU_OFF_32;
    ret
}

fn try_poweroff(_code: ExitCode) {
    psci_call(PSCI_SYSTEM_OFF, 0, 0, 0);
}

fn try_restart(_code: ExitCode) {
    psci_call(PSCI_SYSTEM_RESET, 0, 0, 0);
}

pub(super) fn init() {
    inject_poweroff_handler(try_poweroff);
    inject_restart_handler(try_restart);
}
