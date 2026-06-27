// SPDX-License-Identifier: MPL-2.0

//! Generic Interrupt Controller version 3 (GICv3).

use alloc::collections::btree_map::{BTreeMap, Entry};
use core::fmt;

use spin::Once;

use crate::{
    Result,
    arch::{boot::DEVICE_TREE, trap::TrapFrame},
    cpu::PrivilegeLevel,
    early_println,
    io::{IoMem, IoMemAllocatorBuilder, Sensitive},
    irq::{IrqLine, call_irq_callback_functions},
    sync::{LocalIrqDisabled, SpinLock},
};

/// The [`IrqChip`] singleton.
pub static IRQ_CHIP: Once<IrqChip> = Once::new();

/// The stride between consecutive redistributor (RD + SGI) frames.
const GICR_STRIDE: usize = 0x2_0000;

// GICD register offsets.
const GICD_CTLR: usize = 0x000;
const GICD_IGROUPR: usize = 0x080;
const GICD_ISENABLER: usize = 0x100;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_IROUTER: usize = 0x6000;

// GICR (RD frame) register offsets (relative to a CPU's frame base).
const GICR_WAKER: usize = 0x014;
const GICR_IGROUPR0: usize = 0x080;
const GICR_ISENABLER0: usize = 0x100;

/// An IRQ chip backed by a GICv3.
pub struct IrqChip {
    gicd: IoMem<Sensitive>,
    gicr: IoMem<Sensitive>,
    /// Maps a GIC INTID to the software IRQ number of the allocated `IrqLine`.
    intid_to_swirq: SpinLock<BTreeMap<u32, u8>, LocalIrqDisabled>,
}

impl IrqChip {
    fn gicd_write32(&self, offset: usize, val: u32) {
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicd.write_once(offset, &val) };
    }

    fn gicd_read32(&self, offset: usize) -> u32 {
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicd.read_once::<u32>(offset) }
    }

    fn gicd_write8(&self, offset: usize, val: u8) {
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicd.write_once(offset, &val) };
    }

    fn gicd_write64(&self, offset: usize, val: u64) {
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicd.write_once(offset, &val) };
    }

    fn gicr_write32(&self, cpu: u32, offset: usize, val: u32) {
        let off = cpu as usize * GICR_STRIDE + offset;
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicr.write_once(off, &val) };
    }

    fn gicr_read32(&self, cpu: u32, offset: usize) -> u32 {
        let off = cpu as usize * GICR_STRIDE + offset;
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicr.read_once::<u32>(off) }
    }

    fn gicr_write8(&self, cpu: u32, offset: usize, val: u8) {
        let off = cpu as usize * GICR_STRIDE + offset;
        // SAFETY: The caller computes offsets per the GICv3 spec.
        unsafe { self.gicr.write_once(off, &val) };
    }

    /// Enables a GIC interrupt and records its mapping to a software IRQ.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the INTID is not already mapped by another
    /// `IrqLine`, and that this is called at the right boot time.
    pub(in crate::arch) unsafe fn register_and_enable(&self, intid: u32, sw_irq: u8) {
        {
            let mut map = self.intid_to_swirq.lock();
            match map.entry(intid) {
                Entry::Occupied(_) => return,
                Entry::Vacant(v) => v.insert(sw_irq),
            };
        }
        // SAFETY: enabling a GIC interrupt is safe at this point per the caller.
        unsafe { self.enable_interrupt(intid) };
    }

    /// # Safety
    ///
    /// The caller must ensure proper boot-time ordering.
    unsafe fn enable_interrupt(&self, intid: u32) {
        // SAFETY: register accesses follow the GICv3 specification.
        unsafe {
            self.set_priority(intid, 0xA0);
            if intid < 32 {
                let cpu = crate::arch::boot::smp::get_current_cpu_id();
                self.gicr_write32(cpu, GICR_ISENABLER0, 1 << intid);
            } else {
                let word = GICD_ISENABLER + 4 * (intid / 32) as usize;
                self.gicd_write32(word, 1 << (intid % 32));
                // Route the SPI to the BSP (affinity 0).
                let router = GICD_IROUTER + 8 * (intid - 32) as usize;
                self.gicd_write64(router, 0);
            }
        }
    }

    unsafe fn set_priority(&self, intid: u32, priority: u8) {
        // SAFETY: priority registers are one byte per INTID.
        if intid < 32 {
            let cpu = crate::arch::boot::smp::get_current_cpu_id();
            self.gicr_write8(cpu, GICR_IPRIORITYR + intid as usize, priority);
        } else {
            self.gicd_write8(GICD_IPRIORITYR + intid as usize, priority);
        }
    }

    /// # Safety
    ///
    /// Must be called once during BSP GIC initialization.
    unsafe fn init_distributor(&self) {
        // Disable the distributor while configuring.
        self.gicd_write32(GICD_CTLR, 0);
        // Put SPIs in group 1.
        for word in 1..32 {
            self.gicd_write32(GICD_IGROUPR + 4 * word, 0xFFFF_FFFF);
        }
        // Set default priority for SPIs.
        for intid in 32..992 {
            self.gicd_write8(GICD_IPRIORITYR + intid as usize, 0xA0);
        }
        // Enable group 1 (affinity routing).
        self.gicd_write32(GICD_CTLR, 0b10);
        let _ = self.gicd_read32(GICD_CTLR);
    }

    /// # Safety
    ///
    /// Must be called once per CPU during GIC initialization.
    unsafe fn init_redistributor(&self) {
        let cpu = crate::arch::boot::smp::get_current_cpu_id();
        // Wake the redistributor (clear ProcessorSleep).
        self.gicr_write32(cpu, GICR_WAKER, 0);
        while self.gicr_read32(cpu, GICR_WAKER) & 0b100 != 0 {
            core::hint::spin_loop();
        }
        // Put SGIs/PPIs in group 1.
        self.gicr_write32(cpu, GICR_IGROUPR0, 0xFFFF_FFFF);
    }

    /// # Safety
    ///
    /// Must be called once per CPU after the redistributor is awake.
    unsafe fn enable_cpu_interface(&self) {
        // SAFETY: programming the CPU interface system registers per the spec.
        unsafe {
            core::arch::asm!(
                "msr icc_sre_el1, {0}",
                in(reg) 0x1u64,
                options(preserves_flags, nostack)
            );
            core::arch::asm!(
                "msr icc_pmr_el1, {0}",
                in(reg) 0xFFu64,
                options(preserves_flags, nostack)
            );
            core::arch::asm!(
                "msr icc_bpr1_el1, {0}",
                in(reg) 0x0u64,
                options(preserves_flags, nostack)
            );
            core::arch::asm!(
                "msr icc_igrpen1_el1, {0}",
                in(reg) 0x1u64,
                options(preserves_flags, nostack)
            );
        }
    }

    /// Maps a device interrupt specified in the device tree to an IRQ line.
    pub fn map_fdt_pin_to(
        &self,
        interrupt_source_in_fdt: InterruptSourceInFdt,
        irq_line: IrqLine,
    ) -> Result<MappedIrqLine> {
        let intid = interrupt_source_in_fdt.interrupt;
        let sw_irq = irq_line.num();
        {
            let mut map = self.intid_to_swirq.lock();
            if map.contains_key(&intid) {
                return Err(crate::Error::AccessDenied);
            }
            map.insert(intid, sw_irq);
        }
        // SAFETY: we hold the unique mapping for this INTID.
        unsafe { self.enable_interrupt(intid) };
        Ok(MappedIrqLine { irq_line, intid })
    }
}

// GICR IPRIORITYR offset relative to the RD frame base.
const GICR_IPRIORITYR: usize = 0x400;

/// Dispatches a claimed GIC interrupt to the registered callback, if any.
///
/// This is called from the trap handler after reading `ICC_IAR1_EL1`.
pub(in crate::arch) fn dispatch_irq(intid: u32, trap_frame: &TrapFrame, priv_level: PrivilegeLevel) {
    let chip = IRQ_CHIP.get().expect("GIC not initialized");
    let sw_irq = chip.intid_to_swirq.lock().get(&intid).copied();
    if let Some(sw_irq) = sw_irq {
        let hw_irq_line = super::HwIrqLine::new(sw_irq);
        call_irq_callback_functions(trap_frame, &hw_irq_line, priv_level);
    }
}

/// Registers and enables a GIC interrupt.
///
/// # Safety
///
/// See [`IrqChip::register_and_enable`].
pub(in crate::arch) unsafe fn register_and_enable(intid: u32, sw_irq: u8) {
    // SAFETY: the caller upholds the safety contract of `register_and_enable`.
    unsafe {
        IRQ_CHIP
            .get()
            .expect("GIC not initialized")
            .register_and_enable(intid, sw_irq);
    }
}

/// Initializes the GIC on the BSP.
///
/// # Safety
///
/// This function must be called exactly once in the boot context of the BSP,
/// before any interrupt can be delivered.
pub(in crate::arch) unsafe fn init_on_bsp(io_mem_builder: &IoMemAllocatorBuilder) {
    let device_tree = DEVICE_TREE.get().unwrap();

    // Find the GICv3 node and read the GICD and GICR register regions.
    let gic_node = device_tree
        .all_nodes()
        .find(|node| {
            node.compatible()
                .is_some_and(|c| c.all().any(|s| s == "arm,gic-v3"))
        })
        .expect("GICv3 node not found in the device tree");
    let (gicd_range, gicr_range) = {
        let mut regs = gic_node.reg().expect("GIC node has no reg property");
        let gicd = regs.next().expect("GIC has no GICD reg");
        let gicr = regs.next().expect("GIC has no GICR reg");
        let gicd_base = gicd.starting_address as usize;
        let gicd_size = gicd.size.expect("GICD has no size");
        let gicr_base = gicr.starting_address as usize;
        let gicr_size = gicr.size.expect("GICR has no size");
        early_println!(
            "GIC: gicd={:#x}+{:#x} gicr={:#x}+{:#x}",
            gicd_base,
            gicd_size,
            gicr_base,
            gicr_size
        );
        (gicd_base..gicd_base + gicd_size, gicr_base..gicr_base + gicr_size)
    };

    let gicd = io_mem_builder.reserve(gicd_range, crate::mm::CachePolicy::Uncacheable);
    let gicr = io_mem_builder.reserve(gicr_range, crate::mm::CachePolicy::Uncacheable);

    IRQ_CHIP.call_once(|| IrqChip {
        gicd,
        gicr,
        intid_to_swirq: SpinLock::new(BTreeMap::new()),
    });

    // SAFETY: initializing the GIC is safe at this point in the boot.
    unsafe {
        IRQ_CHIP.get().unwrap().init_distributor();
        IRQ_CHIP.get().unwrap().init_redistributor();
        IRQ_CHIP.get().unwrap().enable_cpu_interface();
    }
}

/// Initializes the GIC CPU interface and redistributor on an AP.
///
/// # Safety
///
/// Must be called once on each AP.
pub(in crate::arch) unsafe fn init_on_ap() {
    // SAFETY: per-CPU GIC initialization.
    unsafe {
        IRQ_CHIP.get().unwrap().init_redistributor();
        IRQ_CHIP.get().unwrap().enable_cpu_interface();
    }
}

/// An [`IrqLine`] mapped to a GIC INTID managed by the [`IRQ_CHIP`].
pub struct MappedIrqLine {
    irq_line: IrqLine,
    intid: u32,
}

impl fmt::Debug for MappedIrqLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MappedIrqLine")
            .field("irq_line", &self.irq_line)
            .field("intid", &self.intid)
            .finish_non_exhaustive()
    }
}

impl core::ops::Deref for MappedIrqLine {
    type Target = IrqLine;
    fn deref(&self) -> &Self::Target {
        &self.irq_line
    }
}

impl core::ops::DerefMut for MappedIrqLine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.irq_line
    }
}

impl Drop for MappedIrqLine {
    fn drop(&mut self) {
        let chip = IRQ_CHIP.get().unwrap();
        let mut map = chip.intid_to_swirq.lock();
        map.remove(&self.intid);
    }
}

/// Interrupt source identifier in the device tree.
#[derive(Clone, Copy, Debug)]
pub struct InterruptSourceInFdt {
    /// Phandle of the interrupt controller it connects to.
    pub interrupt_parent: u32,
    /// The GIC INTID (hardware interrupt number).
    pub interrupt: u32,
}
