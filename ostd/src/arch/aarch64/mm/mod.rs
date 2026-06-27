// SPDX-License-Identifier: MPL-2.0

//! AArch64 paging.

use core::ops::Range;

pub(crate) use util::{
    __atomic_cmpxchg_fallible, __atomic_load_fallible, __memcpy_fallible, __memset_fallible,
};

use crate::mm::{
    PAGE_SIZE, Paddr, PagingConstsTrait, PagingLevel, PodOnce, Vaddr,
    dma::DmaDirection,
    page_prop::{
        CachePolicy, PageFlags, PageProperty, PageTableFlags, PrivilegedPageFlags as PrivFlags,
    },
    page_table::{PteScalar, PteTrait},
};

mod util;

#[derive(Clone, Debug, Default)]
pub(crate) struct PagingConsts {}

impl PagingConstsTrait for PagingConsts {
    /// 4 KiB base page (4K granule).
    const BASE_PAGE_SIZE: usize = 4096;
    /// Four levels: L0 (root) .. L3 (leaf) map to asterinas levels 4 .. 1.
    const NR_LEVELS: PagingLevel = 4;
    /// 48-bit virtual address space (4K granule, 4 levels).
    const ADDRESS_WIDTH: usize = 48;
    /// AArch64 virtual addresses are sign-extended canonical addresses.
    const VA_SIGN_EXT: bool = true;
    /// The largest translation is a 1 GiB block at level 3 (aarch64 L1).
    const HIGHEST_TRANSLATION_LEVEL: PagingLevel = 3;
    const PTE_SIZE: usize = size_of::<PageTableEntry>();
}

bitflags::bitflags! {
    /// Possible flags for an AArch64 stage-1 page table entry.
    ///
    /// The bit positions follow the ARMv8-A VMSAv8-64 stage-1 descriptor layout.
    /// Software-available bits (PBHA / Res0 in bits 55..=57) are reused to store
    /// asterinas-specific metadata since the MMU ignores them.
    #[repr(C)]
    #[derive(Pod)]
    pub(crate) struct PteFlags: usize {
        /// Bit 0. The entry is valid.
        const VALID =           1 << 0;
        /// Bit 1. Descriptor type. 1 = table (or page at L3), 0 = block descriptor.
        const DESC_TYPE =       1 << 1;
        /// Bit 2. AttrIndx[0]. AttrIndx = 1 selects the device/uncacheable MAIR entry.
        const UCACHE =          1 << 2;
        /// Bit 6. AP[1]. If set, the mapping is accessible from EL0 (user mode).
        const AP_EL0 =          1 << 6;
        /// Bit 7. AP[2]. If set, the mapping is read-only.
        const AP_RO =           1 << 7;
        /// Bit 10. Access flag. Always set to avoid access-flag faults.
        const AF =              1 << 10;
        /// Bit 11. Not global. If set, the mapping is local to the current ASID.
        const NG =              1 << 11;
        /// Bit 53. Privileged execute-never.
        const PXN =             1 << 53;
        /// Bit 54. Unprivileged execute-never (also used for EL1 XN).
        const UXN =             1 << 54;
        // Software-available bits (PBHA / Res0, ignored by the MMU).
        /// Bit 55. Software bit, mapped to `PrivilegedPageFlags::AVAIL1`.
        const SW_AVAIL1 =       1 << 55;
        /// Bit 56. Software bit, mapped to `PageFlags::AVAIL2`.
        const SW_AVAIL2 =       1 << 56;
        /// Bit 57. Software bit, mapped to `PageFlags::DIRTY`.
        const SW_DIRTY =        1 << 57;
    }
}

/// Inner-shareable, used for cacheable normal memory.
const SH_INNER: usize = 0b11 << 8;

pub(crate) fn tlb_flush_addr(vaddr: Vaddr) {
    // SAFETY: Invalidating a single TLB entry by VA is safe.
    unsafe {
        core::arch::asm!(
            "tlbi vaae1is, {0}",
            "dsb ish",
            "isb",
            in(reg) vaddr >> 12,
            options(nostack, preserves_flags),
        );
    }
}

pub(crate) fn tlb_flush_addr_range(range: &Range<Vaddr>) {
    for vaddr in range.clone().step_by(PAGE_SIZE) {
        tlb_flush_addr(vaddr);
    }
}

pub(crate) fn tlb_flush_all_excluding_global() {
    // SAFETY: `vmalle1is` flushes all EL1 stage-1 TLB entries (it also flushes
    // global entries, which is a conservative over-flush, just like RISC-V does).
    unsafe {
        core::arch::asm!(
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags),
        );
    }
}

pub(crate) fn tlb_flush_all_including_global() {
    // SAFETY: Same as above.
    unsafe {
        core::arch::asm!(
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags),
        );
    }
}

pub(crate) fn can_sync_dma() -> bool {
    // The QEMU `virt` machine with Cortex-A72 is cache-coherent, so explicit
    // cache maintenance operations are not required for DMA correctness.
    false
}

/// # Safety
///
/// The caller must ensure that the virtual address range and DMA direction
/// correspond correctly to a DMA region and that `can_sync_dma()` returns `true`.
pub(crate) unsafe fn sync_dma_range<D: DmaDirection>(_range: Range<Vaddr>) {
    // Coherent DMA: nothing to do. `can_sync_dma()` is always `false` for now.
    unreachable!("sync_dma_range should not be called when can_sync_dma() is false")
}

/// Activates the given root-level page table.
///
/// Both `TTBR0_EL1` (user, low canonical half) and `TTBR1_EL1` (kernel, high
/// canonical half) are pointed at the same root, since asterinas keeps the
/// kernel mappings in the upper index range (256..512) of the same table. The
/// hardware splits translations by bit 47 of the virtual address.
///
/// # Safety
///
/// Changing the root-level page table is unsafe because it can violate memory
/// safety if the new mapping is incorrect.
pub(crate) unsafe fn activate_page_table(root_paddr: Paddr, _root_pt_cache: CachePolicy) {
    assert!(root_paddr.is_multiple_of(PagingConsts::BASE_PAGE_SIZE));
    // SAFETY: The caller ensures the page table rooted at `root_paddr` is valid.
    unsafe {
        core::arch::asm!(
            "msr ttbr0_el1, {0}",
            "msr ttbr1_el1, {0}",
            "isb",
            in(reg) root_paddr,
            options(nostack, preserves_flags),
        );
        tlb_flush_all_including_global();
    }
}

pub(crate) fn current_page_table_paddr() -> Paddr {
    let ttbr: usize;
    // SAFETY: Reading `TTBR0_EL1` has no side effects.
    unsafe { core::arch::asm!("mrs {0}, ttbr0_el1", out(reg) ttbr, options(nostack, preserves_flags)) };
    // Mask off the ASID field (bits 63..48) and low bits to recover the base address.
    ttbr & 0x0000_FFFF_FFFF_F000
}

/// A stage-1 page table entry (descriptor).
///
/// The output address is always stored in bits [47:12] regardless of the
/// descriptor type. For block descriptors this is correct because block
/// addresses are naturally aligned to the block size, so the address bits
/// below the block alignment are zero and never collide with the lower
/// attribute bits (bits [11:2]).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod)]
pub(crate) struct PageTableEntry(usize);

impl PageTableEntry {
    /// Bits [47:12] hold the output address.
    const ADDR_MASK: usize = 0x0000_FFFF_FFFF_F000;

    fn paddr(&self) -> Paddr {
        self.0 & Self::ADDR_MASK
    }

    /// Returns whether this entry is a leaf (a mapped page or block).
    fn is_last(&self, level: PagingLevel) -> bool {
        match level {
            // Level 1 (aarch64 L3) is always a page descriptor (leaf).
            1 => self.0 & PteFlags::VALID.bits() != 0,
            // Level 4 (aarch64 L0) is always a table (never a leaf).
            4 => false,
            // Levels 2 and 3 are leaves only when the descriptor is a block (bit 1 == 0).
            _ => self.0 & PteFlags::DESC_TYPE.bits() == 0,
        }
    }

    fn prop(&self) -> PageProperty {
        let bits = self.0;

        // AArch64 valid stage-1 entries are always readable.
        let mut flags = PageFlags::R;
        // AP[2] (bit 7) == 0 means writable.
        if bits & PteFlags::AP_RO.bits() == 0 {
            flags |= PageFlags::W;
        }
        // UXN (bit 54) == 0 means executable.
        if bits & PteFlags::UXN.bits() == 0 {
            flags |= PageFlags::X;
        }
        // AF (bit 10) is always set, so the page is considered accessed.
        flags |= PageFlags::ACCESSED;
        if bits & PteFlags::SW_DIRTY.bits() != 0 {
            flags |= PageFlags::DIRTY;
        }
        if bits & PteFlags::SW_AVAIL2.bits() != 0 {
            flags |= PageFlags::AVAIL2;
        }

        let mut priv_flags = PrivFlags::empty();
        if bits & PteFlags::AP_EL0.bits() != 0 {
            priv_flags |= PrivFlags::USER;
        }
        // nG (bit 11) == 0 means global.
        if bits & PteFlags::NG.bits() == 0 {
            priv_flags |= PrivFlags::GLOBAL;
        }
        if bits & PteFlags::SW_AVAIL1.bits() != 0 {
            priv_flags |= PrivFlags::AVAIL1;
        }

        let cache = if bits & PteFlags::UCACHE.bits() != 0 {
            CachePolicy::Uncacheable
        } else {
            CachePolicy::Writeback
        };

        PageProperty {
            flags,
            cache,
            priv_flags,
        }
    }

    fn pt_flags(&self) -> PageTableFlags {
        let mut bits = PageTableFlags::empty();
        if self.0 & PteFlags::SW_AVAIL1.bits() != 0 {
            bits |= PageTableFlags::AVAIL1;
        }
        if self.0 & PteFlags::SW_AVAIL2.bits() != 0 {
            bits |= PageTableFlags::AVAIL2;
        }
        bits
    }

    /// Builds a leaf descriptor (page at level 1, block at levels 2 and 3).
    fn new_page(paddr: Paddr, level: PagingLevel, prop: PageProperty) -> Self {
        let mut bits = PteFlags::VALID.bits() | PteFlags::AF.bits();

        // Descriptor type: level 1 (aarch64 L3) uses a page descriptor (bit 1 = 1);
        // levels 2 and 3 use a block descriptor (bit 1 = 0).
        if level == 1 {
            bits |= PteFlags::DESC_TYPE.bits();
        }

        if !prop.flags.contains(PageFlags::W) {
            bits |= PteFlags::AP_RO.bits();
        }
        if !prop.flags.contains(PageFlags::X) {
            bits |= PteFlags::UXN.bits() | PteFlags::PXN.bits();
        }
        if prop.priv_flags.contains(PrivFlags::USER) {
            bits |= PteFlags::AP_EL0.bits();
        }
        if !prop.priv_flags.contains(PrivFlags::GLOBAL) {
            bits |= PteFlags::NG.bits();
        }
        if prop.flags.contains(PageFlags::DIRTY) {
            bits |= PteFlags::SW_DIRTY.bits();
        }
        if prop.priv_flags.contains(PrivFlags::AVAIL1) {
            bits |= PteFlags::SW_AVAIL1.bits();
        }
        if prop.flags.contains(PageFlags::AVAIL2) {
            bits |= PteFlags::SW_AVAIL2.bits();
        }

        match prop.cache {
            CachePolicy::Writeback => bits |= SH_INNER,
            CachePolicy::Uncacheable => bits |= PteFlags::UCACHE.bits(),
            _ => panic!("unsupported cache policy"),
        }

        Self((paddr & Self::ADDR_MASK) | bits)
    }

    /// Builds a table descriptor pointing to a lower-level page table.
    fn new_pt(paddr: Paddr, flags: PageTableFlags) -> Self {
        let mut bits = PteFlags::VALID.bits() | PteFlags::DESC_TYPE.bits() | SH_INNER;
        if flags.contains(PageTableFlags::AVAIL1) {
            bits |= PteFlags::SW_AVAIL1.bits();
        }
        if flags.contains(PageTableFlags::AVAIL2) {
            bits |= PteFlags::SW_AVAIL2.bits();
        }
        Self((paddr & Self::ADDR_MASK) | bits)
    }
}

impl PodOnce for PageTableEntry {}

// SAFETY: The implementation is safe because:
//  - `from_usize` and `into_usize` are not overridden;
//  - `from_repr` and `repr` are correctly implemented;
//  - a zeroed PTE represents an absent entry.
unsafe impl PteTrait for PageTableEntry {
    fn from_repr(repr: &PteScalar, level: PagingLevel) -> Self {
        match repr {
            PteScalar::Absent => PageTableEntry(0),
            PteScalar::PageTable(paddr, flags) => Self::new_pt(*paddr, *flags),
            PteScalar::Mapped(paddr, prop) => Self::new_page(*paddr, level, *prop),
        }
    }

    fn to_repr(&self, level: PagingLevel) -> PteScalar {
        if self.0 & PteFlags::VALID.bits() == 0 {
            return PteScalar::Absent;
        }

        if self.is_last(level) {
            PteScalar::Mapped(self.paddr(), self.prop())
        } else {
            PteScalar::PageTable(self.paddr(), self.pt_flags())
        }
    }
}
