// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Result;
use crate::structured_values::NtfsStructuredValueFromSlice;
use crate::types::Vcn;
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};
use core::iter::FusedIterator;
use core::mem;
use core::ops::Range;
use memoffset::offset_of;

/// Size of all [`IndexEntryHeader`] fields plus some reserved bytes.
const INDEX_ENTRY_HEADER_SIZE: i64 = 16;

#[repr(C, packed)]
struct IndexEntryHeader {
    file_ref: u64,
    index_entry_length: u16,
    key_length: u16,
    flags: u8,
}

bitflags! {
    pub struct NtfsIndexEntryFlags: u8 {
        /// This index entry points to a sub-node.
        const HAS_SUBNODE = 0x01;
        /// This is the last index entry in the list.
        const LAST_ENTRY = 0x02;
    }
}

pub(crate) struct IndexEntryRange {
    range: Range<usize>,
    position: u64,
}

impl IndexEntryRange {
    pub(crate) const fn new(range: Range<usize>, position: u64) -> Self {
        Self { range, position }
    }

    pub(crate) fn to_entry<'s>(&self, slice: &'s [u8]) -> NtfsIndexEntry<'s> {
        NtfsIndexEntry::new(&slice[self.range.clone()], self.position)
    }
}

#[derive(Clone, Debug)]
pub struct NtfsIndexEntry<'s> {
    slice: &'s [u8],
    position: u64,
}

impl<'s> NtfsIndexEntry<'s> {
    pub(crate) const fn new(slice: &'s [u8], position: u64) -> Self {
        Self { slice, position }
    }

    pub fn flags(&self) -> NtfsIndexEntryFlags {
        let flags = self.slice[offset_of!(IndexEntryHeader, flags)];
        NtfsIndexEntryFlags::from_bits_truncate(flags)
    }

    pub fn index_entry_length(&self) -> u16 {
        let start = offset_of!(IndexEntryHeader, index_entry_length);
        LittleEndian::read_u16(&self.slice[start..])
    }

    pub fn key_length(&self) -> u16 {
        let start = offset_of!(IndexEntryHeader, key_length);
        LittleEndian::read_u16(&self.slice[start..])
    }

    /// Returns the structured value of the key of this Index Entry,
    /// or `None` if this Index Entry has no key.
    /// The last Index Entry never has a key.
    pub fn key_structured_value<K>(&self) -> Option<Result<K>>
    where
        K: NtfsStructuredValueFromSlice<'s>,
    {
        // The key/stream is only set when the last entry flag is not set.
        // https://flatcap.org/linux-ntfs/ntfs/concepts/index_entry.html
        if self.key_length() == 0 || self.flags().contains(NtfsIndexEntryFlags::LAST_ENTRY) {
            return None;
        }

        let start = INDEX_ENTRY_HEADER_SIZE as usize;
        let end = start + self.key_length() as usize;
        let position = self.position + start as u64;

        let structured_value = iter_try!(K::from_slice(&self.slice[start..end], position));
        Some(Ok(structured_value))
    }

    /// Returns the Virtual Cluster Number (VCN) of the subnode of this Index Entry,
    /// or `None` if this Index Entry has no subnode.
    pub fn subnode_vcn(&self) -> Option<Vcn> {
        if !self.flags().contains(NtfsIndexEntryFlags::HAS_SUBNODE) {
            return None;
        }

        // Get the subnode VCN from the very end of the Index Entry.
        let start = self.index_entry_length() as usize - mem::size_of::<Vcn>();
        let vcn = Vcn::from(LittleEndian::read_i64(&self.slice[start..]));

        Some(vcn)
    }
}

pub(crate) struct IndexNodeEntryRanges {
    data: Vec<u8>,
    range: Range<usize>,
    position: u64,
}

impl IndexNodeEntryRanges {
    pub(crate) fn new(data: Vec<u8>, range: Range<usize>, position: u64) -> Self {
        debug_assert!(range.end <= data.len());

        Self {
            data,
            range,
            position,
        }
    }

    pub(crate) fn data<'d>(&'d self) -> &'d [u8] {
        &self.data
    }
}

impl Iterator for IndexNodeEntryRanges {
    type Item = IndexEntryRange;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.is_empty() {
            return None;
        }

        // Get the current entry.
        let start = self.range.start;
        let position = self.position + self.range.start as u64;
        let entry = NtfsIndexEntry::new(&self.data[start..], position);
        let end = start + entry.index_entry_length() as usize;

        if entry.flags().contains(NtfsIndexEntryFlags::LAST_ENTRY) {
            // This is the last entry.
            // Ensure that we don't read any other entries by advancing `self.range.start` to the end.
            self.range.start = self.data.len();
        } else {
            // This is not the last entry.
            // Advance our iterator to the next entry.
            self.range.start = end;
        }

        Some(IndexEntryRange::new(start..end, position))
    }
}

impl FusedIterator for IndexNodeEntryRanges {}

#[derive(Clone, Debug)]
pub struct NtfsIndexNodeEntries<'s> {
    slice: &'s [u8],
    position: u64,
}

impl<'s> NtfsIndexNodeEntries<'s> {
    pub(crate) fn new(slice: &'s [u8], position: u64) -> Self {
        Self { slice, position }
    }
}

impl<'s> Iterator for NtfsIndexNodeEntries<'s> {
    type Item = NtfsIndexEntry<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() {
            return None;
        }

        // Get the current entry.
        let entry = NtfsIndexEntry::new(self.slice, self.position);

        if entry.flags().contains(NtfsIndexEntryFlags::LAST_ENTRY) {
            // This is the last entry.
            // Ensure that we don't read any other entries by emptying the slice.
            self.slice = &[];
        } else {
            // This is not the last entry.
            // Advance our iterator to the next entry.
            let bytes_to_advance = entry.index_entry_length() as usize;
            self.slice = &self.slice[bytes_to_advance..];
            self.position += bytes_to_advance as u64;
        }

        Some(entry)
    }
}

impl<'s> FusedIterator for NtfsIndexNodeEntries<'s> {}
