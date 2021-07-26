// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute_value::{
    NtfsAttributeValue, NtfsNonResidentAttributeValue, NtfsResidentAttributeValue,
};
use crate::error::{NtfsError, Result};
use crate::ntfs_file::NtfsFile;
use crate::string::NtfsString;
use crate::structured_values::{
    NtfsStructuredValueFromNonResidentAttributeValue, NtfsStructuredValueFromSlice,
};
use crate::types::Vcn;
use binread::io::{Read, Seek};
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};
use core::iter::FusedIterator;
use core::mem;
use core::ops::Range;
use enumn::N;
use memoffset::offset_of;

/// On-disk structure of the generic header of an NTFS attribute.
#[repr(C, packed)]
struct NtfsAttributeHeader {
    /// Type of the attribute, known types are in [`NtfsAttributeType`].
    ty: u32,
    /// Length of the resident part of this attribute, in bytes.
    length: u32,
    /// 0 if this attribute has a resident value, 1 if this attribute has a non-resident value.
    is_non_resident: u8,
    /// Length of the name, in UTF-16 code points (every code point is 2 bytes).
    name_length: u8,
    /// Offset to the beginning of the name, in bytes from the beginning of this header.
    name_offset: u16,
    /// Flags of the attribute, known flags are in [`NtfsAttributeFlags`].
    flags: u16,
    /// Identifier of this attribute that is unique within the [`NtfsFile`].
    instance: u16,
}

bitflags! {
    pub struct NtfsAttributeFlags: u16 {
        /// The attribute value is compressed.
        const COMPRESSED = 0x0001;
        /// The attribute value is encrypted.
        const ENCRYPTED = 0x4000;
        /// The attribute value is stored sparsely.
        const SPARSE = 0x8000;
    }
}

/// On-disk structure of the extra header of an NTFS attribute that has a resident value.
#[repr(C, packed)]
struct NtfsResidentAttributeHeader {
    attribute_header: NtfsAttributeHeader,
    /// Length of the value, in bytes.
    value_length: u32,
    /// Offset to the beginning of the value, in bytes from the beginning of the [`NtfsAttributeHeader`].
    value_offset: u16,
    /// 1 if this attribute (with resident value) is referenced in an index.
    indexed_flag: u8,
}

/// On-disk structure of the extra header of an NTFS attribute that has a non-resident value.
#[repr(C, packed)]
struct NtfsNonResidentAttributeHeader {
    attribute_header: NtfsAttributeHeader,
    /// Lower boundary of Virtual Cluster Numbers (VCNs) referenced by this attribute.
    /// This becomes relevant when file data is split over multiple attributes.
    /// Otherwise, it's zero.
    lowest_vcn: Vcn,
    /// Upper boundary of Virtual Cluster Numbers (VCNs) referenced by this attribute.
    /// This becomes relevant when file data is split over multiple attributes.
    /// Otherwise, it's zero (or even -1 for zero-length files according to NTFS-3G).
    highest_vcn: Vcn,
    /// Offset to the beginning of the value data runs.
    data_runs_offset: u16,
    /// Binary exponent denoting the number of clusters in a compression unit.
    /// A typical value is 4, meaning that 2^4 = 16 clusters are part of a compression unit.
    /// A value of zero means no compression (but that should better be determined via
    /// [`NtfsAttributeFlags`]).
    compression_unit_exponent: u8,
    reserved: [u8; 5],
    /// Allocated space for the attribute value, in bytes. This is always a multiple of the cluster size.
    /// For compressed files, this is always a multiple of the compression unit size.
    allocated_size: u64,
    /// Size of the attribute value, in bytes.
    /// This can be larger than `allocated_size` if the value is compressed or stored sparsely.
    data_size: u64,
    /// Size of the initialized part of the attribute value, in bytes.
    /// This is usually the same as `data_size`.
    initialized_size: u64,
}

#[derive(Clone, Copy, Debug, Eq, N, PartialEq)]
#[repr(u32)]
pub enum NtfsAttributeType {
    StandardInformation = 0x10,
    AttributeList = 0x20,
    FileName = 0x30,
    ObjectId = 0x40,
    SecurityDescriptor = 0x50,
    VolumeName = 0x60,
    VolumeInformation = 0x70,
    Data = 0x80,
    IndexRoot = 0x90,
    IndexAllocation = 0xA0,
    Bitmap = 0xB0,
    ReparsePoint = 0xC0,
    EAInformation = 0xD0,
    EA = 0xE0,
    PropertySet = 0xF0,
    LoggedUtilityStream = 0x100,
    End = 0xFFFF_FFFF,
}

#[derive(Debug)]
pub struct NtfsAttribute<'n, 'f> {
    file: &'f NtfsFile<'n>,
    offset: usize,
}

impl<'n, 'f> NtfsAttribute<'n, 'f> {
    fn new(file: &'f NtfsFile<'n>, offset: usize) -> Self {
        Self { file, offset }
    }

    /// Returns the length of this NTFS attribute, in bytes.
    ///
    /// This denotes the length of the attribute structure on disk.
    /// Apart from various headers, this structure also includes the name and,
    /// for resident attributes, the actual value.
    pub fn attribute_length(&self) -> u32 {
        let start = self.offset + offset_of!(NtfsAttributeHeader, length);
        LittleEndian::read_u32(&self.file.record_data()[start..])
    }

    /// Returns flags set for this attribute as specified by [`NtfsAttributeFlags`].
    pub fn flags(&self) -> NtfsAttributeFlags {
        let start = self.offset + offset_of!(NtfsAttributeHeader, flags);
        NtfsAttributeFlags::from_bits_truncate(LittleEndian::read_u16(
            &self.file.record_data()[start..],
        ))
    }

    /// Returns `true` if this is a resident attribute, i.e. one where its value
    /// is part of the attribute structure.
    pub fn is_resident(&self) -> bool {
        let start = self.offset + offset_of!(NtfsAttributeHeader, is_non_resident);
        let is_non_resident = self.file.record_data()[start];
        is_non_resident == 0
    }

    /// Gets the name of this NTFS attribute (if any) and returns it wrapped in an [`NtfsString`].
    ///
    /// Note that most NTFS attributes have no name and are distinguished by their types.
    /// Use [`NtfsAttribute::ty`] to get the attribute type.
    pub fn name(&self) -> Option<Result<NtfsString<'f>>> {
        if self.name_offset() == 0 || self.name_length() == 0 {
            return None;
        }

        iter_try!(self.validate_name_sizes());

        let start = self.offset + self.name_offset() as usize;
        let end = start + self.name_length();
        let string = NtfsString(&self.file.record_data()[start..end]);

        Some(Ok(string))
    }

    fn name_offset(&self) -> u16 {
        let start = self.offset + offset_of!(NtfsAttributeHeader, name_offset);
        LittleEndian::read_u16(&self.file.record_data()[start..])
    }

    /// Returns the length of the name of this NTFS attribute, in bytes.
    ///
    /// An attribute name has a maximum length of 255 UTF-16 code points (510 bytes).
    /// It is always part of the attribute itself and hence also of the length
    /// returned by [`NtfsAttribute::attribute_length`].
    pub fn name_length(&self) -> usize {
        let start = self.offset + offset_of!(NtfsAttributeHeader, name_length);
        let name_length_in_characters = self.file.record_data()[start];
        name_length_in_characters as usize * mem::size_of::<u16>()
    }

    pub fn non_resident_structured_value<T, S>(&self, fs: &mut T) -> Result<S>
    where
        T: Read + Seek,
        S: NtfsStructuredValueFromNonResidentAttributeValue<'n, 'f>,
    {
        let ty = self.ty()?;
        if ty != S::TY {
            return Err(NtfsError::StructuredValueOfDifferentType {
                position: self.position(),
                ty,
            });
        }

        if self.is_resident() {
            return Err(NtfsError::UnexpectedResidentAttribute {
                position: self.position(),
            });
        }

        S::from_non_resident_attribute_value(fs, self.non_resident_value()?)
    }

    fn non_resident_value(&self) -> Result<NtfsNonResidentAttributeValue<'n, 'f>> {
        debug_assert!(!self.is_resident());
        let start = self.offset + self.non_resident_value_data_runs_offset() as usize;
        let end = start + self.attribute_length() as usize;
        let data = &self.file.record_data()[start..end];
        let position = self.file.position() + start as u64;

        NtfsNonResidentAttributeValue::new(
            self.file.ntfs(),
            data,
            position,
            self.non_resident_value_data_size(),
        )
    }

    fn non_resident_value_data_size(&self) -> u64 {
        debug_assert!(!self.is_resident());
        let start = self.offset + offset_of!(NtfsNonResidentAttributeHeader, data_size);
        LittleEndian::read_u64(&self.file.record_data()[start..])
    }

    fn non_resident_value_data_runs_offset(&self) -> u16 {
        debug_assert!(!self.is_resident());
        let start = self.offset + offset_of!(NtfsNonResidentAttributeHeader, data_runs_offset);
        LittleEndian::read_u16(&self.file.record_data()[start..])
    }

    /// Returns the absolute position of this NTFS attribute within the filesystem, in bytes.
    pub fn position(&self) -> u64 {
        self.file.position() + self.offset as u64
    }

    pub fn resident_structured_value<S>(&self) -> Result<S>
    where
        S: NtfsStructuredValueFromSlice<'f>,
    {
        let ty = self.ty()?;
        if ty != S::TY {
            return Err(NtfsError::StructuredValueOfDifferentType {
                position: self.position(),
                ty,
            });
        }

        if !self.is_resident() {
            return Err(NtfsError::UnexpectedNonResidentAttribute {
                position: self.position(),
            });
        }

        let resident_value = self.resident_value()?;
        S::from_slice(resident_value.data(), self.position())
    }

    pub(crate) fn resident_value(&self) -> Result<NtfsResidentAttributeValue<'f>> {
        debug_assert!(self.is_resident());
        self.validate_resident_value_sizes()?;

        let start = self.offset + self.resident_value_offset() as usize;
        let end = start + self.resident_value_length() as usize;
        let data = &self.file.record_data()[start..end];

        Ok(NtfsResidentAttributeValue::new(data, self.position()))
    }

    fn resident_value_length(&self) -> u32 {
        debug_assert!(self.is_resident());
        let start = self.offset + offset_of!(NtfsResidentAttributeHeader, value_length);
        LittleEndian::read_u32(&self.file.record_data()[start..])
    }

    fn resident_value_offset(&self) -> u16 {
        debug_assert!(self.is_resident());
        let start = self.offset + offset_of!(NtfsResidentAttributeHeader, value_offset);
        LittleEndian::read_u16(&self.file.record_data()[start..])
    }

    /// Returns the type of this NTFS attribute, or [`NtfsError::UnsupportedAttributeType`]
    /// if it's an unknown type.
    pub fn ty(&self) -> Result<NtfsAttributeType> {
        let start = self.offset + offset_of!(NtfsAttributeHeader, ty);
        let ty = LittleEndian::read_u32(&self.file.record_data()[start..]);

        NtfsAttributeType::n(ty).ok_or(NtfsError::UnsupportedAttributeType {
            position: self.position(),
            actual: ty,
        })
    }

    fn validate_name_sizes(&self) -> Result<()> {
        let start = self.name_offset();
        if start as u32 >= self.attribute_length() {
            return Err(NtfsError::InvalidAttributeNameOffset {
                position: self.position(),
                expected: start,
                actual: self.attribute_length(),
            });
        }

        let end = start as usize + self.name_length();
        if end > self.attribute_length() as usize {
            return Err(NtfsError::InvalidAttributeNameLength {
                position: self.position(),
                expected: end,
                actual: self.attribute_length(),
            });
        }

        Ok(())
    }

    fn validate_resident_value_sizes(&self) -> Result<()> {
        debug_assert!(self.is_resident());

        let start = self.resident_value_offset();
        if start as u32 >= self.attribute_length() {
            return Err(NtfsError::InvalidResidentAttributeValueOffset {
                position: self.position(),
                expected: start,
                actual: self.attribute_length(),
            });
        }

        let end = start as u32 + self.resident_value_length();
        if end > self.attribute_length() {
            return Err(NtfsError::InvalidResidentAttributeValueLength {
                position: self.position(),
                expected: end,
                actual: self.attribute_length(),
            });
        }

        Ok(())
    }

    /// Returns an [`NtfsAttributeValue`] structure to read the value of this NTFS attribute.
    pub fn value(&self) -> Result<NtfsAttributeValue<'n, 'f>> {
        if self.is_resident() {
            let resident_value = self.resident_value()?;
            Ok(NtfsAttributeValue::Resident(resident_value))
        } else {
            let non_resident_value = self.non_resident_value()?;
            Ok(NtfsAttributeValue::NonResident(non_resident_value))
        }
    }

    /// Returns the length of the value of this NTFS attribute, in bytes.
    pub fn value_length(&self) -> u64 {
        if self.is_resident() {
            self.resident_value_length() as u64
        } else {
            self.non_resident_value_data_size()
        }
    }
}

pub struct NtfsAttributes<'n, 'a> {
    file: &'a NtfsFile<'n>,
    items_range: Range<usize>,
}

impl<'n, 'a> NtfsAttributes<'n, 'a> {
    pub(crate) fn new(file: &'a NtfsFile<'n>) -> Self {
        let start = file.first_attribute_offset() as usize;
        let end = file.used_size() as usize;
        let items_range = start..end;

        Self { file, items_range }
    }
}

impl<'n, 'a> Iterator for NtfsAttributes<'n, 'a> {
    type Item = NtfsAttribute<'n, 'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.items_range.is_empty() {
            return None;
        }

        // This may be an entire attribute or just the 4-byte end marker.
        // Check if this marks the end of the attribute list.
        let ty = LittleEndian::read_u32(&self.file.record_data()[self.items_range.start..]);
        if ty == NtfsAttributeType::End as u32 {
            return None;
        }

        // It's a real attribute.
        let attribute = NtfsAttribute::new(self.file, self.items_range.start);
        self.items_range.start += attribute.attribute_length() as usize;

        Some(attribute)
    }
}

impl<'n, 'a> FusedIterator for NtfsAttributes<'n, 'a> {}
