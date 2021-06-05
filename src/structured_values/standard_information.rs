// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use crate::structured_values::{NewNtfsStructuredValue, NtfsFileAttributeFlags};
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

impl<'n> NewNtfsStructuredValue<'n> for NtfsStandardInformation {
    fn new<T>(
        _ntfs: &'n Ntfs,
        fs: &mut T,
        value: NtfsAttributeValue<'n>,
        length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if length < STANDARD_INFORMATION_SIZE_NTFS1 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::StandardInformation,
                expected: STANDARD_INFORMATION_SIZE_NTFS1,
                actual: length,
            });
        }

        let mut value_attached = value.attach(fs);
        let data = value_attached.read_le::<StandardInformationData>()?;

        let mut ntfs3_data = None;
        if length >= STANDARD_INFORMATION_SIZE_NTFS3 {
            ntfs3_data = Some(value_attached.read_le::<StandardInformationDataNtfs3>()?);
        }

        Ok(Self { data, ntfs3_data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfs::Ntfs;
    use crate::ntfs_file::KnownNtfsFile;
    use crate::structured_values::NtfsStructuredValue;

    #[test]
    fn test_standard_information() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let mft = ntfs
            .ntfs_file(&mut testfs1, KnownNtfsFile::MFT as u64)
            .unwrap();
        let mut mft_attributes = mft.attributes().attach(&mut testfs1);

        // Check the StandardInformation attribute of the MFT.
        let attribute = mft_attributes.nth(0).unwrap().unwrap();
        assert_eq!(
            attribute.ty().unwrap(),
            NtfsAttributeType::StandardInformation,
        );
        assert_eq!(attribute.attribute_length(), 96);
        assert!(attribute.is_resident());
        assert_eq!(attribute.name_length(), 0);
        assert_eq!(attribute.value_length(), 72);

        // Try to read the actual information.
        let value = attribute.structured_value(&mut testfs1).unwrap();
        let _standard_info = match value {
            NtfsStructuredValue::StandardInformation(standard_info) => standard_info,
            v => panic!("Unexpected NtfsStructuredValue: {:?}", v),
        };

        // There are no reliable values to check here, so that's it.
    }
}
