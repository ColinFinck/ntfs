// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::{NtfsAttribute, NtfsAttributeType, NtfsAttributes};
use crate::error::{NtfsError, Result};
use crate::index::NtfsIndex;
use crate::indexes::NtfsFileNameIndex;
use crate::ntfs::Ntfs;
use crate::record::{Record, RecordHeader};
use crate::structured_values::{
    NtfsFileName, NtfsIndexAllocation, NtfsIndexRoot, NtfsStandardInformation,
};
use binread::io::{Read, Seek, SeekFrom};
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};
use memoffset::offset_of;

#[repr(u64)]
pub enum KnownNtfsFileRecordNumber {
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
    file_record_number: u64,
}

impl<'n> NtfsFile<'n> {
    pub(crate) fn new<T>(
        ntfs: &'n Ntfs,
        fs: &mut T,
        position: u64,
        file_record_number: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        let mut data = vec![0; ntfs.file_record_size() as usize];
        fs.seek(SeekFrom::Start(position))?;
        fs.read_exact(&mut data)?;

        let mut record = Record::new(ntfs, data, position);
        Self::validate_signature(&record)?;
        record.fixup()?;

        let file = Self {
            record,
            file_record_number,
        };
        file.validate_sizes()?;

        Ok(file)
    }

    pub fn allocated_size(&self) -> u32 {
        let start = offset_of!(FileRecordHeader, allocated_size);
        LittleEndian::read_u32(&self.record.data()[start..])
    }

    /// Returns the first attribute of the given type, or `NtfsError::AttributeNotFound`.
    pub(crate) fn attribute_by_ty<'f>(
        &'f self,
        ty: NtfsAttributeType,
    ) -> Result<NtfsAttribute<'n, 'f>> {
        self.attributes()
            .find(|attribute| {
                // TODO: Replace by attribute.ty().contains() once https://github.com/rust-lang/rust/issues/62358 has landed.
                attribute.ty().map(|x| x == ty).unwrap_or(false)
            })
            .ok_or(NtfsError::AttributeNotFound {
                position: self.position(),
                ty,
            })
    }

    pub fn attributes<'f>(&'f self) -> NtfsAttributes<'n, 'f> {
        NtfsAttributes::new(self)
    }

    /// Convenience function to get a $DATA attribute of this file.
    ///
    /// As NTFS supports multiple data streams per file, you can specify the name of the $DATA attribute
    /// to look up.
    /// Passing an empty string here looks up the default unnamed $DATA attribute (commonly known as the "file data").
    ///
    /// If you need more control over which $DATA attribute is available and picked up,
    /// you can use [`NtfsFile::attributes`] to iterate over all attributes of this file.
    pub fn data<'f>(&'f self, data_stream_name: &str) -> Option<Result<NtfsAttribute<'n, 'f>>> {
        // Create an iterator that emits all $DATA attributes.
        let iter = self.attributes().filter(|attribute| {
            // TODO: Replace by attribute.ty().contains() once https://github.com/rust-lang/rust/issues/62358 has landed.
            attribute
                .ty()
                .map(|ty| ty == NtfsAttributeType::Data)
                .unwrap_or(false)
        });

        for attribute in iter {
            let name = iter_try!(attribute.name());

            if data_stream_name == name {
                return Some(Ok(attribute));
            }
        }

        None
    }

    /// Convenience function to return an [`NtfsIndex`] if this file is a directory.
    ///
    /// Apart from any propagated error, this function may return [`NtfsError::NotADirectory`]
    /// if this [`NtfsFile`] is not a directory.
    ///
    /// If you need more control over the picked up $INDEX_ROOT and $INDEX_ALLOCATION attributes
    /// you can use [`NtfsFile::attributes`] to iterate over all attributes of this file.
    pub fn directory_index<'f, T>(
        &'f self,
        fs: &mut T,
    ) -> Result<NtfsIndex<'n, 'f, NtfsFileNameIndex>>
    where
        T: Read + Seek,
    {
        if !self.flags().contains(NtfsFileFlags::IS_DIRECTORY) {
            return Err(NtfsError::NotADirectory {
                position: self.position(),
            });
        }

        // Get the Index Root attribute that needs to exist.
        let index_root = self
            .attribute_by_ty(NtfsAttributeType::IndexRoot)?
            .resident_structured_value::<NtfsIndexRoot>()?;

        // Get the Index Allocation attribute that is only required for large indexes.
        let index_allocation_attribute = self.attribute_by_ty(NtfsAttributeType::IndexAllocation);
        let index_allocation = if let Ok(attribute) = index_allocation_attribute {
            Some(attribute.non_resident_structured_value::<_, NtfsIndexAllocation>(fs)?)
        } else {
            None
        };

        NtfsIndex::<NtfsFileNameIndex>::new(index_root, index_allocation)
    }

    /// Returns the NTFS file record number of this file.
    ///
    /// This number uniquely identifies this file and can be used to recreate this [`NtfsFile`]
    /// object via [`Ntfs::file`].
    pub fn file_record_number(&self) -> u64 {
        self.file_record_number
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

    /// Convenience function to get the $STANDARD_INFORMATION attribute of this file
    /// (see [`NtfsStandardInformation`]).
    ///
    /// This internally calls [`NtfsFile::attributes`] to iterate through the file's
    /// attributes and pick up the first $STANDARD_INFORMATION attribute.
    pub fn info(&self) -> Result<NtfsStandardInformation> {
        let attribute = self.attribute_by_ty(NtfsAttributeType::StandardInformation)?;
        attribute.resident_structured_value::<NtfsStandardInformation>()
    }

    /// Convenience function to get the $FILE_NAME attribute of this file (see [`NtfsFileName`]).
    ///
    /// This internally calls [`NtfsFile::attributes`] to iterate through the file's
    /// attributes and pick up the first $FILE_NAME attribute.
    pub fn name(&self) -> Result<NtfsFileName> {
        let attribute = self.attribute_by_ty(NtfsAttributeType::FileName)?;
        attribute.resident_structured_value::<NtfsFileName>()
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

    fn validate_signature(record: &Record) -> Result<()> {
        let signature = &record.signature();
        let expected = b"FILE";

        if signature == expected {
            Ok(())
        } else {
            Err(NtfsError::InvalidFileSignature {
                position: record.position(),
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
