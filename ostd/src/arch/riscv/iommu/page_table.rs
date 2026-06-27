// SPDX-License-Identifier: MPL-2.0

//! IOMMU second-stage page table configuration.
//!
//! The RISC-V IOMMU uses standard RISC-V Sv39/Sv48 page table formats for
//! second-stage (device) translation. This module defines [`IommuPtConfig`]
//! which lets the generic DMA allocator (`mm/dma/util.rs`) allocate device
//! addresses over the same address space managed by the IOMMU page tables.

use core::marker::PhantomData;
use core::ops::Range;

use crate::mm::{
    Paddr, PagingLevel,
    page_prop::PageProperty,
    page_table::PageTableConfig,
};

/// Marker type for the IOMMU second-stage page table configuration.
///
/// It reuses the standard RISC-V Sv48 paging constants and PTE format
/// (same as the CPU page table), so the generic page-table walker can
/// operate on IOMMU page tables directly.
#[derive(Clone, Debug)]
pub struct IommuPtConfig;

/// A mapped item in the IOMMU page table: (physical address, level, property).
type IommuItem = (Paddr, PagingLevel, PageProperty);

// SAFETY: The implementation mirrors the kernel page table configuration.
unsafe impl PageTableConfig for IommuPtConfig {
    const TOP_LEVEL_INDEX_RANGE: Range<usize> = 0..256;
    const TOP_LEVEL_CAN_UNMAP: bool = true;

    type E = crate::arch::mm::PageTableEntry;
    type C = crate::arch::mm::PagingConsts;

    type Item = IommuItem;
    type ItemRef<'a> = PhantomData<&'a ()>;

    fn item_raw_info(item: &Self::Item) -> (Paddr, PagingLevel, PageProperty) {
        *item
    }

    unsafe fn item_from_raw(
        paddr: Paddr,
        level: PagingLevel,
        prop: PageProperty,
    ) -> Self::Item {
        (paddr, level, prop)
    }

    unsafe fn item_ref_from_raw<'a>(
        _paddr: Paddr,
        _level: PagingLevel,
        _prop: PageProperty,
    ) -> Self::ItemRef<'a> {
        PhantomData
    }
}
