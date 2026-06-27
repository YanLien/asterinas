// SPDX-License-Identifier: MPL-2.0

//! Handles traps.

#[expect(clippy::module_inception)]
mod trap;

use spin::Once;
pub use trap::{RawUserContext, TrapFrame};

use crate::{
    arch::{
        cpu::context::CpuException,
        irq::{enable_local, chip::dispatch_irq},
    },
    cpu::PrivilegeLevel,
    ex_table::ExTable,
    mm::MAX_USERSPACE_VADDR,
};

/// The spurious interrupt identifier returned by a GIC `IAR` read when no
/// interrupt is pending.
const GIC_SPURIOUS: u64 = 1023;

/// Initializes interrupt handling on the current CPU.
///
/// # Safety
///
/// On the current CPU, this function must be called
/// - only once and
/// - before any trap can occur.
pub(crate) unsafe fn init_on_cpu() {
    // SAFETY: The caller ensures the safety conditions.
    unsafe { trap::init_on_cpu() };
}

/// Reads and acknowledges the highest-priority pending interrupt.
///
/// Returns the INTID, or `None` if the interrupt was spurious.
fn ack_interrupt() -> Option<u32> {
    let iar: u64;
    // SAFETY: Reading `ICC_IAR1_EL1` acknowledges an interrupt but has no
    // memory-safety implications.
    unsafe {
        core::arch::asm!(
            "mrs {0}, icc_iar1_el1",
            out(reg) iar,
            options(preserves_flags, nostack),
        );
    }
    if iar == GIC_SPURIOUS {
        None
    } else {
        Some(iar as u32)
    }
}

/// Signals the end of interrupt processing for the given INTID.
fn end_of_interrupt(intid: u32) {
    // SAFETY: Writing `ICC_EOIR1_EL1` drops the priority of a previously
    // acknowledged interrupt.
    unsafe {
        core::arch::asm!(
            "msr icc_eoir1_el1, {0}",
            in(reg) intid as u64,
            options(preserves_flags, nostack),
        );
    }
}

/// Handles a synchronous exception taken from the kernel (EL1).
// SAFETY: The name does not collide with other symbols.
#[unsafe(no_mangle)]
unsafe extern "C" fn trap_handler(f: &mut TrapFrame) {
    let esr: usize;
    let far: usize;
    // SAFETY: Reading `ESR_EL1` and `FAR_EL1` has no side effects.
    unsafe {
        core::arch::asm!("mrs {0}, esr_el1", out(reg) esr, options(preserves_flags, nostack));
        core::arch::asm!("mrs {0}, far_el1", out(reg) far, options(preserves_flags, nostack));
    }
    let exception = CpuException::from_esr(esr, far);

    match exception {
        CpuException::InstructionAbort(addr)
        | CpuException::DataAbortRead(addr)
        | CpuException::DataAbortWrite(addr) => {
            if (0..MAX_USERSPACE_VADDR).contains(&addr) {
                handle_user_page_fault(f, &exception);
            } else {
                panic!(
                    "Cannot handle page fault in kernel space, exception: {:#x?}, trapframe: {:#x?}.",
                    exception, f
                );
            }
        }
        _ => {
            panic!(
                "Cannot handle kernel exception, exception: {:#x?}, trapframe: {:#x?}.",
                exception, f
            );
        }
    }
}

/// Handles an interrupt taken from the kernel (EL1).
// SAFETY: The name does not collide with other symbols.
#[unsafe(no_mangle)]
unsafe extern "C" fn irq_handler(f: &mut TrapFrame) {
    handle_irq(f, PrivilegeLevel::Kernel);
    enable_local();
}

pub(super) fn handle_irq(trap_frame: &TrapFrame, priv_level: PrivilegeLevel) {
    if let Some(intid) = ack_interrupt() {
        dispatch_irq(intid, trap_frame, priv_level);
        end_of_interrupt(intid);
    }
}

#[expect(clippy::type_complexity)]
static USER_PAGE_FAULT_HANDLER: Once<fn(&CpuException) -> Result<(), ()>> = Once::new();

/// Injects a custom handler for page faults that occur in the kernel and are
/// caused by a user-space address.
pub fn inject_user_page_fault_handler(handler: fn(&CpuException) -> Result<(), ()>) {
    USER_PAGE_FAULT_HANDLER.call_once(|| handler);
}

fn handle_user_page_fault(f: &mut TrapFrame, exception: &CpuException) {
    let handler = USER_PAGE_FAULT_HANDLER
        .get()
        .expect("Page fault handler is missing");

    let res = handler(exception);
    if res.is_ok() {
        return;
    }

    // Use the exception table to recover to normal execution.
    if let Some(addr) = ExTable::find_recovery_inst_addr(f.pc) {
        f.pc = addr;
    } else {
        panic!(
            "Failed to handle page fault, exception: {:?}, trapframe: {:#x?}.",
            exception, f
        );
    }
}
