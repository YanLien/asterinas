// SPDX-License-Identifier: MPL-2.0

//! The architecture support of context switch.

use crate::task::TaskContextApi;

core::arch::global_asm!(include_str!("switch.S"));

/// The kernel task context, containing the callee-saved registers and the
/// return address (used as the instruction pointer for a fresh task).
#[repr(C)]
#[derive(Clone, Debug)]
pub(crate) struct TaskContext {
    regs: CalleeRegs,
    ra: usize,
}

impl TaskContext {
    /// Creates a new `TaskContext`.
    pub(crate) const fn new() -> Self {
        TaskContext {
            regs: CalleeRegs::new(),
            ra: 0,
        }
    }
}

/// Callee-saved registers (excluding `x30`/LR, which is stored as `ra`).
#[repr(C)]
#[derive(Clone, Debug)]
struct CalleeRegs {
    sp: u64,
    x19: u64,
    x20: u64,
    x21: u64,
    x22: u64,
    x23: u64,
    x24: u64,
    x25: u64,
    x26: u64,
    x27: u64,
    x28: u64,
    x29: u64,
}

impl CalleeRegs {
    /// Creates a new `CalleeRegs`.
    pub(self) const fn new() -> Self {
        CalleeRegs {
            sp: 0,
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
        }
    }
}

impl TaskContextApi for TaskContext {
    fn set_instruction_pointer(&mut self, ip: usize) {
        // `x30` (LR) is loaded by the context switch and used by `ret`, so it
        // doubles as the entry point of a fresh task.
        self.ra = ip;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.regs.sp = sp as u64;
    }
}

unsafe extern "C" {
    pub(crate) unsafe fn context_switch(nxt: *const TaskContext, cur: *mut TaskContext);
    pub(crate) unsafe fn first_context_switch(nxt: *const TaskContext);
    pub(crate) unsafe fn kernel_task_entry_wrapper();
}
