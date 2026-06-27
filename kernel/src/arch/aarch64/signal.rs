// SPDX-License-Identifier: MPL-2.0

use ostd::{
    arch::cpu::context::{CpuException, UserContext},
    user::UserContextApi,
};

use crate::{
    process::signal::{SignalContext, sig_num::SigNum, signals::fault::FaultSignal},
    thread::exception::ToFaultSignal,
};

impl SignalContext for UserContext {
    fn set_arguments(&mut self, sig_num: SigNum, siginfo_addr: usize, ucontext_addr: usize) {
        self.set_x(0, sig_num.as_u8() as usize);
        self.set_x(1, siginfo_addr);
        self.set_x(2, ucontext_addr);
    }
}

impl ToFaultSignal for CpuException {
    fn to_fault_signal(&self, user_ctx: &UserContext) -> Option<FaultSignal> {
        use CpuException::*;

        use crate::process::signal::constants::*;

        let pc = user_ctx.instruction_pointer() as u64;

        let (num, code, addr) = match self {
            InstructionAbort(fault_addr) => (SIGSEGV, SEGV_MAPERR, *fault_addr as u64),
            DataAbortRead(fault_addr) | DataAbortWrite(fault_addr) => {
                (SIGSEGV, SEGV_MAPERR, *fault_addr as u64)
            }
            IllegalInstruction => (SIGILL, ILL_ILLOPC, pc),
            Breakpoint => (SIGTRAP, TRAP_BRKPT, pc),
            // `Svc` is the system call path, not a fault.
            Svc => return None,
            Unknown => (SIGILL, ILL_ILLOPC, pc),
        };

        Some(FaultSignal::new(num, code, Some(addr)))
    }
}
