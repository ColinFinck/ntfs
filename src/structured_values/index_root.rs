// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::{NtfsAttributeValue, NtfsResidentAttributeValue};
use crate::error::{NtfsError, Result};
use crate::index_entry::{IndexNodeEntryRanges, NtfsIndexNodeEntries};
use crate::index_record::{IndexNodeHeader, INDEX_NODE_HEADER_SIZE};
use crate::indexes::NtfsIndexEntryType;
use crate::structured_values::{
    NtfsStructuredValue, NtfsStructuredValueFromResidentAttributeValue,
};
use binread::io::{Read, Seek};
use byteorder::{ByteOrder, LittleEndian};
use core::ops::Range;
use memoffset::offset_of;

/// Size of all [`IndexRootHeader`] fields plus some reserved bytes.
const INDEX_ROOT_HEADER_SIZE: usize = 16;

#[repr(C, packed)]
struct IndexRootHeader {
    ty: u32,
    collation_rule: u32,
    index_record_size: u32,
    clusters_per_index_record: i8,
}

/// Structure of an $INDEX_ROOT attribute.
///
/// This attribute describes the top-level nodes of a B-tree.
/// The sub-nodes are managed via [`NtfsIndexAllocation`].
///
/// NTFS uses B-trees for describing directories (as indexes of [`NtfsFileName`]s), looking up Object IDs,
/// Reparse Points, and Security Descriptors, to just name a few.
///
/// An $INDEX_ROOT attribute is always resident.
///
/// Reference: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/index_root.html>
///
/// [`NtfsFileName`]: crate::structured_values::NtfsFileName
/// [`NtfsIndexAllocation`]: crate::structured_values::NtfsIndexAllocation
#[derive(Clone, Debug)]
pub struct NtfsIndexRoot<'f> {
    slice: &'f [u8],
    position: u64,
}

const LARGE_INDEX_FLAG: u8 = 0x01;

impl<'f> NtfsIndexRoot<'f> {
    fn new(slice: &'f [u8], position: u64) -> Result<Self> {
        if slice.len() < INDEX_ROOT_HEADER_SIZE + INDEX_NODE_HEADER_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::IndexRoot,
                expected: INDEX_ROOT_HEADER_SIZE as u64,
                actual: slice.len() as u64,
            });
        }

        let index_root = Self { slice, position };
        index_root.validate_sizes()?;

        Ok(index_root)
    }

    /// Returns an iterator over all top-level nodes of the B-tree.
    pub fn entries<E>(&self) -> Result<NtfsIndexNodeEntries<'f, E>>
    where
        E: NtfsIndexEntryType,
    {
        let (entries_range, position) = self.entries_range_and_position();
        let slice = &self.slice[entries_range];

        Ok(NtfsIndexNodeEntries::new(slice, position))
    }

    fn entries_range_and_position(&self) -> (Range<usize>, u64) {
        let start = INDEX_ROOT_HEADER_SIZE as usize + self.index_entries_offset() as usize;
        let end = INDEX_ROOT_HEADER_SIZE as usize + self.index_data_size() as usize;
        let position = self.position + start as u64;

        (start..end, position)
    }

    pub(crate) fn entry_ranges<E>(&self) -> IndexNodeEntryRanges<E>
    where
        E: NtfsIndexEntryType,
    {
        let (entries_range, position) = self.entries_range_and_position();
        let entries_data = self.slice[entries_range].to_vec();
        let range = 0..entries_data.len();

        IndexNodeEntryRanges::new(entries_data, range, position)
    }

    /// Returns the allocated size of this NTFS Index Root, in bytes.
    pub fn index_allocated_size(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, allocated_size);
        LittleEndian::read_u32(&self.slice[start..])
    }

    /// Returns the size actually used by index data within this NTFS Index Root, in bytes.
    pub fn index_data_size(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, index_size);
        LittleEndian::read_u32(&self.slice[start..])
    }

    fn index_entries_offset(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, entries_offset);
        LittleEndian::read_u32(&self.slice[start..])
    }

    /// Returns the size of a single Index Record, in bytes.
    pub fn index_record_size(&self) -> u32 {
        let start = offset_of!(IndexRootHeader, index_record_size);
        LittleEndian::read_u32(&self.slice[start..])
    }

    /// Returns whether the index belonging to this Index Root is large enough
    /// to need an extra Index Allocation attribute.
    /// Otherwise, the entire index information is stored in this Index Root.
    pub fn is_large_index(&self) -> bool {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, flags);
        (self.slice[start] & LARGE_INDEX_FLAG) != 0
    }

    /// Returns the absolute position of this Index Root within the filesystem, in bytes.
    pub fn position(&self) -> u64 {
        self.position
    }

    fn validate_sizes(&self) -> Result<()> {
        let (entries_range, _position) = self.entries_range_and_position();

        if entries_range.start >= self.slice.len() {
            return Err(NtfsError::InvalidIndexRootEntriesOffset {
                position: self.position,
                expected: entries_range.start,
                actual: self.slice.len(),
            });
        }

        if entries_range.end > self.slice.len() {
            return Err(NtfsError::InvalidIndexRootUsedSize {
                position: self.position,
                expected: entries_range.end,
                actual: self.slice.len(),
            });
        }

        Ok(())
    }
}

impl<'n, 'f> NtfsStructuredValue<'n, 'f> for NtfsIndexRoot<'f> {
    const TY: NtfsAttributeType = NtfsAttributeType::IndexRoot;

    fn from_attribute_value<T>(_fs: &mut T, value: NtfsAttributeValue<'n, 'f>) -> Result<Self>
    where
        T: Read + Seek,
    {
        let resident_value = match value {
            NtfsAttributeValue::Resident(resident_value) => resident_value,
            _ => {
                let position = value.data_position().unwrap();
                return Err(NtfsError::UnexpectedNonResidentAttribute { position });
            }
        };

        let position = resident_value.data_position().unwrap();
        Self::new(resident_value.data(), position)
    }
}

impl<'n, 'f> NtfsStructuredValueFromResidentAttributeValue<'n, 'f> for NtfsIndexRoot<'f> {
    fn from_resident_attribute_value(value: NtfsResidentAttributeValue<'f>) -> Result<Self> {
        Self::new(value.data(), value.data_position().unwrap())
    }
}
