// SPDX-License-Identifier: MPL-2.0

//! The RISC-V IOMMU support.
//!
//! Implements the RISC-V IOMMU Architecture Specification, providing DMA
//! remapping via first-stage (IOSATP) page tables shared by all devices, a
//! command queue for IOTLB invalidation, and a fault queue for error reporting.
//!
//! When no IOMMU hardware is present (or it cannot be enabled), all operations
//! return [`IommuError::NoIommu`] and DMA falls back to identity mapping.

mod command_queue;
mod discovery;
mod fault_queue;
mod page_table;
mod registers;

use core::hint::spin_loop;
use core::sync::atomic::{AtomicBool, Ordering};

use spin::Once;

use crate::{
    io::IoMemAllocatorBuilder,
    mm::{
        CachePolicy, Daddr, Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, Paddr, PageFlags,
        PageProperty, PageTable, PrivilegedPageFlags, VmIo, page_table::PageTableError,
    },
    sync::{LocalIrqDisabled, SpinLock},
    task::disable_preempt,
};

use command_queue::CommandQueue;
use fault_queue::FaultQueue;
use registers::IommuRegisters;

/// Re-exported for the DMA allocator in `mm/dma/util.rs`.
pub(crate) use page_table::IommuPtConfig;
pub(super) use registers::{atp_ppn, ddtp_mode};

/// Requested command queue depth exponent (entries = 2^(N+1)).
const CMD_QUEUE_LOG2SZ: u64 = 6; // 128 entries
/// Requested fault queue depth exponent.
const FQ_QUEUE_LOG2SZ: u64 = 5; // 64 entries

/// The single PSCID used for the shared first-stage domain.
const PSCID: u64 = 1;

/// IOSATP mode for first-stage Sv48 translation.
const IOSATP_MODE_SV48: u64 = 9;
/// Device-context Translation Control: valid bit.
const DC_TC_V: u64 = 1 << 0;

/// Singleton that holds the initialized IOMMU state.
static IOMMU: Once<RiscvIommu> = Once::new();

/// Whether DMA remapping was enabled.
static DMA_REMAPPING_ENABLED: AtomicBool = AtomicBool::new(false);

struct RiscvIommu {
    registers: IommuRegisters,
    cmd_queue: SpinLock<CommandQueue, LocalIrqDisabled>,
    page_table: SpinLock<PageTable<IommuPtConfig>, LocalIrqDisabled>,
    #[allow(dead_code)]
    fault_queue: FaultQueue,
    pscid: u64,
    /// Keeps the Device Directory Table root page alive.
    _ddt_frame: Frame<()>,
}

/// An enumeration representing possible errors related to IOMMU.
#[derive(Debug)]
pub(crate) enum IommuError {
    /// No IOMMU is available.
    NoIommu,
    /// Error encountered during modification of the page table.
    ModificationError(PageTableError),
}

/// Initializes the RISC-V IOMMU.
///
/// Discovers the IOMMU via the device tree, initializes its queues and device
/// directory table, and enables DMA remapping.
///
/// If no IOMMU is present, or it cannot be enabled, returns `Ok(())` and DMA
/// uses identity mapping.
pub(crate) fn init(io_mem_builder: &IoMemAllocatorBuilder) -> Result<(), IommuError> {
    // Try to discover the IOMMU in the device tree.
    let Some(iommu_info) = discovery::discover() else {
        return Ok(());
    };

    // Reserve the IOMMU register MMIO region.
    let reg_base = iommu_info.reg_range.start;
    let iommu_mem = io_mem_builder.reserve(iommu_info.reg_range, CachePolicy::Uncacheable);
    let regs = IommuRegisters::new(iommu_mem);

    let (major, minor) = regs.version();
    crate::info!(
        "RISC-V IOMMU v{}.{} at MMIO {:#x}",
        major,
        minor,
        reg_base
    );

    // First-stage Sv48 translation is required for DMA remapping.
    if !regs.supports_sv48() {
        crate::warn!("RISC-V IOMMU lacks Sv48 support; DMA remapping disabled");
        return Ok(());
    }

    // Step 1: Enable the command and fault queues.
    let mut cmd_queue = CommandQueue::new(&regs, CMD_QUEUE_LOG2SZ);
    let fault_queue = FaultQueue::new(&regs, FQ_QUEUE_LOG2SZ);

    // Step 2: Create the shared first-stage (IOSATP Sv48) page table. All
    // devices are attached to this single page table, mirroring the x86
    // approach where every PCI device shares one second-stage table.
    let page_table = PageTable::<IommuPtConfig>::empty();
    let root_paddr = page_table.root_paddr();

    // Step 3: Allocate the 1-level Device Directory Table and program every
    // device context to use the shared page table.
    let dc_size = if regs.extended_dc_format() { 64 } else { 32 };
    let n_entries = PAGE_SIZE / dc_size;
    let ddt_frame = FrameAllocOptions::new()
        .zeroed(true)
        .alloc_frame()
        .unwrap();

    let fsc = (IOSATP_MODE_SV48 << 60) | atp_ppn(root_paddr);
    let ta = PSCID << 12;
    let bare_iohgatp: u64 = 0;
    for i in 0..n_entries {
        let off = i * dc_size;
        ddt_frame.write_val::<u64>(off, &DC_TC_V).unwrap(); // tc
        ddt_frame.write_val::<u64>(off + 8, &bare_iohgatp).unwrap(); // iohgatp (BARE)
        ddt_frame.write_val::<u64>(off + 16, &ta).unwrap(); // ta
        ddt_frame.write_val::<u64>(off + 24, &fsc).unwrap(); // fsc (IOSATP)
    }
    let ddt_paddr = ddt_frame.paddr();

    // Step 4: Program DDTP to 1-level mode. WARL may downgrade the mode.
    regs.write_ddtp(ddtp_mode::LVL1, ddt_paddr);
    while regs.ddtp_busy() {
        spin_loop();
    }
    let accepted_mode = regs.ddtp_mode();
    if accepted_mode < ddtp_mode::LVL1 {
        crate::warn!(
            "IOMMU DDTP rejected 1LVL mode (accepted {accepted_mode}); DMA remapping disabled"
        );
        return Ok(());
    }

    // Step 5: Issue initial invalidations and wait for them to complete.
    cmd_queue.submit(&regs, command_queue::Command::ddt_inval_all());
    cmd_queue.submit(
        &regs,
        command_queue::Command::iotlb_inval_pscid(PSCID),
    );
    cmd_queue.drain(&regs);

    // Step 6: Enable DMA remapping.
    DMA_REMAPPING_ENABLED.store(true, Ordering::SeqCst);
    crate::info!("RISC-V IOMMU DMA remapping enabled (1LVL DDT, Sv48 first-stage)");

    IOMMU.call_once(|| RiscvIommu {
        registers: regs,
        cmd_queue: SpinLock::new(cmd_queue),
        page_table: SpinLock::new(page_table),
        fault_queue,
        pscid: PSCID,
        _ddt_frame: ddt_frame,
    });

    Ok(())
}

/// Maps a device address to a physical address in the shared first-stage page
/// table.
///
/// # Safety
///
/// Mapping an incorrect address may lead to a kernel data leak or DMA to
/// unintended memory.
pub(crate) unsafe fn map(daddr: Daddr, paddr: Paddr) -> Result<(), IommuError> {
    let Some(iommu) = IOMMU.get() else {
        return Err(IommuError::NoIommu);
    };

    // The DMA allocator calls this with local IRQs disabled.
    let preempt_guard = disable_preempt();
    {
        let pt = iommu.page_table.lock();
        let mut cursor = pt
            .cursor_mut(&preempt_guard, &(daddr..daddr + PAGE_SIZE))
            .map_err(IommuError::ModificationError)?;
        let prop = PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Uncacheable,
            priv_flags: PrivilegedPageFlags::empty(),
        };
        // SAFETY: The caller guarantees `paddr` is untyped DMA memory.
        unsafe { cursor.map((paddr, 1, prop)) };
    }

    // Flush the IOTLB for the shared domain and wait for completion.
    let mut cq = iommu.cmd_queue.lock();
    cq.submit(&iommu.registers, command_queue::Command::iotlb_inval_pscid(iommu.pscid));
    cq.drain(&iommu.registers);

    Ok(())
}

/// Unmaps a device address from the shared first-stage page table.
pub(crate) fn unmap(daddr: Daddr) -> Result<(), IommuError> {
    let Some(iommu) = IOMMU.get() else {
        return Err(IommuError::NoIommu);
    };

    let preempt_guard = disable_preempt();
    {
        let pt = iommu.page_table.lock();
        let mut cursor = pt
            .cursor_mut(&preempt_guard, &(daddr..daddr + PAGE_SIZE))
            .map_err(IommuError::ModificationError)?;
        // SAFETY: Unmapping a page from the IOMMU page table is always safe.
        let frag = unsafe { cursor.take_next(PAGE_SIZE) };
        debug_assert!(frag.is_some(), "unmapping an unmapped device address {daddr:#x}");
    }

    let mut cq = iommu.cmd_queue.lock();
    cq.submit(&iommu.registers, command_queue::Command::iotlb_inval_pscid(iommu.pscid));
    cq.drain(&iommu.registers);

    Ok(())
}

/// Returns whether DMA remapping is active.
pub(crate) fn has_dma_remapping() -> bool {
    DMA_REMAPPING_ENABLED.load(Ordering::Relaxed)
}

/// Returns whether interrupt remapping is active.
pub(crate) fn has_interrupt_remapping() -> bool {
    false
}
