// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use crate::structured_values::{NtfsStructuredValue, NtfsStructuredValueFromData};
use alloc::vec::Vec;
use core::mem;

/// The smallest VolumeName attribute has a name containing just a single character.
const VOLUME_NAME_MIN_SIZE: usize = mem::size_of::<u16>();

/// The largest VolumeName attribute has a name containing 128 UTF-16 code points (256 bytes).
const VOLUME_NAME_MAX_SIZE: usize = 128 * mem::size_of::<u16>();

#[derive(Clone, Debug)]
pub struct NtfsVolumeName {
    name: Vec<u8>,
}

impl NtfsVolumeName {
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

impl NtfsStructuredValue for NtfsVolumeName {
    const TY: NtfsAttributeType = NtfsAttributeType::VolumeName;
}

impl<'d> NtfsStructuredValueFromData<'d> for NtfsVolumeName {
    fn from_data(data: &'d [u8], position: u64) -> Result<Self> {
        if data.len() < VOLUME_NAME_MIN_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MIN_SIZE,
                actual: data.len(),
            });
        } else if data.len() > VOLUME_NAME_MAX_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::VolumeName,
                expected: VOLUME_NAME_MAX_SIZE,
                actual: data.len(),
            });
        }

        let name = data.to_vec();
        Ok(Self { name })
    }
}
