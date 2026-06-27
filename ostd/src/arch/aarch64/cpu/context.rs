// SPDX-License-Identifier: MPL-2.0

//! CPU execution context control.

use alloc::boxed::Box;
use core::{arch::global_asm, fmt::Debug};

use ostd_pod::IntoBytes;

use crate::{
    arch::trap::{RawUserContext, TrapFrame, handle_irq},
    cpu::PrivilegeLevel,
    user::{ReturnReason, UserContextApi, UserContextApiInternal},
};

/// Userspace CPU context, including general-purpose registers and exception information.
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct UserContext {
    user_context: RawUserContext,
    exception: Option<CpuException>,
}

/// General registers: `x0`..`x30`.
#[expect(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GeneralRegs {
    pub regs: [usize; 31],
}

/// AArch64 CPU exceptions.
///
/// Variants that carry a fault address expose it through the associated data field.
#[derive(Clone, Copy, Debug)]
pub enum CpuException {
    /// An instruction abort (e.g. instruction page fault).
    InstructionAbort(FaultAddress),
    /// A data abort on a read (e.g. load page fault).
    DataAbortRead(FaultAddress),
    /// A data abort on a write (e.g. store page fault).
    DataAbortWrite(FaultAddress),
    /// An `SVC` instruction from EL0 (system call).
    Svc,
    /// An illegal instruction or other synchronous exception.
    IllegalInstruction,
    /// A software breakpoint.
    Breakpoint,
    /// An unknown/unhandled exception class.
    Unknown,
}

/// The fault address of an abort exception.
pub type FaultAddress = usize;

impl CpuException {
    /// Decodes a CPU exception from the exception syndrome register (`ESR_EL1`)
    /// and the fault address register (`FAR_EL1`), both captured by the trap
    /// vector.
    pub(in crate::arch) fn from_esr(esr: usize, far: usize) -> Self {
        // Exception Class is ESR bits [31:26].
        let ec = (esr >> 26) & 0x3F;
        // For data aborts, bit 6 (WnR) indicates write (1) vs read (0).
        let is_write = (esr & (1 << 6)) != 0;
        match ec {
            0x15 => Self::Svc,
            0x20 | 0x21 => Self::InstructionAbort(far),
            0x24 | 0x25 => {
                if is_write {
                    Self::DataAbortWrite(far)
                } else {
                    Self::DataAbortRead(far)
                }
            }
            0x0E | 0x1A => Self::IllegalInstruction,
            0x30 | 0x31 => Self::Breakpoint,
            _ => Self::Unknown,
        }
    }
}

impl UserContext {
    /// Returns a reference to the general registers.
    pub fn general_regs(&self) -> &GeneralRegs {
        &self.user_context.general
    }

    /// Returns a mutable reference to the general registers.
    pub fn general_regs_mut(&mut self) -> &mut GeneralRegs {
        &mut self.user_context.general
    }

    /// Takes the CPU exception out.
    pub fn take_exception(&mut self) -> Option<CpuException> {
        self.exception.take()
    }

    /// Sets the thread-local storage pointer (user `TPIDR_EL0`).
    pub fn set_tls_pointer(&mut self, tls: usize) {
        self.user_context.tpidr = tls;
    }

    /// Gets the thread-local storage pointer (user `TPIDR_EL0`).
    pub fn tls_pointer(&self) -> usize {
        self.user_context.tpidr
    }
}

impl UserContextApiInternal for UserContext {
    fn execute<F>(&mut self, mut has_kernel_event: F) -> ReturnReason
    where
        F: FnMut() -> bool,
    {
        loop {
            crate::task::scheduler::might_preempt();
            self.user_context.run();

            if self.user_context.is_irq != 0 {
                // The trap was an interrupt. Dispatch it and resume user mode.
                crate::arch::irq::enable_local();
                handle_irq(&self.as_trap_frame(), PrivilegeLevel::User);
                if has_kernel_event() {
                    break ReturnReason::KernelEvent;
                }
                continue;
            }

            let exception =
                CpuException::from_esr(self.user_context.esr, self.user_context.far);
            match exception {
                CpuException::Svc => {
                    crate::arch::irq::enable_local();
                    // Skip the `svc` instruction.
                    self.user_context.pc += 4;
                    break ReturnReason::UserSyscall;
                }
                _ => {
                    crate::arch::irq::enable_local();
                    self.exception = Some(exception);
                    break ReturnReason::UserException;
                }
            }
        }
    }

    fn as_trap_frame(&self) -> TrapFrame {
        TrapFrame {
            general: self.user_context.general,
            sp: self.user_context.sp,
            pc: self.user_context.pc,
            pstate: self.user_context.pstate,
        }
    }
}

impl UserContextApi for UserContext {
    fn trap_number(&self) -> usize {
        self.user_context.esr
    }

    fn trap_error_code(&self) -> usize {
        self.user_context.far
    }

    fn instruction_pointer(&self) -> usize {
        self.user_context.pc
    }

    fn set_instruction_pointer(&mut self, ip: usize) {
        self.user_context.pc = ip;
    }

    fn stack_pointer(&self) -> usize {
        self.user_context.sp
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.user_context.sp = sp;
    }
}

impl UserContext {
    /// Gets the value of register `xN` (`N` in `0..=30`).
    #[inline(always)]
    pub fn x(&self, idx: usize) -> usize {
        self.user_context.general.regs[idx]
    }

    /// Sets the value of register `xN` (`N` in `0..=30`).
    #[inline(always)]
    pub fn set_x(&mut self, idx: usize, val: usize) {
        self.user_context.general.regs[idx] = val;
    }
}

/// The FPU/SIMD context of a user task.
///
/// AArch64 provides mandatory FP/SIMD (`V0`..`V31`, `FPSR`, `FPCR`).
#[derive(Clone, Debug)]
pub struct FpuContext {
    inner: Box<FpuState>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod)]
pub struct FpuState {
    v: [u64; 64],
    fpsr: u32,
    fpcr: u32,
}

// FIXME: Rust does not derive `Default` for arrays larger than 32 elements yet.
// See <https://github.com/rust-lang/rust/issues/61415>.
impl Default for FpuState {
    fn default() -> Self {
        Self {
            v: [0; 64],
            fpsr: 0,
            fpcr: 0,
        }
    }
}

impl FpuContext {
    /// Creates a new (zeroed) FPU context.
    pub fn new() -> Self {
        Self {
            inner: Box::new(FpuState::default()),
        }
    }

    /// Saves the current CPU FPU state into this context.
    pub fn save(&mut self) {
        // SAFETY: The pointer is valid and properly aligned.
        unsafe { save_fpu_context(self.inner.as_mut() as *mut FpuState) };
    }

    /// Loads the CPU FPU state from this context.
    pub fn load(&self) {
        // SAFETY: The pointer is valid and properly aligned.
        unsafe { load_fpu_context(self.inner.as_ref() as *const FpuState) };
    }

    /// Returns the FPU context as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    /// Returns the FPU context as a mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        self.inner.as_mut_bytes()
    }
}

impl Default for FpuContext {
    fn default() -> Self {
        Self::new()
    }
}

global_asm!(include_str!("fpu.S"));

unsafe extern "C" {
    unsafe fn save_fpu_context(ctx: *mut FpuState);
    unsafe fn load_fpu_context(ctx: *const FpuState);
}
