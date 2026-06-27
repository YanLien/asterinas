// SPDX-License-Identifier: MPL-2.0

use core::fmt;

use ostd::{
    arch::cpu::context::{CpuException, UserContext},
    cpu::PinCurrentCpu,
    task::DisabledPreemptGuard,
    user::UserContextApi,
};

use crate::{
    cpu::LinuxAbi,
    vm::{perms::VmPerms, vmar::PageFaultInfo},
};

impl LinuxAbi for UserContext {
    fn syscall_num(&self) -> usize {
        // AArch64 Linux syscall number is passed in `x8`.
        self.x(8)
    }

    fn syscall_ret(&self) -> usize {
        self.x(0)
    }

    fn set_syscall_ret(&mut self, ret: usize) {
        self.set_x(0, ret)
    }

    fn syscall_args(&self) -> [usize; 6] {
        [self.x(0), self.x(1), self.x(2), self.x(3), self.x(4), self.x(5)]
    }
}

/// Represents the context of a signal handler.
///
/// This contains the context saved before a signal handler is invoked; it will
/// be restored by `sys_rt_sigreturn`.
#[repr(C)]
#[repr(align(16))]
#[derive(Clone, Copy, Debug, Default, Pod)]
pub struct SigContext {
    /// General-purpose registers `x0`..`x30`.
    regs: [usize; 31],
    /// The stack pointer (`sp`).
    sp: usize,
    /// The program counter (`pc`).
    pc: usize,
    /// The saved processor state (`pstate`).
    pstate: usize,
    /// The fault address captured by the trap.
    fault_addr: usize,
    /// Padding so that the struct size (288 bytes) is a multiple of its
    /// 16-byte alignment, leaving no internal padding for `Pod`.
    __pad: usize,
}

impl SigContext {
    /// Copies the saved general registers into a [`UserContext`].
    pub fn copy_user_regs_to(&self, dst: &mut UserContext) {
        dst.general_regs_mut().regs = self.regs;
        dst.set_stack_pointer(self.sp);
        dst.set_instruction_pointer(self.pc);
    }

    /// Copies the general registers from a [`UserContext`] into `self`.
    pub fn copy_user_regs_from(&mut self, src: &UserContext) {
        self.regs = src.general_regs().regs;
        self.sp = src.stack_pointer();
        self.pc = src.instruction_pointer();
    }
}

impl TryFrom<&CpuException> for PageFaultInfo {
    // [`Err`] indicates that the [`CpuException`] is not a page fault, with no
    // additional error information.
    type Error = ();

    fn try_from(value: &CpuException) -> Result<Self, ()> {
        use CpuException::*;

        let (fault_addr, required_perms) = match value {
            InstructionAbort(addr) => (addr, VmPerms::EXEC),
            DataAbortRead(addr) => (addr, VmPerms::READ),
            // On AArch64, writable mappings are also readable.
            DataAbortWrite(addr) => (addr, VmPerms::READ | VmPerms::WRITE),
            _ => return Err(()),
        };

        Ok(PageFaultInfo::new(*fault_addr, required_perms))
    }
}

/// CPU information to be shown in `/proc/cpuinfo`.
pub struct CpuInformation {
    processor: u32,
}

impl CpuInformation {
    /// Constructs the information for the current CPU.
    pub fn new(guard: &DisabledPreemptGuard) -> Self {
        Self {
            processor: guard.current_cpu().into(),
        }
    }
}

impl fmt::Display for CpuInformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "processor\t: {}", self.processor)
    }
}
