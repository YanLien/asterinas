// SPDX-License-Identifier: MPL-2.0

//! FDT-based discovery of the RISC-V IOMMU.

use core::ops::Range;

use crate::arch::boot::DEVICE_TREE;

/// Information about a discovered IOMMU.
pub(super) struct IommuInfo {
    /// The MMIO register range of the IOMMU.
    pub reg_range: Range<usize>,
}

/// Searches the device tree for a RISC-V IOMMU node
/// (`compatible = "riscv,iommu"`).
///
/// Returns `None` if no IOMMU is present.
pub(super) fn discover() -> Option<IommuInfo> {
    let device_tree = DEVICE_TREE.get()?;

    let iommu_node = device_tree
        .all_nodes()
        .find(|node| {
            node.compatible()
                .is_some_and(|c| c.all().any(|s| s == "riscv,iommu"))
        })?;

    let mut regs = iommu_node.reg()?;
    let reg = regs.next()?;
    let base = reg.starting_address as usize;
    let size = reg.size?;

    Some(IommuInfo {
        reg_range: base..base + size,
    })
}
