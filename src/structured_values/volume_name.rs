// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use binread::io::{Read, Seek};
use core::mem;

/// The smallest VolumeName attribute has a name containing just a single character.
const VOLUME_NAME_MIN_SIZE: u64 = mem::size_of::<u16>() as u64;

/// The largest VolumeName attribute has a name containing 128 UTF-16 code points (256 bytes).
const VOLUME_NAME_MAX_SIZE: u64 = 128 * mem::size_of::<u16>() as u64;

#[derive(Clone, Debug)]
pub struct NtfsVolumeName {
    name_position: u64,
    name_length: u16,
}

impl NtfsVolumeName {
    pub(crate) fn new<T>(
        attribute_position: u64,
        value_attached: NtfsAttributeValueAttached<'_, T>,
        value_length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < VOLUME_NAME_MIN_SIZE {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MIN_SIZE,
                actual: value_length,
            });
        } else if value_length > VOLUME_NAME_MAX_SIZE {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MAX_SIZE,
                actual: value_length,
            });
        }

        let name_position = value_attached.position();
        let name_length = value_length as u16;

        Ok(Self {
            name_position,
            name_length,
        })
    }

    /// Returns the volume name length, in bytes.
    ///
    /// A volume name has a maximum length of 128 UTF-16 code points (256 bytes).
    pub fn name_length(&self) -> usize {
        self.name_length as usize
    }

    /// Reads the volume name into the given buffer, and returns an
    /// [`NtfsString`] wrapping that buffer.
    pub fn read_name<'a, T>(&self, fs: &mut T, buf: &'a mut [u8]) -> Result<NtfsString<'a>>
    where
        T: Read + Seek,
    {
        NtfsString::read_from_fs(fs, self.name_position, self.name_length(), buf)
    }
}
