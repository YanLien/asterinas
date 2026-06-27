// SPDX-License-Identifier: MPL-2.0

//! Multiprocessor Boot Support.

use core::arch::global_asm;

use crate::{
    boot::smp::{PerApRawInfo, ap_early_entry},
    mm::{Paddr, Vaddr},
};

// Include the AP boot assembly code.
global_asm!(include_str!("ap_boot.S"));

/// The 64-bit PSCI `CPU_ON` function identifier.
const PSCI_CPU_ON_64: u64 = 0xC400_0003;

/// The offset from a physical (load) address to the kernel virtual address.
/// Equals `KERNEL_VMA - KERNEL_LMA` from the linker script.
const KERNEL_VMA_OFFSET: Vaddr = 0xFFFF_FFFF_0000_0000;

pub(crate) fn count_processors() -> Option<u32> {
    let mut count = 0;
    for_each_cpu_id(|_| count += 1);
    if count == 0 {
        None
    } else {
        Some(count)
    }
}

/// Brings up all application processors.
///
/// # Safety
///
/// The caller must ensure that
///  1. we're in the boot context of the BSP,
///  2. all APs have not yet been booted, and
///  3. the arguments are valid to boot APs.
pub(crate) unsafe fn bringup_all_aps(info_ptr: *const PerApRawInfo, pt_ptr: Paddr, num_cpus: u32) {
    if num_cpus <= 1 {
        return;
    }

    // SAFETY: The variables are defined in the AP boot assembly and are
    // written here exclusively during BSP bring-up.
    unsafe {
        fill_boot_info_ptr(info_ptr);
        fill_boot_page_table_ptr(pt_ptr);
    }

    let bsp_id = get_bootstrap_cpu_id();
    crate::info!("Bootstrapping CPU is {}, booting all other CPUs", bsp_id);

    let mut next_cpu_id = 1u32;
    for_each_cpu_id(|mpidr| {
        if mpidr != bsp_id {
            // SAFETY: each MPIDR is iterated once, so each AP is booted once.
            unsafe { bringup_ap(mpidr, next_cpu_id) };
            next_cpu_id += 1;
        }
    });
}

/// # Safety
///
/// The caller must ensure the resources for the AP's boot (stack and page
/// table) are set up and that this CPU hasn't booted yet.
unsafe fn bringup_ap(mpidr: u32, cpu_id: u32) {
    crate::info!("Starting CPU {} (mpidr={})", cpu_id, mpidr);

    let entry = get_ap_boot_start_addr();
    let result = psci_call(PSCI_CPU_ON_64, mpidr as u64, entry as u64, cpu_id as u64);

    if result == 0 {
        crate::debug!("Successfully started CPU {}", cpu_id);
    } else {
        crate::error!(
            "Failed to start CPU {}: PSCI error {:#x}",
            cpu_id,
            result
        );
    }
}

/// Issues a PSCI call using the conduit configured in the device tree.
fn psci_call(function: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    // SAFETY: PSCI calls are handled by the hypervisor/firmware and are safe to
    // issue at EL1.
    unsafe {
        if super::psci_conduit_is_hvc() {
            core::arch::asm!(
                "hvc #0",
                inout("x0") function => ret,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nostack)
            );
        } else {
            core::arch::asm!(
                "smc #0",
                inout("x0") function => ret,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nostack)
            );
        }
    }
    ret
}

/// Fills the AP boot info array pointer.
///
/// # Safety
///
/// This writes to the static mutable variable `__ap_boot_info_array_pointer`.
/// The caller must ensure exclusive access.
unsafe fn fill_boot_info_ptr(info_ptr: *const PerApRawInfo) {
    unsafe extern "C" {
        static mut __ap_boot_info_array_pointer: *const PerApRawInfo;
    }
    // SAFETY: upheld by the caller.
    unsafe {
        __ap_boot_info_array_pointer = info_ptr;
    }
}

/// Fills the AP boot page table pointer.
///
/// # Safety
///
/// This writes to the static mutable variable `__ap_boot_page_table_pointer`.
/// The caller must ensure exclusive access.
unsafe fn fill_boot_page_table_ptr(pt_ptr: Paddr) {
    unsafe extern "C" {
        static mut __ap_boot_page_table_pointer: Paddr;
    }
    // SAFETY: upheld by the caller.
    unsafe {
        __ap_boot_page_table_pointer = pt_ptr;
    }
}

/// Returns the physical address of `ap_boot_start`.
fn get_ap_boot_start_addr() -> Paddr {
    // `ap_boot_start` is linked at its physical address (the `.ap_boot` section
    // is placed at the physical load address), so taking its address yields the
    // physical entry point directly.
    unsafe extern "C" {
        static ap_boot_start: u8;
    }
    // SAFETY: We only read the symbol's link-time address.
    core::ptr::addr_of!(ap_boot_start) as usize
}

fn for_each_cpu_id(mut f: impl FnMut(u32)) {
    let Some(device_tree) = super::DEVICE_TREE.get() else {
        f(get_bootstrap_cpu_id());
        return;
    };
    device_tree.cpus().for_each(|cpu_node| {
        if let Some(device_type) = cpu_node.property("device_type") {
            if device_type.as_str() == Some("cpu")
                && let Some(reg) = cpu_node.property("reg")
            {
                f(reg.as_usize().unwrap() as u32);
            }
        }
    });
}

fn get_bootstrap_cpu_id() -> u32 {
    // SAFETY: `BOOTSTRAP_CPU_ID` is written once in `aarch64_boot` before any
    // AP reads it.
    unsafe { super::BOOTSTRAP_CPU_ID }
}

/// Returns the hardware CPU id (MPIDR Aff0) of the current CPU.
pub(in crate::arch) fn get_current_cpu_id() -> u32 {
    let mpidr: u64;
    // SAFETY: Reading `MPIDR_EL1` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, mpidr_el1",
            out(reg) mpidr,
            options(preserves_flags, nostack)
        );
    }
    (mpidr & 0xFF) as u32
}

/// The entry point of the Rust code portion for an application processor.
///
/// # Safety
///
/// - This function must be called only once on each AP at a proper timing in
///   the AP's boot assembly code.
// SAFETY: The name does not collide with other symbols.
#[unsafe(no_mangle)]
unsafe extern "C" fn aarch64_ap_early_entry(cpu_id: u32) -> ! {
    // SAFETY: This is valid to call because the caller is the AP's boot assembly
    // code, which guarantees it is called once per AP with the correct cpu_id.
    unsafe { ap_early_entry(cpu_id) };
}
