// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributes;
use crate::error::{NtfsError, Result};
use binread::io::{Read, Seek, SeekFrom};
use binread::{BinRead, BinReaderExt};
use bitflags::bitflags;

#[repr(u64)]
pub enum KnownNtfsFile {
    MFT = 0,
    MFTMirr = 1,
    LogFile = 2,
    Volume = 3,
    AttrDef = 4,
    RootDirectory = 5,
    Bitmap = 6,
    Boot = 7,
    BadClus = 8,
    Secure = 9,
    UpCase = 10,
    Extend = 11,
}

#[allow(unused)]
#[derive(BinRead)]
struct NtfsRecordHeader {
    signature: [u8; 4],
    update_sequence_array_offset: u16,
    update_sequence_array_size: u16,
    logfile_sequence_number: u64,
}

#[allow(unused)]
#[derive(BinRead)]
pub(crate) struct NtfsFileRecordHeader {
    record_header: NtfsRecordHeader,
    sequence_number: u16,
    hard_link_count: u16,
    pub(crate) first_attribute_offset: u16,
    flags: u16,
    pub(crate) used_size: u32,
    allocated_size: u32,
    base_file_record: u64,
    next_attribute_number: u16,
}

bitflags! {
    struct NtfsFileRecordFlags: u16 {
        /// Record is in use.
        const IN_USE = 0x0001;
        /// Record is a directory.
        const IS_DIRECTORY = 0x0002;
    }
}

pub struct NtfsFile {
    pub(crate) header: NtfsFileRecordHeader,
    pub(crate) position: u64,
}

impl NtfsFile {
    pub(crate) fn new<T>(fs: &mut T, position: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        fs.seek(SeekFrom::Start(position))?;
        let header = fs.read_le::<NtfsFileRecordHeader>()?;

        let file = Self { header, position };
        file.validate_signature()?;

        Ok(file)
    }

    pub fn attributes<'a, T>(&self, fs: &'a mut T) -> NtfsAttributes<'a, T>
    where
        T: Read + Seek,
    {
        NtfsAttributes::new(fs, &self)
    }

    fn validate_signature(&self) -> Result<()> {
        let signature = &self.header.record_header.signature;
        let expected = b"FILE";

        if signature == expected {
            Ok(())
        } else {
            Err(NtfsError::InvalidNtfsFileSignature {
                position: self.position,
                expected,
                actual: *signature,
            })
        }
    }
}
