// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsNonResidentAttributeValue;
use crate::error::{NtfsError, Result};
use crate::file_reference::NtfsFileReference;
use crate::string::NtfsString;
use crate::structured_values::{
    NtfsStructuredValue, NtfsStructuredValueFromNonResidentAttributeValue,
    NtfsStructuredValueFromSlice,
};
use crate::traits::NtfsReadSeek;
use crate::types::Vcn;
use arrayvec::ArrayVec;
use binread::io::{Cursor, Read, Seek, SeekFrom};
use binread::{BinRead, BinReaderExt};
use core::mem;

/// Size of all [`AttributeListEntryHeader`] fields.
const ATTRIBUTE_LIST_ENTRY_HEADER_SIZE: usize = 26;

/// [`AttributeListEntryHeader::name_length`] is an `u8` length field specifying the number of UTF-16 code points.
/// Hence, the name occupies up to 510 bytes.
const NAME_MAX_SIZE: usize = (u8::MAX as usize) * mem::size_of::<u16>();

#[allow(unused)]
#[derive(BinRead, Clone, Debug)]
struct AttributeListEntryHeader {
    /// Type of the attribute, known types are in [`NtfsAttributeType`].
    ty: u32,
    /// Length of this attribute list entry, in bytes.
    list_entry_length: u16,
    /// Length of the name, in UTF-16 code points (every code point is 2 bytes).
    name_length: u8,
    /// Offset to the beginning of the name, in bytes from the beginning of this header.
    name_offset: u8,
    /// Lower boundary of Virtual Cluster Numbers (VCNs) referenced by this attribute.
    /// This becomes relevant when file data is split over multiple attributes.
    /// Otherwise, it's zero.
    lowest_vcn: Vcn,
    /// Reference to the [`NtfsFile`] record where this attribute is stored.
    base_file_reference: NtfsFileReference,
    /// Identifier of this attribute that is unique within the [`NtfsFile`].
    instance: u16,
}

#[derive(Clone, Debug)]
pub enum NtfsAttributeList<'n, 'f> {
    Resident(&'f [u8], u64),
    NonResident(NtfsNonResidentAttributeValue<'n, 'f>),
}

impl<'n, 'f> NtfsAttributeList<'n, 'f> {
    pub fn iter(&self) -> NtfsAttributeListEntries<'n, 'f> {
        NtfsAttributeListEntries::new(self.clone())
    }

    pub fn position(&self) -> u64 {
        match self {
            Self::Resident(_slice, position) => *position,
            Self::NonResident(value) => value.data_position().unwrap(),
        }
    }
}

impl<'n, 'f> NtfsStructuredValue for NtfsAttributeList<'n, 'f> {
    const TY: NtfsAttributeType = NtfsAttributeType::AttributeList;
}

impl<'n, 'f> NtfsStructuredValueFromSlice<'f> for NtfsAttributeList<'n, 'f> {
    fn from_slice(slice: &'f [u8], position: u64) -> Result<Self> {
        Ok(Self::Resident(slice, position))
    }
}

impl<'n, 'f> NtfsStructuredValueFromNonResidentAttributeValue<'n, 'f>
    for NtfsAttributeList<'n, 'f>
{
    fn from_non_resident_attribute_value<T>(
        _fs: &mut T,
        value: NtfsNonResidentAttributeValue<'n, 'f>,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        Ok(Self::NonResident(value))
    }
}

#[derive(Clone, Debug)]
pub struct NtfsAttributeListEntries<'n, 'f> {
    attribute_list: NtfsAttributeList<'n, 'f>,
}

impl<'n, 'f> NtfsAttributeListEntries<'n, 'f> {
    fn new(attribute_list: NtfsAttributeList<'n, 'f>) -> Self {
        Self { attribute_list }
    }

    pub fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsAttributeListEntry>>
    where
        T: Read + Seek,
    {
        match &mut self.attribute_list {
            NtfsAttributeList::Resident(slice, position) => Self::next_resident(slice, position),
            NtfsAttributeList::NonResident(value) => Self::next_non_resident(fs, value),
        }
    }

    pub fn next_non_resident<T>(
        fs: &mut T,
        value: &mut NtfsNonResidentAttributeValue<'n, 'f>,
    ) -> Option<Result<NtfsAttributeListEntry>>
    where
        T: Read + Seek,
    {
        if value.stream_position() >= value.len() {
            return None;
        }

        // Get the current entry.
        let mut value_attached = value.clone().attach(fs);
        let position = value.data_position().unwrap();
        let entry = iter_try!(NtfsAttributeListEntry::new(&mut value_attached, position));

        // Advance our iterator to the next entry.
        iter_try!(value.seek(fs, SeekFrom::Current(entry.list_entry_length() as i64)));

        Some(Ok(entry))
    }

    pub fn next_resident(
        slice: &mut &'f [u8],
        position: &mut u64,
    ) -> Option<Result<NtfsAttributeListEntry>> {
        if slice.is_empty() {
            return None;
        }

        // Get the current entry.
        let mut cursor = Cursor::new(*slice);
        let entry = iter_try!(NtfsAttributeListEntry::new(&mut cursor, *position));

        // Advance our iterator to the next entry.
        let bytes_to_advance = entry.list_entry_length() as usize;
        *slice = &slice[bytes_to_advance..];
        *position += bytes_to_advance as u64;

        Some(Ok(entry))
    }
}

#[derive(Clone, Debug)]
pub struct NtfsAttributeListEntry {
    header: AttributeListEntryHeader,
    name: ArrayVec<u8, NAME_MAX_SIZE>,
    position: u64,
}

impl NtfsAttributeListEntry {
    fn new<T>(r: &mut T, position: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        let header = r.read_le::<AttributeListEntryHeader>()?;

        let mut entry = Self {
            header,
            name: ArrayVec::from([0u8; NAME_MAX_SIZE]),
            position,
        };
        entry.validate_name_length()?;
        entry.read_name(r)?;

        Ok(entry)
    }

    pub fn base_file_reference(&self) -> NtfsFileReference {
        self.header.base_file_reference
    }

    pub fn instance(&self) -> u16 {
        self.header.instance
    }

    pub fn list_entry_length(&self) -> u16 {
        self.header.list_entry_length
    }

    pub fn lowest_vcn(&self) -> Vcn {
        self.header.lowest_vcn
    }

    /// Gets the attribute name and returns it wrapped in an [`NtfsString`].
    pub fn name<'s>(&'s self) -> NtfsString<'s> {
        NtfsString(&self.name)
    }

    /// Returns the file name length, in bytes.
    ///
    /// A file name has a maximum length of 255 UTF-16 code points (510 bytes).
    pub fn name_length(&self) -> usize {
        self.header.name_length as usize * mem::size_of::<u16>()
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    fn read_name<T>(&mut self, r: &mut T) -> Result<()>
    where
        T: Read + Seek,
    {
        debug_assert_eq!(self.name.len(), NAME_MAX_SIZE);

        let name_length = self.name_length();
        r.read_exact(&mut self.name[..name_length])?;
        self.name.truncate(name_length);

        Ok(())
    }

    /// Returns the type of this NTFS attribute, or [`NtfsError::UnsupportedAttributeType`]
    /// if it's an unknown type.
    pub fn ty(&self) -> Result<NtfsAttributeType> {
        NtfsAttributeType::n(self.header.ty).ok_or(NtfsError::UnsupportedAttributeType {
            position: self.position(),
            actual: self.header.ty,
        })
    }

    fn validate_name_length(&self) -> Result<()> {
        let total_size = ATTRIBUTE_LIST_ENTRY_HEADER_SIZE + self.name_length();

        if total_size > self.list_entry_length() as usize {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: self.position(),
                ty: NtfsAttributeType::AttributeList,
                expected: self.list_entry_length() as usize,
                actual: total_size,
            });
        }

        Ok(())
    }
}
