// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::file_reference::NtfsFileReference;
use crate::indexes::NtfsIndexEntryKey;
use crate::string::NtfsString;
use crate::structured_values::{
    NtfsFileAttributeFlags, NtfsStructuredValue, NtfsStructuredValueFromSlice,
};
use crate::time::NtfsTime;
use arrayvec::ArrayVec;
use binread::io::Cursor;
use binread::{BinRead, BinReaderExt};
use core::mem;
use enumn::N;

/// Size of all [`FileNameHeader`] fields.
const FILE_NAME_HEADER_SIZE: usize = 66;

/// The smallest FileName attribute has a name containing just a single character.
const FILE_NAME_MIN_SIZE: usize = FILE_NAME_HEADER_SIZE + mem::size_of::<u16>();

/// The "name" stored in the FileName attribute has an `u8` length field specifying the number of UTF-16 code points.
/// Hence, the name occupies up to 510 bytes.
const NAME_MAX_SIZE: usize = (u8::MAX as usize) * mem::size_of::<u16>();

#[allow(unused)]
#[derive(BinRead, Clone, Debug)]
struct FileNameHeader {
    parent_directory_reference: NtfsFileReference,
    creation_time: NtfsTime,
    modification_time: NtfsTime,
    mft_record_modification_time: NtfsTime,
    access_time: NtfsTime,
    allocated_size: u64,
    data_size: u64,
    file_attributes: u32,
    reparse_point_tag: u32,
    name_length: u8,
    namespace: u8,
}

#[derive(Clone, Copy, Debug, Eq, N, PartialEq)]
#[repr(u8)]
pub enum NtfsFileNamespace {
    Posix = 0,
    Win32 = 1,
    Dos = 2,
    Win32AndDos = 3,
}

#[derive(Clone, Debug)]
pub struct NtfsFileName {
    header: FileNameHeader,
    name: ArrayVec<u8, NAME_MAX_SIZE>,
}

impl NtfsFileName {
    pub fn access_time(&self) -> NtfsTime {
        self.header.access_time
    }

    pub fn allocated_size(&self) -> u64 {
        self.header.allocated_size
    }

    pub fn creation_time(&self) -> NtfsTime {
        self.header.creation_time
    }

    pub fn data_size(&self) -> u64 {
        self.header.data_size
    }

    pub fn file_attributes(&self) -> NtfsFileAttributeFlags {
        NtfsFileAttributeFlags::from_bits_truncate(self.header.file_attributes)
    }

    pub fn mft_record_modification_time(&self) -> NtfsTime {
        self.header.mft_record_modification_time
    }

    pub fn modification_time(&self) -> NtfsTime {
        self.header.modification_time
    }

    /// Gets the file name and returns it wrapped in an [`NtfsString`].
    pub fn name<'s>(&'s self) -> NtfsString<'s> {
        NtfsString(&self.name)
    }

    /// Returns the file name length, in bytes.
    ///
    /// A file name has a maximum length of 255 UTF-16 code points (510 bytes).
    pub fn name_length(&self) -> usize {
        self.header.name_length as usize * mem::size_of::<u16>()
    }

    /// Returns the namespace this name belongs to, or [`NtfsError::UnsupportedFileNamespace`]
    /// if it's an unknown namespace.
    pub fn namespace(&self) -> NtfsFileNamespace {
        NtfsFileNamespace::n(self.header.namespace).unwrap()
    }

    pub fn parent_directory_reference(&self) -> NtfsFileReference {
        self.header.parent_directory_reference
    }

    fn read_name(&mut self, data: &[u8]) {
        debug_assert!(self.name.is_empty());
        let start = FILE_NAME_HEADER_SIZE;
        let end = start + self.name_length();
        self.name.try_extend_from_slice(&data[start..end]).unwrap();
    }

    fn validate_name_length(&self, data_size: usize, position: u64) -> Result<()> {
        let total_size = FILE_NAME_HEADER_SIZE + self.name_length();

        if total_size > data_size {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: position,
                ty: NtfsAttributeType::FileName,
                expected: data_size,
                actual: total_size,
            });
        }

        Ok(())
    }

    fn validate_namespace(&self, position: u64) -> Result<()> {
        if NtfsFileNamespace::n(self.header.namespace).is_none() {
            return Err(NtfsError::UnsupportedFileNamespace {
                position,
                actual: self.header.namespace,
            });
        }

        Ok(())
    }
}

impl NtfsStructuredValue for NtfsFileName {
    const TY: NtfsAttributeType = NtfsAttributeType::FileName;
}

impl<'s> NtfsStructuredValueFromSlice<'s> for NtfsFileName {
    fn from_slice(slice: &'s [u8], position: u64) -> Result<Self> {
        if slice.len() < FILE_NAME_MIN_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::FileName,
                expected: FILE_NAME_MIN_SIZE,
                actual: slice.len(),
            });
        }

        let mut cursor = Cursor::new(slice);
        let header = cursor.read_le::<FileNameHeader>()?;

        let mut file_name = Self {
            header,
            name: ArrayVec::new(),
        };
        file_name.validate_name_length(slice.len(), position)?;
        file_name.validate_namespace(position)?;
        file_name.read_name(slice);

        Ok(file_name)
    }
}

// `NtfsFileName` is special in the regard that the index entry key has the same structure as the structured value.
impl NtfsIndexEntryKey for NtfsFileName {
    fn key_from_slice(slice: &[u8], position: u64) -> Result<Self> {
        Self::from_slice(slice, position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::KnownNtfsFile;
    use crate::ntfs::Ntfs;
    use crate::time::tests::NT_TIMESTAMP_2021_01_01;

    #[test]
    fn test_file_name() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let mft = ntfs.file(&mut testfs1, KnownNtfsFile::MFT as u64).unwrap();
        let mut mft_attributes = mft.attributes();

        // Check the FileName attribute of the MFT.
        let attribute = mft_attributes.nth(1).unwrap();
        assert_eq!(attribute.ty().unwrap(), NtfsAttributeType::FileName);
        assert_eq!(attribute.attribute_length(), 104);
        assert!(attribute.is_resident());
        assert_eq!(attribute.name_length(), 0);
        assert_eq!(attribute.value_length(), 74);

        // Check the actual "file name" of the MFT.
        let file_name = attribute
            .resident_structured_value::<NtfsFileName>()
            .unwrap();

        let creation_time = file_name.creation_time();
        assert!(*creation_time > NT_TIMESTAMP_2021_01_01);
        assert_eq!(creation_time, file_name.modification_time());
        assert_eq!(creation_time, file_name.mft_record_modification_time());
        assert_eq!(creation_time, file_name.access_time());

        let allocated_size = file_name.allocated_size();
        assert!(allocated_size > 0);
        assert_eq!(allocated_size, file_name.data_size());

        assert_eq!(file_name.name_length(), 8);

        // Test various ways to compare the same string.
        assert_eq!(file_name.name(), "$MFT");
        assert_eq!(file_name.name().to_string_lossy(), String::from("$MFT"));
        assert_eq!(
            file_name.name(),
            NtfsString(&[b'$', 0, b'M', 0, b'F', 0, b'T', 0])
        );
    }
}
