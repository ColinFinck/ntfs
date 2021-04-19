// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use crate::string::NtfsString;
use crate::structured_values::NtfsFileAttributeFlags;
use crate::time::NtfsTime;
use binread::io::{Read, Seek, SeekFrom};
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
        mut value_attached: NtfsAttributeValueAttached<'_, T>,
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
        let name_length = self.name_length();
        if buf.len() < name_length {
            return Err(NtfsError::BufferTooSmall {
                expected: name_length,
                actual: buf.len(),
            });
        }

        fs.seek(SeekFrom::Start(self.name_position))?;
        fs.read_exact(&mut buf[..name_length])?;

        Ok(NtfsString(&buf[..name_length]))
    }
}
