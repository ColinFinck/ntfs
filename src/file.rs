// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::{NtfsAttributeItem, NtfsAttributeType, NtfsAttributes, NtfsAttributesRaw};
use crate::error::{NtfsError, Result};
use crate::file_reference::NtfsFileReference;
use crate::index::NtfsIndex;
use crate::indexes::NtfsFileNameIndex;
use crate::ntfs::Ntfs;
use crate::record::{Record, RecordHeader};
use crate::structured_values::{
    NtfsFileName, NtfsFileNamespace, NtfsIndexRoot, NtfsStandardInformation,
    NtfsStructuredValueFromResidentAttributeValue,
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
    base_file_record: NtfsFileReference,
    next_attribute_instance: u16,
}

bitflags! {
    pub struct NtfsFileFlags: u16 {
        /// Record is in use.
        const IN_USE = 0x0001;
        /// Record is a directory.
        const IS_DIRECTORY = 0x0002;
    }
}

#[derive(Clone, Debug)]
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

    /// This provides a flattened "data-centric" view of the attributes and abstracts away the filesystem details
    /// to deal with many or large attributes (Attribute Lists and split attributes).
    /// Use [`NtfsFile::attributes_raw`] to iterate over the plain attributes on the filesystem.
    pub fn attributes<'f>(&'f self) -> NtfsAttributes<'n, 'f> {
        NtfsAttributes::<'n, 'f>::new(self)
    }

    pub fn attributes_raw<'f>(&'f self) -> NtfsAttributesRaw<'n, 'f> {
        NtfsAttributesRaw::new(self)
    }

    /// Convenience function to get a $DATA attribute of this file.
    ///
    /// As NTFS supports multiple data streams per file, you can specify the name of the $DATA attribute
    /// to look up.
    /// Passing an empty string here looks up the default unnamed $DATA attribute (commonly known as the "file data").
    ///
    /// If you need more control over which $DATA attribute is available and picked up,
    /// you can use [`NtfsFile::attributes`] to iterate over all attributes of this file.
    pub fn data<'f, T>(
        &'f self,
        fs: &mut T,
        data_stream_name: &str,
    ) -> Option<Result<NtfsAttributeItem<'n, 'f>>>
    where
        T: Read + Seek,
    {
        let mut iter = self.attributes();

        while let Some(item) = iter.next(fs) {
            let item = iter_try!(item);
            let attribute = item.to_attribute();

            let ty = iter_try!(attribute.ty());
            if ty != NtfsAttributeType::Data {
                continue;
            }

            let name = iter_try!(attribute.name());
            if name != data_stream_name {
                continue;
            }

            return Some(Ok(item));
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
        if !self.is_directory() {
            return Err(NtfsError::NotADirectory {
                position: self.position(),
            });
        }

        // A FILE record may contain multiple indexes, so we have to match the name of the directory index.
        let directory_index_name = "$I30";

        // The IndexRoot attribute is always resident and has to exist for every directory.
        let index_root = self.find_resident_attribute_structured_value::<NtfsIndexRoot>(Some(
            directory_index_name,
        ))?;

        // The IndexAllocation attribute is only required for "large" indexes.
        // It is always non-resident and may even be in an AttributeList.
        let mut index_allocation_item = None;
        if index_root.is_large_index() {
            let mut iter = self.attributes();

            while let Some(item) = iter.next(fs) {
                let item = item?;
                let attribute = item.to_attribute();

                let ty = attribute.ty()?;
                if ty != NtfsAttributeType::IndexAllocation {
                    continue;
                }

                let name = attribute.name()?;
                if name != directory_index_name {
                    continue;
                }

                index_allocation_item = Some(item);
                break;
            }
        }

        NtfsIndex::<NtfsFileNameIndex>::new(index_root, index_allocation_item)
    }

    /// Returns the NTFS file record number of this file.
    ///
    /// This number uniquely identifies this file and can be used to recreate this [`NtfsFile`]
    /// object via [`Ntfs::file`].
    pub fn file_record_number(&self) -> u64 {
        self.file_record_number
    }

    /// Finds a resident attribute of a specific type, optionally with a specific name, and returns its structured value.
    /// Returns [`NtfsError::AttributeNotFound`] if no such resident attribute could be found.
    ///
    /// The attribute type is given through the passed structured value type parameter.
    pub(crate) fn find_resident_attribute_structured_value<'f, S>(
        &'f self,
        match_name: Option<&str>,
    ) -> Result<S>
    where
        S: NtfsStructuredValueFromResidentAttributeValue<'n, 'f>,
    {
        // Resident attributes are always stored on the top-level (we don't have to dig into Attribute Lists).
        let attribute = self
            .attributes_raw()
            .find(|attribute| {
                // TODO: Replace by attribute.ty().contains() once https://github.com/rust-lang/rust/issues/62358 has landed.
                let ty_matches = attribute.ty().map(|x| x == S::TY).unwrap_or(false);

                let name_matches = if let Some(name) = match_name {
                    attribute.name().map(|x| x == name).unwrap_or(false)
                } else {
                    true
                };

                ty_matches && name_matches
            })
            .ok_or(NtfsError::AttributeNotFound {
                position: self.position(),
                ty: S::TY,
            })?;
        attribute.resident_structured_value::<S>()
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
    /// This internally calls [`NtfsFile::attributes_raw`] to iterate through the file's
    /// attributes and pick up the first $STANDARD_INFORMATION attribute.
    pub fn info(&self) -> Result<NtfsStandardInformation> {
        self.find_resident_attribute_structured_value::<NtfsStandardInformation>(None)
    }

    pub fn is_directory(&self) -> bool {
        self.flags().contains(NtfsFileFlags::IS_DIRECTORY)
    }

    /// Convenience function to get a $FILE_NAME attribute of this file (see [`NtfsFileName`]).
    ///
    /// A file may have multiple $FILE_NAME attributes for each [`NtfsFileNamespace`].
    /// Files with hard links have further $FILE_NAME attributes for each directory they are in.
    /// You may optionally filter for a namespace and parent directory via the parameters.
    ///
    /// This internally calls [`NtfsFile::attributes`] to iterate through the file's
    /// attributes and pick up the first matching $FILE_NAME attribute.
    pub fn name<T>(
        &self,
        fs: &mut T,
        match_namespace: Option<NtfsFileNamespace>,
        match_parent_record_number: Option<u64>,
    ) -> Option<Result<NtfsFileName>>
    where
        T: Read + Seek,
    {
        let mut iter = self.attributes();

        while let Some(item) = iter.next(fs) {
            let item = iter_try!(item);
            let attribute = item.to_attribute();

            let ty = iter_try!(attribute.ty());
            if ty != NtfsAttributeType::FileName {
                continue;
            }

            let file_name = iter_try!(attribute.structured_value::<_, NtfsFileName>(fs));

            if let Some(namespace) = match_namespace {
                if file_name.namespace() != namespace {
                    continue;
                }
            }

            if let Some(parent_record_number) = match_parent_record_number {
                if file_name.parent_directory_reference().file_record_number()
                    != parent_record_number
                {
                    continue;
                }
            }

            return Some(Ok(file_name));
        }

        None
    }

    /// Returns the [`Ntfs`] object associated to this file.
    pub fn ntfs(&self) -> &'n Ntfs {
        self.record.ntfs()
    }

    /// Returns the absolute byte position of this file record in the NTFS filesystem.
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
