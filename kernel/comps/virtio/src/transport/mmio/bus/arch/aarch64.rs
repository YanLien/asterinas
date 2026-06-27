// SPDX-License-Identifier: MPL-2.0

pub(super) use ostd::arch::irq::MappedIrqLine;
use ostd::arch::{
    boot::DEVICE_TREE,
    irq::{IRQ_CHIP, InterruptSourceInFdt},
};

pub(super) fn probe_for_device() {
    // The device tree parsing logic here assumes a Linux-compatible device
    // tree.
    // Reference: <https://www.kernel.org/doc/Documentation/devicetree/bindings/virtio/mmio.txt>.
    let device_tree = DEVICE_TREE.get().unwrap();
    let mmio_nodes = device_tree.all_nodes().filter(|node| {
        node.compatible().is_some_and(|compatibles| {
            compatibles
                .all()
                .any(|compatible| compatible == "virtio,mmio")
        })
    });
    mmio_nodes.for_each(|node| {
        let mmio_region = node.reg().unwrap().next().unwrap();
        let mmio_start = mmio_region.starting_address as usize;
        let mmio_end = mmio_start + mmio_region.size.unwrap();

        // GIC interrupts use a 3-cell specifier: <type number flags>.
        //   type == 0: SPI  -> INTID = 32 + number
        //   type == 1: PPI  -> INTID = 16 + number
        let interrupt_cells = node.property("interrupts").unwrap().value;
        let intid = {
            let cells = interrupt_cells;
            let irq_type = u32::from_be_bytes(cells[0..4].try_into().unwrap());
            let irq_number = u32::from_be_bytes(cells[4..8].try_into().unwrap());
            if irq_type == 0 {
                32 + irq_number
            } else {
                16 + irq_number
            }
        };

        let interrupt_source_in_fdt = InterruptSourceInFdt {
            interrupt: intid,
            // `interrupt-parent` is often inherited from the root and not
            // stated explicitly on QEMU `virt`; the GIC mapping uses the INTID
            // directly, so a missing parent is harmless.
            interrupt_parent: node
                .property("interrupt-parent")
                .and_then(|prop| prop.as_usize())
                .unwrap_or(0) as u32,
        };

        let _ = super::try_register_mmio_device(mmio_start..mmio_end, |irq_line| {
            IRQ_CHIP
                .get()
                .unwrap()
                .map_fdt_pin_to(interrupt_source_in_fdt, irq_line)
        });
    });
}
