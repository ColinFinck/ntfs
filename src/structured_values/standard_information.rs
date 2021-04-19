// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use crate::structured_values::NtfsFileAttributeFlags;
use crate::time::NtfsTime;
use binread::io::{Read, Seek};
use binread::{BinRead, BinReaderExt};

/// Size of all [`StandardInformationData`] fields plus some reserved bytes.
const STANDARD_INFORMATION_SIZE_NTFS1: u64 = 48;

/// Size of all [`StandardInformationData`] plus [`StandardInformationDataNtfs3`] fields.
const STANDARD_INFORMATION_SIZE_NTFS3: u64 = 72;

#[derive(BinRead, Clone, Debug)]
struct StandardInformationData {
    creation_time: NtfsTime,
    modification_time: NtfsTime,
    mft_record_modification_time: NtfsTime,
    access_time: NtfsTime,
    file_attributes: u32,
}

#[derive(BinRead, Clone, Debug)]
struct StandardInformationDataNtfs3 {
    maximum_versions: u32,
    version: u32,
    class_id: u32,
    owner_id: u32,
    security_id: u32,
    quota_charged: u64,
    usn: u64,
}

#[derive(Clone, Debug)]
pub struct NtfsStandardInformation {
    data: StandardInformationData,
    ntfs3_data: Option<StandardInformationDataNtfs3>,
}

impl NtfsStandardInformation {
    pub(crate) fn new<T>(
        attribute_position: u64,
        mut value_attached: NtfsAttributeValueAttached<'_, T>,
        value_length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < STANDARD_INFORMATION_SIZE_NTFS1 {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::StandardInformation,
                expected: STANDARD_INFORMATION_SIZE_NTFS1,
                actual: value_length,
            });
        }

        let data = value_attached.read_le::<StandardInformationData>()?;
        let ntfs3_data = if value_length >= STANDARD_INFORMATION_SIZE_NTFS3 as u64 {
            Some(value_attached.read_le::<StandardInformationDataNtfs3>()?)
        } else {
            None
        };

        Ok(Self { data, ntfs3_data })
    }

    pub fn access_time(&self) -> NtfsTime {
        self.data.access_time
    }

    pub fn class_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.class_id)
    }

    pub fn creation_time(&self) -> NtfsTime {
        self.data.creation_time
    }

    pub fn file_attributes(&self) -> NtfsFileAttributeFlags {
        NtfsFileAttributeFlags::from_bits_truncate(self.data.file_attributes)
    }

    pub fn maximum_versions(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.maximum_versions)
    }

    pub fn mft_record_modification_time(&self) -> NtfsTime {
        self.data.mft_record_modification_time
    }

    pub fn modification_time(&self) -> NtfsTime {
        self.data.modification_time
    }

    pub fn owner_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.owner_id)
    }

    pub fn quota_charged(&self) -> Option<u64> {
        self.ntfs3_data.as_ref().map(|x| x.quota_charged)
    }

    pub fn security_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.security_id)
    }

    pub fn usn(&self) -> Option<u64> {
        self.ntfs3_data.as_ref().map(|x| x.usn)
    }

    pub fn version(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.version)
    }
}
