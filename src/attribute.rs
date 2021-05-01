// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute_value::{NtfsAttributeResidentValue, NtfsAttributeValue};
use crate::error::{NtfsError, Result};
use crate::ntfs_file::NtfsFile;
use crate::string::NtfsString;
use crate::structured_values::{
    NtfsFileName, NtfsObjectId, NtfsStandardInformation, NtfsStructuredValue,
    NtfsVolumeInformation, NtfsVolumeName,
};
use binread::io::{Read, Seek, SeekFrom};
use binread::{BinRead, BinReaderExt};
use bitflags::bitflags;
use core::iter::FusedIterator;
use core::mem;
use core::ops::Range;
use enumn::N;

/// On-disk structure of the generic header of an NTFS attribute.
#[allow(unused)]
#[derive(BinRead)]
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

impl NtfsAttributeHeader {
    fn is_resident(&self) -> bool {
        self.is_non_resident == 0
    }
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
#[allow(unused)]
#[derive(BinRead)]
struct NtfsAttributeResidentHeader {
    /// Length of the value, in bytes.
    value_length: u32,
    /// Offset to the beginning of the value, in bytes from the beginning of the [`NtfsAttributeHeader`].
    value_offset: u16,
    /// 1 if this attribute (with resident value) is referenced in an index.
    indexed_flag: u8,
}

/// On-disk structure of the extra header of an NTFS attribute that has a non-resident value.
#[allow(unused)]
#[derive(BinRead)]
struct NtfsAttributeNonResidentHeader {
    /// Lower boundary of Virtual Cluster Numbers (VCNs) referenced by this attribute.
    /// This becomes relevant when file data is split over multiple attributes.
    /// Otherwise, it's zero.
    lowest_vcn: i64,
    /// Upper boundary of Virtual Cluster Numbers (VCNs) referenced by this attribute.
    /// This becomes relevant when file data is split over multiple attributes.
    /// Otherwise, it's zero (or even -1 for zero-length files according to NTFS-3G).
    highest_vcn: i64,
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
    allocated_size: i64,
    /// Size of the attribute value, in bytes.
    /// This can be larger than `allocated_size` if the value is compressed or stored sparsely.
    data_size: i64,
    /// Size of the initialized part of the attribute value, in bytes.
    /// This is usually the same as `data_size`.
    initialized_size: i64,
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

enum NtfsAttributeExtraHeader {
    Resident(NtfsAttributeResidentHeader),
    NonResident(NtfsAttributeNonResidentHeader),
}

impl NtfsAttributeExtraHeader {
    fn new<T>(fs: &mut T, header: &NtfsAttributeHeader) -> Result<Self>
    where
        T: Read + Seek,
    {
        if header.is_resident() {
            // Read the resident header.
            let resident_header = fs.read_le::<NtfsAttributeResidentHeader>()?;
            Ok(Self::Resident(resident_header))
        } else {
            // Read the non-resident header.
            let non_resident_header = fs.read_le::<NtfsAttributeNonResidentHeader>()?;
            Ok(Self::NonResident(non_resident_header))
        }
    }
}

pub struct NtfsAttribute {
    position: u64,
    header: NtfsAttributeHeader,
    extra_header: NtfsAttributeExtraHeader,
}

impl NtfsAttribute {
    fn new<T>(fs: &mut T, position: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        // Read the common header for resident and non-resident attributes.
        fs.seek(SeekFrom::Start(position))?;
        let header = fs.read_le::<NtfsAttributeHeader>()?;

        // This must be a real attribute and not an end marker!
        // The caller must have already checked for potential end markers.
        debug_assert!(header.ty != NtfsAttributeType::End as u32);

        // Read the extra header specific to the attribute type.
        let extra_header = NtfsAttributeExtraHeader::new(fs, &header)?;

        let attribute = Self {
            position,
            header,
            extra_header,
        };
        Ok(attribute)
    }

    /// Returns the length of this NTFS attribute, in bytes.
    ///
    /// This denotes the length of the attribute structure on disk.
    /// Apart from various headers, this structure also includes the name and,
    /// for resident attributes, the actual value.
    pub fn attribute_length(&self) -> u32 {
        self.header.length
    }

    /// Returns flags set for this attribute as specified by [`NtfsAttributeFlags`].
    pub fn flags(&self) -> NtfsAttributeFlags {
        NtfsAttributeFlags::from_bits_truncate(self.header.flags)
    }

    /// Returns `true` if this is a resident attribute, i.e. one where its value
    /// is part of the attribute structure.
    pub fn is_resident(&self) -> bool {
        self.header.is_resident()
    }

    /// Returns the length of the name of this NTFS attribute, in bytes.
    ///
    /// An attribute name has a maximum length of 255 UTF-16 code points (510 bytes).
    /// It is always part of the attribute itself and hence also of the length
    /// returned by [`NtfsAttribute::attribute_length`].
    pub fn name_length(&self) -> usize {
        self.header.name_length as usize * mem::size_of::<u16>()
    }

    /// Returns the absolute position of this NTFS attribute within the filesystem, in bytes.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Reads the name of this NTFS attribute into the given buffer, and returns an
    /// [`NtfsString`] wrapping that buffer.
    ///
    /// Note that most NTFS attributes have no name and are distinguished by their types.
    /// Use [`NtfsAttribute::ty`] to get the attribute type.
    pub fn read_name<'a, T>(&self, fs: &mut T, buf: &'a mut [u8]) -> Result<NtfsString<'a>>
    where
        T: Read + Seek,
    {
        let name_position = self.position + self.header.name_offset as u64;
        NtfsString::read_from_fs(fs, name_position, self.name_length(), buf)
    }

    pub fn structured_value<T>(&self, fs: &mut T) -> Result<NtfsStructuredValue>
    where
        T: Read + Seek,
    {
        let attached_value = self.value().attach(fs);

        match self.ty()? {
            NtfsAttributeType::StandardInformation => {
                let inner = NtfsStandardInformation::new(
                    self.position,
                    attached_value,
                    self.value_length(),
                )?;
                Ok(NtfsStructuredValue::StandardInformation(inner))
            }
            NtfsAttributeType::AttributeList => panic!("TODO"),
            NtfsAttributeType::FileName => {
                let inner = NtfsFileName::new(self.position, attached_value, self.value_length())?;
                Ok(NtfsStructuredValue::FileName(inner))
            }
            NtfsAttributeType::ObjectId => {
                let inner = NtfsObjectId::new(self.position, attached_value, self.value_length())?;
                Ok(NtfsStructuredValue::ObjectId(inner))
            }
            NtfsAttributeType::SecurityDescriptor => panic!("TODO"),
            NtfsAttributeType::VolumeName => {
                let inner =
                    NtfsVolumeName::new(self.position, attached_value, self.value_length())?;
                Ok(NtfsStructuredValue::VolumeName(inner))
            }
            NtfsAttributeType::VolumeInformation => {
                let inner =
                    NtfsVolumeInformation::new(self.position, attached_value, self.value_length())?;
                Ok(NtfsStructuredValue::VolumeInformation(inner))
            }
            ty => Err(NtfsError::UnsupportedStructuredValue {
                position: self.position,
                ty,
            }),
        }
    }

    /// Returns the type of this NTFS attribute, or [`NtfsError::UnsupportedNtfsAttributeType`]
    /// if it's an unknown type.
    pub fn ty(&self) -> Result<NtfsAttributeType> {
        NtfsAttributeType::n(self.header.ty).ok_or(NtfsError::UnsupportedNtfsAttributeType {
            position: self.position,
            actual: self.header.ty,
        })
    }

    /// Returns an [`NtfsAttributeValue`] structure to read the value of this NTFS attribute.
    pub fn value(&self) -> NtfsAttributeValue {
        match &self.extra_header {
            NtfsAttributeExtraHeader::Resident(resident_header) => {
                let value_position = self.position + resident_header.value_offset as u64;
                let value_length = resident_header.value_length;
                let value = NtfsAttributeResidentValue::new(value_position, value_length);
                NtfsAttributeValue::Resident(value)
            }
            NtfsAttributeExtraHeader::NonResident(_non_resident_header) => {
                panic!("TODO")
            }
        }
    }

    /// Returns the length of the value of this NTFS attribute, in bytes.
    pub fn value_length(&self) -> u64 {
        match &self.extra_header {
            NtfsAttributeExtraHeader::Resident(resident_header) => {
                resident_header.value_length as u64
            }
            NtfsAttributeExtraHeader::NonResident(non_resident_header) => {
                non_resident_header.data_size as u64
            }
        }
    }
}

pub struct NtfsAttributes<'a, T: Read + Seek> {
    fs: &'a mut T,
    items_range: Range<u64>,
}

impl<'a, T> NtfsAttributes<'a, T>
where
    T: Read + Seek,
{
    pub(crate) fn new(fs: &'a mut T, file: &NtfsFile) -> Self {
        let start = file.position() + file.first_attribute_offset() as u64;
        let end = file.position() + file.used_size() as u64;
        let items_range = start..end;

        Self { fs, items_range }
    }
}

impl<'a, T> Iterator for NtfsAttributes<'a, T>
where
    T: Read + Seek,
{
    type Item = Result<NtfsAttribute>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.items_range.is_empty() {
            return None;
        }

        // This may be an entire attribute or just the 4-byte end marker.
        // Check if this marks the end of the attribute list.
        let position = self.items_range.start;
        iter_try!(self.fs.seek(SeekFrom::Start(position)));
        let ty = iter_try!(self.fs.read_le::<u32>());
        if ty == NtfsAttributeType::End as u32 {
            return None;
        }

        // It's a real attribute.
        let attribute = iter_try!(NtfsAttribute::new(&mut self.fs, position));
        self.items_range.start += attribute.attribute_length() as u64;

        Some(Ok(attribute))
    }
}

impl<'a, T> FusedIterator for NtfsAttributes<'a, T> where T: Read + Seek {}
