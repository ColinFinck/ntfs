// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use crate::structured_values::NtfsFileAttributeFlags;
use crate::time::NtfsTime;
use binread::io::{Read, Seek};
use binread::{BinRead, BinReaderExt};
use core::mem;
use enumn::N;

/// Size of all [`FileNameHeader`] fields.
const FILE_NAME_HEADER_SIZE: u64 = 66;

/// The smallest FileName attribute has a name containing just a single character.
const FILE_NAME_MIN_SIZE: u64 = FILE_NAME_HEADER_SIZE + mem::size_of::<u16>() as u64;

#[allow(unused)]
#[derive(BinRead, Clone, Debug)]
struct FileNameHeader {
    parent_directory_ref: u64,
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
    name_position: u64,
}

impl NtfsFileName {
    pub(crate) fn new<T>(
        attribute_position: u64,
        mut value_attached: NtfsAttributeValueAttached<'_, '_, T>,
        value_length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < FILE_NAME_MIN_SIZE {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::FileName,
                expected: FILE_NAME_MIN_SIZE,
                actual: value_length,
            });
        }

        let header = value_attached.read_le::<FileNameHeader>()?;
        let name_position = value_attached.position() + FILE_NAME_HEADER_SIZE;

        Ok(Self {
            header,
            name_position,
        })
    }

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

    /// Returns the file name length, in bytes.
    ///
    /// A file name has a maximum length of 255 UTF-16 code points (510 bytes).
    pub fn name_length(&self) -> usize {
        self.header.name_length as usize * mem::size_of::<u16>()
    }

    /// Returns the namespace this name belongs to, or [`NtfsError::UnsupportedNtfsFileNamespace`]
    /// if it's an unknown namespace.
    pub fn namespace(&self) -> Result<NtfsFileNamespace> {
        NtfsFileNamespace::n(self.header.namespace).ok_or(NtfsError::UnsupportedNtfsFileNamespace {
            position: self.name_position,
            actual: self.header.namespace,
        })
    }

    /// Reads the file name into the given buffer, and returns an
    /// [`NtfsString`] wrapping that buffer.
    pub fn read_name<'a, T>(&self, fs: &mut T, buf: &'a mut [u8]) -> Result<NtfsString<'a>>
    where
        T: Read + Seek,
    {
        NtfsString::read_from_fs(fs, self.name_position, self.name_length(), buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfs::Ntfs;
    use crate::ntfs_file::KnownNtfsFile;
    use crate::structured_values::NtfsStructuredValue;
    use crate::time::tests::NT_TIMESTAMP_2021_01_01;

    #[test]
    fn test_file_name() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let mft = ntfs
            .ntfs_file(&mut testfs1, KnownNtfsFile::MFT as u64)
            .unwrap();
        let mut mft_attributes = mft.attributes().attach(&mut testfs1);

        // Check the FileName attribute of the MFT.
        let attribute = mft_attributes.nth(1).unwrap().unwrap();
        assert_eq!(attribute.ty().unwrap(), NtfsAttributeType::FileName);
        assert_eq!(attribute.attribute_length(), 104);
        assert!(attribute.is_resident());
        assert_eq!(attribute.name_length(), 0);
        assert_eq!(attribute.value_length(), 74);

        // Check the actual "file name" of the MFT.
        let value = attribute.structured_value(&mut testfs1).unwrap();
        let file_name = match value {
            NtfsStructuredValue::FileName(file_name) => file_name,
            v => panic!("Unexpected NtfsStructuredValue: {:?}", v),
        };

        let creation_time = file_name.creation_time();
        assert!(*creation_time > NT_TIMESTAMP_2021_01_01);
        assert_eq!(creation_time, file_name.modification_time());
        assert_eq!(creation_time, file_name.mft_record_modification_time());
        assert_eq!(creation_time, file_name.access_time());

        let allocated_size = file_name.allocated_size();
        assert!(allocated_size > 0);
        assert_eq!(allocated_size, file_name.data_size());

        assert_eq!(file_name.name_length(), 8);

        let mut buf = [0u8; 8];
        let file_name_string = file_name.read_name(&mut testfs1, &mut buf).unwrap();

        // Test various ways to compare the same string.
        assert_eq!(file_name_string, "$MFT");
        assert_eq!(file_name_string.to_string_lossy(), String::from("$MFT"));
        assert_eq!(
            file_name_string,
            NtfsString(&[b'$', 0, b'M', 0, b'F', 0, b'T', 0])
        );
    }
}
