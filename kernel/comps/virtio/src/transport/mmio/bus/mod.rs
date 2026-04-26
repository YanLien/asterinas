// SPDX-License-Identifier: MPL-2.0

//! Virtio over MMIO

use core::ops::Range;

use bus::MmioBus;
use ostd::{debug, io::IoMem, irq::IrqLine, sync::SpinLock};

use crate::transport::mmio::bus::common_device::{
    MmioCommonDevice, mmio_check_magic, mmio_read_device_id,
};

#[cfg_attr(target_arch = "x86_64", path = "arch/x86.rs")]
#[cfg_attr(target_arch = "riscv64", path = "arch/riscv.rs")]
#[cfg_attr(target_arch = "loongarch64", path = "arch/loongarch.rs")]
mod arch;

#[expect(clippy::module_inception)]
pub(super) mod bus;
pub(super) mod common_device;

/// The MMIO bus instance.
pub(super) static MMIO_BUS: SpinLock<MmioBus> = SpinLock::new(MmioBus::new());

pub(super) fn init() {
    ostd::early_println!("[virtio-debug] mmio bus init begin");
    #[cfg(target_arch = "x86_64")]
    ostd::if_tdx_enabled!({
        ostd::early_println!("[virtio-debug] skip mmio probe because TDX is enabled");
        // TODO: support virtio-mmio devices on TDX.
        //
        // Currently, virtio-mmio devices need to acquire sub-page MMIO regions,
        // which are not supported by `IoMem::acquire` in the TDX environment.
    } else {
        ostd::early_println!("[virtio-debug] mmio bus probe begin");
        arch::probe_for_device();
        ostd::early_println!("[virtio-debug] mmio bus probe done");
    });
    #[cfg(not(target_arch = "x86_64"))]
    arch::probe_for_device();
}

/// Tries to validate a potential VirtIO-MMIO device, map it to an IRQ line, and
/// register it as a VirtIO-MMIO device.
///
/// Returns `Ok(())` if the device was registered, or a specific
/// `MmioRegisterError` otherwise.
#[cfg_attr(target_arch = "loongarch64", expect(unused))]
fn try_register_mmio_device<F>(
    mmio_range: Range<usize>,
    map_irq_line: F,
) -> Result<(), MmioRegisterError>
where
    F: FnOnce(IrqLine) -> ostd::Result<arch::MappedIrqLine>,
{
    let start_addr = mmio_range.start;
    ostd::early_println!(
        "[virtio-debug] try_register_mmio_device range {:#x}..{:#x}",
        mmio_range.start,
        mmio_range.end
    );
    let Ok(io_mem) = IoMem::acquire(mmio_range) else {
        ostd::early_println!("[virtio-debug] IoMem::acquire failed at {:#x}", start_addr);
        debug!(
            "Abort MMIO detection at {:#x} because the MMIO address is not available",
            start_addr
        );
        return Err(MmioRegisterError::MmioUnavailable);
    };
    ostd::early_println!("[virtio-debug] IoMem::acquire ok at {:#x}", start_addr);

    // We now check the requirements specified in Virtual I/O Device (VIRTIO) Version 1.3,
    // Section 4.2.2.2 Driver Requirements: MMIO Device Register Layout.

    // "The driver MUST ignore a device with MagicValue which is not 0x74726976, although it
    // MAY report an error."
    if !mmio_check_magic(&io_mem) {
        ostd::early_println!("[virtio-debug] magic mismatch at {:#x}", start_addr);
        debug!(
            "Abort MMIO detection at {:#x} because the magic number does not match",
            start_addr
        );
        return Err(MmioRegisterError::MagicMismatch);
    }
    ostd::early_println!("[virtio-debug] magic ok at {:#x}", start_addr);

    // TODO: "The driver MUST ignore a device with Version which is not 0x2, although it MAY
    // report an error."

    // "The driver MUST ignore a device with DeviceID 0x0, but MUST NOT report any error."
    match mmio_read_device_id(&io_mem) {
        Err(_) | Ok(0) => {
            ostd::early_println!("[virtio-debug] no virtio device id at {:#x}", start_addr);
            return Err(MmioRegisterError::NoDevice);
        }
        Ok(device_id) => {
            ostd::early_println!(
                "[virtio-debug] device id {} at {:#x}",
                device_id,
                start_addr
            );
        }
    }

    let Ok(mapped_irq_line) = IrqLine::alloc().and_then(map_irq_line) else {
        ostd::early_println!("[virtio-debug] irq unavailable at {:#x}", start_addr);
        debug!(
            "Ignore MMIO device at {:#x} because its IRQ line is not available",
            start_addr
        );
        return Err(MmioRegisterError::IrqUnavailable);
    };

    let device = MmioCommonDevice::new(io_mem, mapped_irq_line);
    MMIO_BUS.lock().register_mmio_device(device);
    ostd::early_println!("[virtio-debug] registered mmio device at {:#x}", start_addr);

    Ok(())
}

#[derive(Clone, Copy)]
enum MmioRegisterError {
    /// MMIO region not available.
    MmioUnavailable,
    /// Not a VirtIO-MMIO slot.
    MagicMismatch,
    /// No device present.
    NoDevice,
    /// IRQ line not available.
    IrqUnavailable,
}
