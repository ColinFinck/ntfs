// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::index_entry::{IndexNodeEntryRanges, NtfsIndexNodeEntries};
use crate::index_record::{IndexNodeHeader, INDEX_NODE_HEADER_SIZE};
use crate::structured_values::{NtfsStructuredValue, NtfsStructuredValueFromData};
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

#[derive(Clone, Debug)]
pub struct NtfsIndexRoot<'f> {
    data: &'f [u8],
    position: u64,
}

const LARGE_INDEX_FLAG: u8 = 0x01;

impl<'f> NtfsIndexRoot<'f> {
    pub fn entries(&self) -> Result<NtfsIndexNodeEntries<'f>> {
        let (entries_range, position) = self.entries_range_and_position();
        let data = &self.data[entries_range];

        Ok(NtfsIndexNodeEntries::new(data, position))
    }

    fn entries_range_and_position(&self) -> (Range<usize>, u64) {
        let start = INDEX_ROOT_HEADER_SIZE as usize + self.index_entries_offset() as usize;
        let end = INDEX_ROOT_HEADER_SIZE as usize + self.index_used_size() as usize;
        let position = self.position + start as u64;

        (start..end, position)
    }

    pub(crate) fn entry_ranges(&self) -> IndexNodeEntryRanges {
        let (entries_range, position) = self.entries_range_and_position();
        let entries_data = self.data[entries_range].to_vec();
        let range = 0..entries_data.len();

        IndexNodeEntryRanges::new(entries_data, range, position)
    }

    pub fn index_allocated_size(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, allocated_size);
        LittleEndian::read_u32(&self.data[start..])
    }

    fn index_entries_offset(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, entries_offset);
        LittleEndian::read_u32(&self.data[start..])
    }

    pub fn index_record_size(&self) -> u32 {
        let start = offset_of!(IndexRootHeader, index_record_size);
        LittleEndian::read_u32(&self.data[start..])
    }

    pub fn index_used_size(&self) -> u32 {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, index_size);
        LittleEndian::read_u32(&self.data[start..])
    }

    /// Returns whether the index belonging to this Index Root is large enough
    /// to need an extra Index Allocation attribute.
    /// Otherwise, the entire index information is stored in this Index Root.
    pub fn is_large_index(&self) -> bool {
        let start = INDEX_ROOT_HEADER_SIZE + offset_of!(IndexNodeHeader, flags);
        (self.data[start] & LARGE_INDEX_FLAG) != 0
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    fn validate_sizes(&self) -> Result<()> {
        let (entries_range, _position) = self.entries_range_and_position();

        if entries_range.start >= self.data.len() {
            return Err(NtfsError::InvalidNtfsIndexRootEntriesOffset {
                position: self.position,
                expected: entries_range.start,
                actual: self.data.len(),
            });
        }

        if entries_range.end > self.data.len() {
            return Err(NtfsError::InvalidNtfsIndexRootUsedSize {
                position: self.position,
                expected: entries_range.end,
                actual: self.data.len(),
            });
        }

        Ok(())
    }
}

impl<'f> NtfsStructuredValue for NtfsIndexRoot<'f> {
    const TY: NtfsAttributeType = NtfsAttributeType::IndexRoot;
}

impl<'f> NtfsStructuredValueFromData<'f> for NtfsIndexRoot<'f> {
    fn from_data(data: &'f [u8], position: u64) -> Result<Self> {
        if data.len() < INDEX_ROOT_HEADER_SIZE + INDEX_NODE_HEADER_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::IndexRoot,
                expected: INDEX_ROOT_HEADER_SIZE,
                actual: data.len(),
            });
        }

        let index_root = Self { data, position };
        index_root.validate_sizes()?;

        Ok(index_root)
    }
}
