// SPDX-License-Identifier: MPL-2.0

//! The AArch64 boot module defines the entrypoints of Asterinas.

pub(crate) mod smp;

use core::arch::global_asm;

use fdt::Fdt;
use spin::Once;

use crate::{
    boot::{
        BootloaderAcpiArg, BootloaderFramebufferArg,
        memory_region::{MemoryRegion, MemoryRegionArray, MemoryRegionType},
    },
    early_println,
    mm::paddr_to_vaddr,
};

global_asm!(include_str!("bsp_boot.S"));

/// The Flattened Device Tree of the platform.
pub static DEVICE_TREE: Once<Fdt> = Once::new();

fn parse_bootloader_name() -> &'static str {
    "Unknown"
}

fn parse_kernel_commandline() -> &'static str {
    DEVICE_TREE.get().unwrap().chosen().bootargs().unwrap_or("")
}

fn parse_initramfs() -> Option<&'static [u8]> {
    let (start, end) = parse_initramfs_range()?;

    let base_va = paddr_to_vaddr(start);
    let length = end - start;
    Some(unsafe { core::slice::from_raw_parts(base_va as *const u8, length) })
}

fn parse_acpi_arg() -> BootloaderAcpiArg {
    BootloaderAcpiArg::NotProvided
}

fn parse_framebuffer_info() -> Option<BootloaderFramebufferArg> {
    // TODO: Parse framebuffer info from device tree.
    None
}

fn parse_memory_regions() -> MemoryRegionArray {
    let mut regions = MemoryRegionArray::new();

    for region in DEVICE_TREE.get().unwrap().memory().regions() {
        if region.size.unwrap_or(0) > 0 {
            regions
                .push(MemoryRegion::new(
                    region.starting_address as usize,
                    region.size.unwrap(),
                    MemoryRegionType::Usable,
                ))
                .unwrap();
        }
    }

    if let Some(node) = DEVICE_TREE.get().unwrap().find_node("/reserved-memory") {
        for child in node.children() {
            if let Some(reg_iter) = child.reg() {
                for region in reg_iter {
                    regions
                        .push(MemoryRegion::new(
                            region.starting_address as usize,
                            region.size.unwrap(),
                            MemoryRegionType::Reserved,
                        ))
                        .unwrap();
                }
            }
        }
    }

    // Add the kernel region.
    regions.push(MemoryRegion::kernel()).unwrap();

    // Add the initramfs region.
    if let Some((start, end)) = parse_initramfs_range() {
        regions
            .push(MemoryRegion::new(
                start,
                end - start,
                MemoryRegionType::Module,
            ))
            .unwrap();
    }

    // Reserve the device tree so the frame allocator does not overwrite it
    // while the FDT is still in use.
    let dtb_paddr = unsafe { FDT_PADDR };
    if dtb_paddr != 0 {
        let header = paddr_to_vaddr(dtb_paddr) as *const u8;
        // The `totalsize` field is at offset 4 of the FDT header, big-endian.
        let totalsize =
            u32::from_be(unsafe { core::ptr::read_unaligned(header.add(4) as *const u32) })
                as usize;
        regions
            .push(MemoryRegion::new(dtb_paddr, totalsize, MemoryRegionType::Reserved))
            .unwrap();
    }

    regions.into_non_overlapping()
}

fn parse_initramfs_range() -> Option<(usize, usize)> {
    let chosen = DEVICE_TREE.get().unwrap().find_node("/chosen").unwrap();
    let initrd_start = chosen.property("linux,initrd-start")?.as_usize()?;
    let initrd_end = chosen.property("linux,initrd-end")?.as_usize()?;
    Some((initrd_start, initrd_end))
}

/// Reads the PSCI conduit (`hvc` vs `smc`) from the device tree.
///
/// Returns `true` if HVC should be used. Defaults to `true` for QEMU `virt`.
pub(super) fn psci_conduit_is_hvc() -> bool {
    let Some(device_tree) = DEVICE_TREE.get() else {
        return true;
    };
    if let Some(node) = device_tree.find_node("/psci") {
        if let Some(method) = node.property("method") {
            if let Some(s) = method.as_str() {
                return s == "hvc";
            }
        }
    }
    true
}

static mut BOOTSTRAP_CPU_ID: u32 = u32::MAX;
/// The physical address of the flattened device tree (from `x0`), kept so the
/// memory region parser can reserve it.
static mut FDT_PADDR: usize = 0;

/// Scans physical RAM for the FDT magic (`0xD00DFEED`) as a fallback when the
/// bootloader did not pass the device tree address in `x0`.
///
/// The scan covers the low 512 MiB of RAM, which is sufficient for the QEMU
/// `virt` machine with the default test memory size.
fn find_flattened_device_tree() -> usize {
    // The FDT magic 0xD00DFEED stored big-endian reads as 0xEDFE0DD0 on a
    // little-endian AArch64 CPU.
    const FDT_MAGIC: u32 = 0xEDFE0DD0;
    const RAM_START: usize = 0x4000_0000;
    const SCAN_LIMIT: usize = 0x6000_0000;

    let mut paddr = RAM_START;
    while paddr + 4 <= SCAN_LIMIT {
        let va = paddr_to_vaddr(paddr);
        // SAFETY: We read a 4-byte value from linear-mapped RAM.
        let val = unsafe { core::ptr::read_volatile(va as *const u32) };
        if val == FDT_MAGIC {
            return paddr;
        }
        paddr += 4;
    }
    panic!("FDT magic not found in RAM");
}


/// The entry point of the Rust code portion of Asterinas.
///
/// # Safety
///
/// - This function must be called only once at a proper timing in the BSP's
///   boot assembly code.
/// - The caller must follow C calling conventions and put the right arguments
///   in registers.
// SAFETY: The name does not collide with other symbols.
#[unsafe(no_mangle)]
unsafe extern "C" fn aarch64_boot(x0: usize, x1: usize, x2: usize, x3: usize) -> ! {
    early_println!("Enter aarch64_boot");
    early_println!("boot regs: x0={:#x} x1={:#x} x2={:#x} x3={:#x}", x0, x1, x2, x3);
    let device_tree_paddr = x0;

    // SAFETY: We don't create Rust references, so there are no aliasing
    // problems. Other processors have not been booted yet.
    unsafe { BOOTSTRAP_CPU_ID = read_mpidr_aff0() };

    let device_tree_paddr = if device_tree_paddr != 0 {
        device_tree_paddr
    } else {
        // Some QEMU `virt` boot paths do not pass the device tree address in
        // x0. Fall back to scanning RAM for the FDT magic.
        early_println!("dtb not in x0; scanning RAM for FDT magic...");
        find_flattened_device_tree()
    };
    let device_tree_ptr = paddr_to_vaddr(device_tree_paddr) as *const u8;
    let fdt = unsafe { Fdt::from_ptr(device_tree_ptr).unwrap() };
    DEVICE_TREE.call_once(|| fdt);

    // SAFETY: written once on the BSP before APs boot and before the memory
    // region parser reads it.
    unsafe { FDT_PADDR = device_tree_paddr };

    use crate::boot::{EARLY_INFO, EarlyBootInfo, start_kernel};

    EARLY_INFO.call_once(|| EarlyBootInfo {
        bootloader_name: parse_bootloader_name(),
        kernel_cmdline: parse_kernel_commandline(),
        initramfs: parse_initramfs(),
        acpi_arg: parse_acpi_arg(),
        framebuffer_arg: parse_framebuffer_info(),
        memory_regions: parse_memory_regions(),
    });

    // SAFETY: The safety is guaranteed by the safety preconditions and the fact
    // that we call it once after setting up necessary resources.
    unsafe { start_kernel() };
}

/// Reads the current CPU's MPIDR affinity field (Aff0), used as the hardware
/// CPU identifier.
fn read_mpidr_aff0() -> u32 {
    let mpidr: u64;
    // SAFETY: Reading `MPIDR_EL1` has no side effects.
    unsafe {
        core::arch::asm!(
            "mrs {0}, mpidr_el1",
            out(reg) mpidr,
            options(preserves_flags, nostack)
        );
    }
    // Aff0 is bits [7:0]. Mask off the MT bit (bit 24) and higher affinity.
    (mpidr & 0xFF) as u32
}
