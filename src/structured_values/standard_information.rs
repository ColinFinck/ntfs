// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::structured_values::{
    NtfsFileAttributeFlags, NtfsStructuredValue, NtfsStructuredValueFromSlice,
};
use crate::time::NtfsTime;
use binread::io::Cursor;
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

impl NtfsStructuredValue for NtfsStandardInformation {
    const TY: NtfsAttributeType = NtfsAttributeType::StandardInformation;
}

impl<'s> NtfsStructuredValueFromSlice<'s> for NtfsStandardInformation {
    fn from_slice(slice: &'s [u8], position: u64) -> Result<Self> {
        if slice.len() < STANDARD_INFORMATION_SIZE_NTFS1 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::StandardInformation,
                expected: STANDARD_INFORMATION_SIZE_NTFS1,
                actual: slice.len(),
            });
        }

        let mut cursor = Cursor::new(slice);
        let ntfs1_data = cursor.read_le::<StandardInformationDataNtfs1>()?;

        let mut ntfs3_data = None;
        if slice.len() >= STANDARD_INFORMATION_SIZE_NTFS3 {
            ntfs3_data = Some(cursor.read_le::<StandardInformationDataNtfs3>()?);
        }

        Ok(Self {
            ntfs1_data,
            ntfs3_data,
        })
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
        let mut mft_attributes = mft.attributes();

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
