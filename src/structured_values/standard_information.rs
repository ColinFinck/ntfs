// Copyright 2021-2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use zerocopy::{FromBytes, Immutable, KnownLayout, LittleEndian, Unaligned, U32, U64};

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::{NtfsAttributeValue, NtfsResidentAttributeValue};
use crate::error::{NtfsError, Result};
use crate::helpers::{read_pod, ReadOnlyCursor};
use crate::io::{Read, Seek};
use crate::structured_values::{
    NtfsFileAttributeFlags, NtfsStructuredValue, NtfsStructuredValueFromResidentAttributeValue,
};
use crate::time::NtfsTime;
use crate::types::NtfsPosition;

/// Size of all [`Ntfs1Fields`] fields.
const NTFS1_FIELDS_SIZE: usize = 48;

/// Size of all [`Ntfs3Fields`] fields.
const ADDITIONAL_NTFS3_FIELDS_SIZE: usize = 24;

#[derive(Clone, Debug, FromBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C, packed)]
struct Ntfs1Fields {
    creation_time: NtfsTime,
    modification_time: NtfsTime,
    mft_record_modification_time: NtfsTime,
    access_time: NtfsTime,
    file_attributes: U32<LittleEndian>,
    maximum_versions: U32<LittleEndian>,
    version: U32<LittleEndian>,
    class_id: U32<LittleEndian>,
}

#[derive(Clone, Debug, FromBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C, packed)]
struct Ntfs3Fields {
    owner_id: U32<LittleEndian>,
    security_id: U32<LittleEndian>,
    quota_charged: U64<LittleEndian>,
    usn: U64<LittleEndian>,
}

/// Structure of a $STANDARD_INFORMATION attribute.
///
/// Among other things, this is the place where the file times and "File Attributes"
/// (Read-Only, Hidden, System, Archive, etc.) are stored.
///
/// A $STANDARD_INFORMATION attribute is always resident.
///
/// Reference: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/standard_information.html>
#[derive(Clone, Debug)]
pub struct NtfsStandardInformation {
    ntfs1_data: Ntfs1Fields,
    ntfs3_data: Option<Ntfs3Fields>,
}

impl NtfsStandardInformation {
    fn new<T>(r: &mut T, position: NtfsPosition, value_length: u64) -> Result<Self>
    where
        T: Read,
    {
        if value_length < NTFS1_FIELDS_SIZE as u64 {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::StandardInformation,
                expected: NTFS1_FIELDS_SIZE as u64,
                actual: value_length,
            });
        }

        let ntfs1_data = read_pod::<T, Ntfs1Fields, NTFS1_FIELDS_SIZE>(r)?;

        let mut ntfs3_data = None;
        if value_length >= (NTFS1_FIELDS_SIZE + ADDITIONAL_NTFS3_FIELDS_SIZE) as u64 {
            ntfs3_data = Some(read_pod::<T, Ntfs3Fields, ADDITIONAL_NTFS3_FIELDS_SIZE>(r)?);
        }

        Ok(Self {
            ntfs1_data,
            ntfs3_data,
        })
    }

    /// Returns the time this file was last accessed.
    pub fn access_time(&self) -> NtfsTime {
        self.ntfs1_data.access_time
    }

    /// Returns the Class ID of the file.
    pub fn class_id(&self) -> u32 {
        self.ntfs1_data.class_id.get()
    }

    /// Returns the time this file was created.
    pub fn creation_time(&self) -> NtfsTime {
        self.ntfs1_data.creation_time
    }

    /// Returns flags that a user can set for a file (Read-Only, Hidden, System, Archive, etc.).
    /// Commonly called "File Attributes" in Windows Explorer.
    pub fn file_attributes(&self) -> NtfsFileAttributeFlags {
        NtfsFileAttributeFlags::from_bits_truncate(self.ntfs1_data.file_attributes.get())
    }

    /// Returns the maximum allowed versions for this file.
    ///
    /// A value of zero means that versioning is disabled for this file.
    pub fn maximum_versions(&self) -> u32 {
        self.ntfs1_data.maximum_versions.get()
    }

    /// Returns the time the MFT record of this file was last modified.
    pub fn mft_record_modification_time(&self) -> NtfsTime {
        self.ntfs1_data.mft_record_modification_time
    }

    /// Returns the time this file was last modified.
    pub fn modification_time(&self) -> NtfsTime {
        self.ntfs1_data.modification_time
    }

    /// Returns the Owner ID of the file, if stored via NTFS 3.x file information.
    pub fn owner_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.owner_id.get())
    }

    /// Returns the quota charged by this file, if stored via NTFS 3.x file information.
    pub fn quota_charged(&self) -> Option<u64> {
        self.ntfs3_data.as_ref().map(|x| x.quota_charged.get())
    }

    /// Returns the Security ID of the file, if stored via NTFS 3.x file information.
    pub fn security_id(&self) -> Option<u32> {
        self.ntfs3_data.as_ref().map(|x| x.security_id.get())
    }

    /// Returns the Update Sequence Number (USN) of the file, if stored via NTFS 3.x file information.
    pub fn usn(&self) -> Option<u64> {
        self.ntfs3_data.as_ref().map(|x| x.usn.get())
    }

    /// Returns the version of the file.
    ///
    /// This will be zero if versioning is disabled for this file.
    pub fn version(&self) -> u32 {
        self.ntfs1_data.version.get()
    }
}

impl<'n, 'f> NtfsStructuredValue<'n, 'f> for NtfsStandardInformation {
    const TY: NtfsAttributeType = NtfsAttributeType::StandardInformation;

    fn from_attribute_value<T>(fs: &mut T, value: NtfsAttributeValue<'n, 'f>) -> Result<Self>
    where
        T: Read + Seek,
    {
        let position = value.data_position();
        let value_length = value.len();

        let mut value_attached = value.attach(fs);
        Self::new(&mut value_attached, position, value_length)
    }
}

impl<'n, 'f> NtfsStructuredValueFromResidentAttributeValue<'n, 'f> for NtfsStandardInformation {
    fn from_resident_attribute_value(value: NtfsResidentAttributeValue<'f>) -> Result<Self> {
        let position = value.data_position();
        let value_length = value.len();

        let mut cursor = ReadOnlyCursor::new(value.data());
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
        let attribute = mft_attributes.next().unwrap().unwrap();
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
