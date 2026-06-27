// SPDX-License-Identifier: MPL-2.0

use inherit_methods_macro::inherit_methods;
use ostd::{
    arch::serial::{SERIAL_PORT, Pl011},
    sync::{LocalIrqDisabled, SpinLock},
};

use crate::{
    CONSOLE_NAME,
    alloc::string::ToString,
    console::{Uart, UartConsole},
};

impl Uart for SpinLock<Pl011, LocalIrqDisabled> {
    fn send(&self, buf: &[u8]) {
        let mut uart = self.lock();
        for byte in buf {
            // Translate NL to CRLF (termios ONLCR behavior).
            if *byte == b'\n' {
                uart.send(b'\r');
            }
            uart.send(*byte);
        }
    }

    fn recv(&self, buf: &mut [u8]) -> usize {
        let mut uart = self.lock();
        for (i, byte) in buf.iter_mut().enumerate() {
            let Some(recv_byte) = uart.recv() else {
                return i;
            };
            *byte = recv_byte;
        }
        buf.len()
    }

    fn flush(&self) {
        let mut uart = self.lock();
        while uart.recv().is_some() {}
    }
}

#[inherit_methods(from = "(**self)")]
impl Uart for &SpinLock<Pl011, LocalIrqDisabled> {
    fn send(&self, buf: &[u8]);
    fn recv(&self, buf: &mut [u8]) -> usize;
    fn flush(&self);
}

pub(super) fn init() {
    let Some(uart) = SERIAL_PORT.get() else {
        return;
    };

    let uart_console = UartConsole::new(uart);

    aster_console::register_device(CONSOLE_NAME.to_string(), uart_console.clone());

    // TODO: Set up the IRQ line and handle the received data.
    // Suppress the dead code warnings of the related methods.
    let _ = || uart_console.trigger_input_callbacks();

    ostd::info!("Registered PL011 as a console");
}
