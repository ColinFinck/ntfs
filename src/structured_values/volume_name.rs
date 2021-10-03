// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use crate::structured_values::{
    NtfsStructuredValue, NtfsStructuredValueFromResidentAttributeValue,
};
use crate::value::slice::NtfsSliceValue;
use crate::value::NtfsValue;
use arrayvec::ArrayVec;
use binread::io::{Cursor, Read, Seek};
use core::mem;

/// The smallest VolumeName attribute has a name containing just a single character.
const VOLUME_NAME_MIN_SIZE: usize = mem::size_of::<u16>();

/// The largest VolumeName attribute has a name containing 128 UTF-16 code points (256 bytes).
const VOLUME_NAME_MAX_SIZE: usize = 128 * mem::size_of::<u16>();

#[derive(Clone, Debug)]
pub struct NtfsVolumeName {
    name: ArrayVec<u8, VOLUME_NAME_MAX_SIZE>,
}

impl NtfsVolumeName {
    fn new<T>(r: &mut T, position: u64, value_length: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < VOLUME_NAME_MIN_SIZE as u64 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MIN_SIZE as u64,
                actual: value_length,
            });
        } else if value_length > VOLUME_NAME_MAX_SIZE as u64 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MAX_SIZE as u64,
                actual: value_length,
            });
        }

        let value_length = value_length as usize;

        let mut name = ArrayVec::from([0u8; VOLUME_NAME_MAX_SIZE]);
        r.read_exact(&mut name[..value_length])?;
        name.truncate(value_length);

        Ok(Self { name })
    }

    /// Gets the file name and returns it wrapped in an [`NtfsString`].
    pub fn name<'s>(&'s self) -> NtfsString<'s> {
        NtfsString(&self.name)
    }

    /// Returns the volume name length, in bytes.
    ///
    /// A volume name has a maximum length of 128 UTF-16 code points (256 bytes).
    pub fn name_length(&self) -> usize {
        self.name.len()
    }
}

impl<'n, 'f> NtfsStructuredValue<'n, 'f> for NtfsVolumeName {
    const TY: NtfsAttributeType = NtfsAttributeType::VolumeName;

    fn from_value<T>(fs: &mut T, value: NtfsValue<'n, 'f>) -> Result<Self>
    where
        T: Read + Seek,
    {
        let position = value.data_position().unwrap();
        let value_length = value.len();

        let mut value_attached = value.attach(fs);
        Self::new(&mut value_attached, position, value_length)
    }
}

impl<'n, 'f> NtfsStructuredValueFromResidentAttributeValue<'n, 'f> for NtfsVolumeName {
    fn from_resident_attribute_value(value: NtfsSliceValue<'f>) -> Result<Self> {
        let position = value.data_position().unwrap();
        let value_length = value.len();

        let mut cursor = Cursor::new(value.data());
        Self::new(&mut cursor, position, value_length)
    }
}
