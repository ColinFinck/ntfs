// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::structured_values::{
    NtfsFileAttributeFlags, NtfsStructuredValue, NtfsStructuredValueFromResidentAttributeValue,
};
use crate::time::NtfsTime;
use crate::value::slice::NtfsSliceValue;
use crate::value::NtfsValue;
use binread::io::{Cursor, Read, Seek};
use binread::{BinRead, BinReaderExt};

/// Size of all [`StandardInformationData`] fields plus some reserved bytes.
const STANDARD_INFORMATION_SIZE_NTFS1: usize = 48;

/// Size of all [`StandardInformationData`] plus [`StandardInformationDataNtfs3`] fields.
const STANDARD_INFORMATION_SIZE_NTFS3: usize = 72;

#[derive(BinRead, Clone, Debug)]
struct StandardInformationDataNtfs1 {
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
    ntfs1_data: StandardInformationDataNtfs1,
    ntfs3_data: Option<StandardInformationDataNtfs3>,
}

impl NtfsStandardInformation {
    fn new<T>(r: &mut T, position: u64, value_length: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < STANDARD_INFORMATION_SIZE_NTFS1 as u64 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::StandardInformation,
                expected: STANDARD_INFORMATION_SIZE_NTFS1 as u64,
                actual: value_length,
            });
        }

        let ntfs1_data = r.read_le::<StandardInformationDataNtfs1>()?;

        let mut ntfs3_data = None;
        if value_length >= STANDARD_INFORMATION_SIZE_NTFS3 as u64 {
            ntfs3_data = Some(r.read_le::<StandardInformationDataNtfs3>()?);
        }

        Ok(Self {
            ntfs1_data,
            ntfs3_data,
        })
    }

    pub fn access_time(&self) -> NtfsTime {
        self.ntfs1_data.access_time
    }

    pub fn class_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.class_id)
    }

    pub fn creation_time(&self) -> NtfsTime {
        self.ntfs1_data.creation_time
    }

    pub fn file_attributes(&self) -> NtfsFileAttributeFlags {
        NtfsFileAttributeFlags::from_bits_truncate(self.ntfs1_data.file_attributes)
    }

    pub fn maximum_versions(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.maximum_versions)
    }

    pub fn mft_record_modification_time(&self) -> NtfsTime {
        self.ntfs1_data.mft_record_modification_time
    }

    pub fn modification_time(&self) -> NtfsTime {
        self.ntfs1_data.modification_time
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

impl<'n, 'f> NtfsStructuredValue<'n, 'f> for NtfsStandardInformation {
    const TY: NtfsAttributeType = NtfsAttributeType::StandardInformation;

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

impl<'n, 'f> NtfsStructuredValueFromResidentAttributeValue<'n, 'f> for NtfsStandardInformation {
    fn from_resident_attribute_value(value: NtfsSliceValue<'f>) -> Result<Self> {
        let position = value.data_position().unwrap();
        let value_length = value.len();

        let mut cursor = Cursor::new(value.data());
        Self::new(&mut cursor, position, value_length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::KnownNtfsFileRecordNumber;
    use crate::ntfs::Ntfs;

    #[test]
    fn test_standard_information() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let mft = ntfs
            .file(&mut testfs1, KnownNtfsFileRecordNumber::MFT as u64)
            .unwrap();
        let mut mft_attributes = mft.attributes_raw();

        // Check the StandardInformation attribute of the MFT.
        let attribute = mft_attributes.nth(0).unwrap();
        assert_eq!(
            attribute.ty().unwrap(),
            NtfsAttributeType::StandardInformation,
        );
        assert_eq!(attribute.attribute_length(), 96);
        assert!(attribute.is_resident());
        assert_eq!(attribute.name_length(), 0);
        assert_eq!(attribute.value_length(), 72);

        // Try to read the actual information.
        let _standard_info = attribute
            .resident_structured_value::<NtfsStandardInformation>()
            .unwrap();

        // There are no reliable values to check here, so that's it.
    }
}
