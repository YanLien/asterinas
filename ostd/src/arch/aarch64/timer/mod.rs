// SPDX-License-Identifier: MPL-2.0

//! The ARM generic timer support.

use core::sync::atomic::{AtomicU64, Ordering};

use spin::Once;

use crate::{
    arch::trap::TrapFrame,
    irq::IrqLine,
    timer::TIMER_FREQ,
};

/// The INTID of the virtual timer (a private PPI: `16 + 11 = 27`).
const VIRTUAL_TIMER_INTID: u32 = 27;

pub(super) static TIMER_IRQ: Once<IrqLine> = Once::new();

static TIMEBASE_FREQ: AtomicU64 = AtomicU64::new(0);
static TIMER_INTERVAL: AtomicU64 = AtomicU64::new(0);

/// Initializes the timer module on the BSP.
///
/// # Safety
///
/// This function is safe to call on the following conditions:
///  1. It is called once and at most once at a proper timing in the boot context
///     of the BSP.
///  2. It is called before any other public function of this module is called.
pub(super) unsafe fn init_on_bsp() {
    let freq = read_cntfrq();
    TIMEBASE_FREQ.store(freq, Ordering::Relaxed);
    TIMER_INTERVAL.store(freq / TIMER_FREQ, Ordering::Relaxed);

    TIMER_IRQ.call_once(|| {
        let mut timer_irq = IrqLine::alloc().unwrap();
        timer_irq.on_active(timer_callback);
        timer_irq
    });

    let irq_num = TIMER_IRQ.get().unwrap().num();
    // SAFETY: register the timer PPI with the GIC during BSP init.
    unsafe { crate::arch::irq::chip::register_and_enable(VIRTUAL_TIMER_INTID, irq_num) };

    // SAFETY: called once on the BSP.
    unsafe { init_current_cpu() };
}

/// Initializes the timer on this AP.
///
/// # Safety
///
/// This function must be called on an AP that hasn't called this function.
pub(super) unsafe fn init_on_ap() {
    // SAFETY: called once on this AP.
    unsafe { init_current_cpu() };
}

/// Initializes the timer on the current CPU.
///
/// # Safety
///
/// This function must be called on a CPU that hasn't called this function.
unsafe fn init_current_cpu() {
    set_next_timer();
    // Enable the virtual timer: unmask and enable. IMASK (bit 1) = 0, ENABLE (bit 0) = 1.
    // SAFETY: writing `CNTV_CTL_EL0` is safe here.
    unsafe {
        core::arch::asm!(
            "msr cntv_ctl_el0, {0}",
            in(reg) 0x1u64,
            options(preserves_flags, nostack)
        );
        core::arch::asm!("isb", options(preserves_flags, nostack));
    }
}

fn timer_callback(trapframe: &TrapFrame) {
    crate::timer::call_timer_callback_functions(trapframe);
    set_next_timer();
}

fn set_next_timer() {
    let interval = TIMER_INTERVAL.load(Ordering::Relaxed);
    // SAFETY: writing `CNTV_TVAL_EL0` schedules the next timer interrupt.
    unsafe {
        core::arch::asm!(
            "msr cntv_tval_el0, {0}",
            in(reg) interval,
            options(preserves_flags, nostack)
        );
    }
}

fn read_cntfrq() -> u64 {
    let freq: u64;
    // SAFETY: Reading `CNTFRQ_EL0` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, cntfrq_el0",
            out(reg) freq,
            options(preserves_flags, nostack)
        );
    }
    freq
}

/// Reads the current virtual-count value of the system counter.
pub(crate) fn read_count() -> u64 {
    let count: u64;
    // SAFETY: Reading `CNTVCT_EL0` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, cntvct_el0",
            out(reg) count,
            options(preserves_flags, nostack)
        );
    }
    count
}

pub(crate) fn get_timebase_freq() -> u64 {
    TIMEBASE_FREQ.load(Ordering::Relaxed)
}
