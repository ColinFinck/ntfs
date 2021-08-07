// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributes;
use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use crate::record::{Record, RecordHeader};
use binread::io::{Read, Seek, SeekFrom};
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};
use memoffset::offset_of;

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

#[repr(C, packed)]
struct FileRecordHeader {
    record_header: RecordHeader,
    sequence_number: u16,
    hard_link_count: u16,
    first_attribute_offset: u16,
    flags: u16,
    used_size: u32,
    allocated_size: u32,
    base_file_record: u64,
    next_attribute_number: u16,
}

bitflags! {
    pub struct NtfsFileFlags: u16 {
        /// Record is in use.
        const IN_USE = 0x0001;
        /// Record is a directory.
        const IS_DIRECTORY = 0x0002;
    }
}

#[derive(Debug)]
pub struct NtfsFile<'n> {
    record: Record<'n>,
}

impl<'n> NtfsFile<'n> {
    pub(crate) fn new<T>(ntfs: &'n Ntfs, fs: &mut T, position: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        let mut data = vec![0; ntfs.file_record_size() as usize];
        fs.seek(SeekFrom::Start(position))?;
        fs.read_exact(&mut data)?;

        let mut record = Record::new(ntfs, data, position);
        record.fixup()?;

        let file = Self { record };
        file.validate_signature()?;
        file.validate_sizes()?;

        Ok(file)
    }

    pub fn allocated_size(&self) -> u32 {
        let start = offset_of!(FileRecordHeader, allocated_size);
        LittleEndian::read_u32(&self.record.data()[start..])
    }

    pub fn attributes<'f>(&'f self) -> NtfsAttributes<'n, 'f> {
        NtfsAttributes::new(self)
    }

    pub(crate) fn first_attribute_offset(&self) -> u16 {
        let start = offset_of!(FileRecordHeader, first_attribute_offset);
        LittleEndian::read_u16(&self.record.data()[start..])
    }

    /// Returns flags set for this NTFS file as specified by [`NtfsFileFlags`].
    pub fn flags(&self) -> NtfsFileFlags {
        let start = offset_of!(FileRecordHeader, flags);
        NtfsFileFlags::from_bits_truncate(LittleEndian::read_u16(&self.record.data()[start..]))
    }

    pub fn hard_link_count(&self) -> u16 {
        let start = offset_of!(FileRecordHeader, hard_link_count);
        LittleEndian::read_u16(&self.record.data()[start..])
    }

    pub(crate) fn ntfs(&self) -> &'n Ntfs {
        self.record.ntfs()
    }

    pub fn position(&self) -> u64 {
        self.record.position()
    }

    pub(crate) fn record_data(&self) -> &[u8] {
        self.record.data()
    }

    pub fn sequence_number(&self) -> u16 {
        let start = offset_of!(FileRecordHeader, sequence_number);
        LittleEndian::read_u16(&self.record.data()[start..])
    }

    pub fn used_size(&self) -> u32 {
        let start = offset_of!(FileRecordHeader, used_size);
        LittleEndian::read_u32(&self.record.data()[start..])
    }

    fn validate_signature(&self) -> Result<()> {
        let signature = &self.record.signature();
        let expected = b"FILE";

        if signature == expected {
            Ok(())
        } else {
            Err(NtfsError::InvalidFileSignature {
                position: self.record.position(),
                expected,
                actual: *signature,
            })
        }
    }

    fn validate_sizes(&self) -> Result<()> {
        if self.allocated_size() > self.record.len() {
            return Err(NtfsError::InvalidFileAllocatedSize {
                position: self.record.position(),
                expected: self.allocated_size(),
                actual: self.record.len(),
            });
        }

        if self.used_size() > self.allocated_size() {
            return Err(NtfsError::InvalidFileUsedSize {
                position: self.record.position(),
                expected: self.used_size(),
                actual: self.allocated_size(),
            });
        }

        Ok(())
    }
}
