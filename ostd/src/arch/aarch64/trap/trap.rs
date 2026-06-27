// SPDX-License-Identifier: MPL-2.0

//! Trap frame definitions and the per-CPU trap vector.

use core::arch::{asm, global_asm};

use crate::arch::cpu::context::GeneralRegs;

global_asm!(include_str!("trap.S"));

/// The pointer to the [`RawUserContext`] currently being executed in user mode
/// on this CPU.
///
/// This is written by `run_user` before entering EL0 and read by the EL0 trap
/// vector to save the user registers back into the context.
///
/// NOTE: This is a single global for simplicity. For full SMP correctness it
/// should become per-CPU (e.g., via a CPU-local cell accessed through
/// `TPIDR_EL1`).
// SAFETY: The name does not collide with other symbols.
#[unsafe(no_mangle)]
static mut CURRENT_USER_CONTEXT: usize = 0;

/// Initializes interrupt handling for the current CPU.
///
/// This function will set `VBAR_EL1` to the internal exception vector table.
///
/// # Safety
///
/// On the current CPU, this function must be called
/// - only once and
/// - before any trap can occur.
pub(super) unsafe fn init_on_cpu() {
    // SAFETY: We believe that these assembly instructions correctly set up the
    // trap handling for the current CPU without side effects.
    unsafe {
        asm!(
            "msr vbar_el1, {0}",
            "isb",
            in(reg) exception_vector as *const () as usize,
            options(preserves_flags, nostack),
        );
    }
}

/// Trap frame of a kernel interrupt or exception.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TrapFrame {
    /// General registers `x0`..`x30`.
    pub general: GeneralRegs,
    /// The stack pointer at the time of the trap.
    pub sp: usize,
    /// The exception link register (`ELR_EL1`), i.e. the faulting PC.
    pub pc: usize,
    /// The saved processor state (`SPSR_EL1`).
    pub pstate: usize,
}

/// Saved registers and exception metadata for a user-mode execution context.
///
/// The layout is part of the ABI with `trap.S` (the `run_user` and the EL0
/// trap vector). Update the assembly if the layout changes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RawUserContext {
    /// General registers `x0`..`x30` (248 bytes).
    pub general: GeneralRegs,
    /// The user stack pointer (`SP_EL0`).
    pub sp: usize,
    /// The user program counter (`ELR_EL1`).
    pub pc: usize,
    /// The saved processor state (`SPSR_EL1`).
    pub pstate: usize,
    /// The user thread pointer / TLS register (`TPIDR_EL0`).
    pub tpidr: usize,
    /// The exception syndrome (`ESR_EL1`), valid for synchronous exceptions.
    pub esr: usize,
    /// The fault address (`FAR_EL1`).
    pub far: usize,
    /// Non-zero if the last trap was an interrupt rather than a sync exception.
    pub is_irq: usize,
}

impl Default for RawUserContext {
    fn default() -> Self {
        Self {
            general: GeneralRegs::default(),
            sp: 0,
            pc: 0,
            // EL0t (mode 0) with IRQ/FIQ enabled (DAIF clear).
            pstate: 0,
            tpidr: 0,
            esr: 0,
            far: 0,
            is_irq: 0,
        }
    }
}

impl RawUserContext {
    /// Goes to user space with the context, and comes back when a trap occurs.
    ///
    /// On return, the context will be reset to the state captured at the trap.
    pub(in crate::arch) fn run(&mut self) {
        let guard = crate::irq::disable_local();

        crate::task::call_pre_user_run_handler(&guard);

        // Return to userspace with interrupts disabled. Otherwise, interrupts
        // after arming the return path will mess up the CPU state.
        core::mem::forget(guard);

        // SAFETY: `run_user` sets up EL0 entry from this context and returns
        // when an EL0 trap occurs.
        unsafe { run_user(self) };
    }
}

unsafe extern "C" {
    unsafe fn exception_vector();
    unsafe fn run_user(regs: &mut RawUserContext);
}
