// SPDX-License-Identifier: MPL-2.0

//! I/O port access.
//!
//! AArch64 has no port I/O; the access traits exist only to satisfy the generic
//! port-I/O interface and are never invoked.

/// An access marker type indicating that a port is only allowed to write values.
pub struct WriteOnlyAccess;
/// An access marker type indicating that a port is allowed to read or write values.
pub struct ReadWriteAccess;

/// A marker trait for access types which allow writing port values.
pub trait IoPortWriteAccess {}
/// A marker trait for access types which allow reading port values.
pub trait IoPortReadAccess {}

impl IoPortWriteAccess for WriteOnlyAccess {}
impl IoPortWriteAccess for ReadWriteAccess {}
impl IoPortReadAccess for ReadWriteAccess {}

/// A helper trait that implements the read port operation.
pub trait PortRead: Sized {
    /// Reads a `Self` value from the given port.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the I/O port could have side effects that violate memory
    /// safety.
    unsafe fn read_from_port(_port: u16) -> Self {
        unimplemented!("AArch64 does not support I/O port access")
    }
}

/// A helper trait that implements the write port operation.
pub trait PortWrite: Sized {
    /// Writes a `Self` value to the given port.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the I/O port could have side effects that violate memory
    /// safety.
    unsafe fn write_to_port(_port: u16, _value: Self) {
        unimplemented!("AArch64 does not support I/O port access")
    }
}

impl PortRead for u8 {}
impl PortWrite for u8 {}
impl PortRead for u16 {}
impl PortWrite for u16 {}
impl PortRead for u32 {}
impl PortWrite for u32 {}
