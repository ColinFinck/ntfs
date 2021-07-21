// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::structured_values::{NtfsStructuredValue, NtfsStructuredValueFromData};
use binread::io::Cursor;
use binread::{BinRead, BinReaderExt};
use bitflags::bitflags;

/// Size of all [`VolumeInformationData`] fields.
const VOLUME_INFORMATION_SIZE: usize = 12;

#[derive(BinRead, Clone, Debug)]
struct VolumeInformationData {
    reserved: u64,
    major_version: u8,
    minor_version: u8,
    flags: u16,
}

bitflags! {
    pub struct NtfsVolumeFlags: u16 {
        /// The volume needs to be checked by `chkdsk`.
        const IS_DIRTY = 0x0001;
        const RESIZE_LOG_FILE = 0x0002;
        const UPGRADE_ON_MOUNT = 0x0004;
        const MOUNTED_ON_NT4 = 0x0008;
        const DELETE_USN_UNDERWAY = 0x0010;
        const REPAIR_OBJECT_ID = 0x0020;
        const CHKDSK_UNDERWAY = 0x4000;
        const MODIFIED_BY_CHKDSK = 0x8000;
    }
}

#[derive(Clone, Debug)]
pub struct NtfsVolumeInformation {
    info: VolumeInformationData,
}

impl NtfsVolumeInformation {
    pub fn flags(&self) -> NtfsVolumeFlags {
        NtfsVolumeFlags::from_bits_truncate(self.info.flags)
    }

    pub fn major_version(&self) -> u8 {
        self.info.major_version
    }

    pub fn minor_version(&self) -> u8 {
        self.info.minor_version
    }
}

impl NtfsStructuredValue for NtfsVolumeInformation {
    const TY: NtfsAttributeType = NtfsAttributeType::VolumeInformation;
}

impl<'d> NtfsStructuredValueFromData<'d> for NtfsVolumeInformation {
    fn from_data(data: &'d [u8], position: u64) -> Result<Self> {
        if data.len() < VOLUME_INFORMATION_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::StandardInformation,
                expected: VOLUME_INFORMATION_SIZE,
                actual: data.len(),
            });
        }

        let mut cursor = Cursor::new(data);
        let info = cursor.read_le::<VolumeInformationData>()?;

        Ok(Self { info })
    }
}
