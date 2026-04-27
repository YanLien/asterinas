// SPDX-License-Identifier: MPL-2.0

use super::SyscallReturn;
use crate::{device, prelude::*};

pub fn sys_getrandom(buf: Vaddr, count: usize, flags: u32, ctx: &Context) -> Result<SyscallReturn> {
    let flags = GetRandomFlags::from_bits(flags)
        .ok_or_else(|| Error::with_message(Errno::EINVAL, "invalid flags"))?;
    debug!(
        "buf = 0x{:x}, count = 0x{:x}, flags = {:?}",
        buf, count, flags
    );

    if flags.contains(GetRandomFlags::GRND_INSECURE | GetRandomFlags::GRND_RANDOM) {
        return_errno_with_message!(
            Errno::EINVAL,
            "requesting insecure and blocking randomness makes no sense"
        );
    }

    // The RNG is initialized during boot with hardware entropy and continuously
    // reseeded from environmental noise (TSC jitter and hardware RNG). It is
    // always ready by the time any userspace program runs. Both /dev/random and
    // /dev/urandom are non-blocking, following the Linux 5.6+ approach.
    // GRND_NONBLOCK has no effect. GRND_INSECURE behaves the same as the
    // default (urandom) path.

    let user_space = ctx.user_space();
    let mut writer = user_space.writer(buf, count)?;
    let read_len = if flags.contains(GetRandomFlags::GRND_RANDOM) {
        device::getrandom(&mut writer)?
    } else {
        // Default, GRND_NONBLOCK, and GRND_INSECURE all use the urandom path.
        device::geturandom(&mut writer)?
    };
    Ok(SyscallReturn::Return(read_len as isize))
}

bitflags::bitflags! {
    /// Flags for `getrandom`.
    ///
    /// Reference: <https://elixir.bootlin.com/linux/v6.16.9/source/include/uapi/linux/random.h#L56>.
    struct GetRandomFlags: u32 {
        const GRND_NONBLOCK = 0x0001;
        const GRND_RANDOM = 0x0002;
        const GRND_INSECURE = 0x0004;
    }
}
