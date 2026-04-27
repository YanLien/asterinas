// SPDX-License-Identifier: MPL-2.0

use crate::{
    fs::file::{AccessMode, CreationFlags, InodeMode, StatusFlags},
    prelude::*,
};

/// Arguments for an open request.
#[derive(Debug)]
pub struct OpenArgs {
    pub creation_flags: CreationFlags,
    pub status_flags: StatusFlags,
    pub access_mode: AccessMode,
    pub inode_mode: InodeMode,
}

impl OpenArgs {
    /// Creates `OpenArgs` from the given flags and mode.
    pub fn from_flags_and_mode(flags: u32, inode_mode: InodeMode) -> Result<Self> {
        let creation_flags = CreationFlags::from_bits_truncate(flags);
        let status_flags = StatusFlags::from_bits_truncate(flags);
        let access_mode = AccessMode::from_u32(flags)?;

        if creation_flags.contains(CreationFlags::O_TMPFILE) {
            if !creation_flags.contains(CreationFlags::O_DIRECTORY) {
                return_errno_with_message!(
                    Errno::EINVAL,
                    "O_TMPFILE requires O_DIRECTORY"
                );
            }
            if !access_mode.is_writable() {
                return_errno_with_message!(
                    Errno::EINVAL,
                    "O_TMPFILE requires O_RDWR or O_WRONLY"
                );
            }
            if creation_flags.contains(CreationFlags::O_CREAT) {
                return_errno_with_message!(
                    Errno::EINVAL,
                    "O_TMPFILE and O_CREAT are mutually exclusive"
                );
            }
            // O_EXCL with O_TMPFILE is silently ignored by Linux.
            if status_flags.contains(StatusFlags::O_PATH) {
                return_errno_with_message!(
                    Errno::EINVAL,
                    "O_TMPFILE and O_PATH are mutually exclusive"
                );
            }
        } else if creation_flags.contains(CreationFlags::O_CREAT)
            && creation_flags.contains(CreationFlags::O_DIRECTORY)
        {
            return_errno_with_message!(
                Errno::EINVAL,
                "O_CREAT and O_DIRECTORY cannot be specified together"
            );
        }

        Ok(Self {
            creation_flags,
            status_flags,
            access_mode,
            inode_mode,
        })
    }

    /// Creates `OpenArgs` from the given access mode and inode mode.
    pub fn from_modes(access_mode: AccessMode, inode_mode: InodeMode) -> Self {
        Self {
            creation_flags: CreationFlags::empty(),
            status_flags: StatusFlags::empty(),
            access_mode,
            inode_mode,
        }
    }

    /// Returns whether to follow the tail link when resolving the path.
    pub fn follow_tail_link(&self) -> bool {
        !(self.creation_flags.contains(CreationFlags::O_NOFOLLOW)
            || self.creation_flags.contains(CreationFlags::O_CREAT)
                && self.creation_flags.contains(CreationFlags::O_EXCL))
    }

    /// Returns whether this is an O_TMPFILE open request.
    pub fn is_tmpfile(&self) -> bool {
        self.creation_flags.contains(CreationFlags::O_TMPFILE)
    }
}
