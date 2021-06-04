// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use crate::structured_values::NewNtfsStructuredValue;
use binread::io::{Read, Seek};
use core::mem;

/// The smallest VolumeName attribute has a name containing just a single character.
const VOLUME_NAME_MIN_SIZE: u64 = mem::size_of::<u16>() as u64;

/// The largest VolumeName attribute has a name containing 128 UTF-16 code points (256 bytes).
const VOLUME_NAME_MAX_SIZE: u64 = 128 * mem::size_of::<u16>() as u64;

#[derive(Clone, Debug)]
pub struct NtfsVolumeName<'n> {
    value: NtfsAttributeValue<'n>,
    name_length: u16,
}

impl<'n> NtfsVolumeName<'n> {
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
        let value_attached = self.value.clone().attach(fs);
        NtfsString::from_reader(value_attached, self.name_length(), buf)
    }
}

impl<'n> NewNtfsStructuredValue<'n> for NtfsVolumeName<'n> {
    fn new<T>(_fs: &mut T, value: NtfsAttributeValue<'n>, length: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        if length < VOLUME_NAME_MIN_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MIN_SIZE,
                actual: length,
            });
        } else if length > VOLUME_NAME_MAX_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MAX_SIZE,
                actual: length,
            });
        }

        let name_length = length as u16;

        Ok(Self { value, name_length })
    }
}
