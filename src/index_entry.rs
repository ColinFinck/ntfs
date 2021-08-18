// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Result;
use crate::file::NtfsFile;
use crate::file_reference::NtfsFileReference;
use crate::indexes::{
    NtfsIndexEntryData, NtfsIndexEntryHasData, NtfsIndexEntryHasFileReference, NtfsIndexEntryKey,
    NtfsIndexEntryType,
};
use crate::ntfs::Ntfs;
use crate::types::Vcn;
use binread::io::{Read, Seek};
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};
use core::convert::TryInto;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem;
use core::ops::Range;
use memoffset::offset_of;

/// Size of all [`IndexEntryHeader`] fields plus some reserved bytes.
const INDEX_ENTRY_HEADER_SIZE: i64 = 16;

#[repr(C, packed)]
struct IndexEntryHeader {
    // The following three fields are used for the u64 file reference if the entry type
    // has no data, but a file reference instead.
    // This is indicated by the entry type implementing `NtfsIndexEntryHasFileReference`.
    // Currently, only `NtfsFileNameIndex` has such a file reference.
    data_offset: u16,
    data_length: u16,
    padding: u32,

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

#[derive(Clone, Debug)]
pub(crate) struct IndexEntryRange<E>
where
    E: NtfsIndexEntryType,
{
    range: Range<usize>,
    position: u64,
    entry_type: PhantomData<E>,
}

impl<E> IndexEntryRange<E>
where
    E: NtfsIndexEntryType,
{
    pub(crate) fn new(range: Range<usize>, position: u64) -> Self {
        let entry_type = PhantomData;
        Self {
            range,
            position,
            entry_type,
        }
    }

    pub(crate) fn to_entry<'s>(&self, slice: &'s [u8]) -> NtfsIndexEntry<'s, E> {
        NtfsIndexEntry::new(&slice[self.range.clone()], self.position)
    }
}

#[derive(Clone, Debug)]
pub struct NtfsIndexEntry<'s, E>
where
    E: NtfsIndexEntryType,
{
    slice: &'s [u8],
    position: u64,
    entry_type: PhantomData<E>,
}

impl<'s, E> NtfsIndexEntry<'s, E>
where
    E: NtfsIndexEntryType,
{
    pub(crate) fn new(slice: &'s [u8], position: u64) -> Self {
        let entry_type = PhantomData;
        Self {
            slice,
            position,
            entry_type,
        }
    }

    pub fn data(&self) -> Option<Result<E::DataType>>
    where
        E: NtfsIndexEntryHasData,
    {
        if self.data_offset() == 0 || self.data_length() == 0 {
            return None;
        }

        let start = self.data_offset() as usize;
        let end = start + self.data_length() as usize;
        let position = self.position + start as u64;

        let data = iter_try!(E::DataType::data_from_slice(
            &self.slice[start..end],
            position
        ));
        Some(Ok(data))
    }

    fn data_offset(&self) -> u16
    where
        E: NtfsIndexEntryHasData,
    {
        let start = offset_of!(IndexEntryHeader, data_offset);
        LittleEndian::read_u16(&self.slice[start..])
    }

    pub fn data_length(&self) -> u16
    where
        E: NtfsIndexEntryHasData,
    {
        let start = offset_of!(IndexEntryHeader, data_length);
        LittleEndian::read_u16(&self.slice[start..])
    }

    /// Returns an [`NtfsFileReference`] for the file referenced by this index entry.
    pub fn file_reference(&self) -> NtfsFileReference
    where
        E: NtfsIndexEntryHasFileReference,
    {
        // The "file_reference_data" is at the same position as the `data_offset`, `data_length`, and `padding` fields.
        // There can either be extra data or a file reference!
        NtfsFileReference::new(self.slice[..mem::size_of::<u64>()].try_into().unwrap())
    }

    pub fn flags(&self) -> NtfsIndexEntryFlags {
        let flags = self.slice[offset_of!(IndexEntryHeader, flags)];
        NtfsIndexEntryFlags::from_bits_truncate(flags)
    }

    pub fn index_entry_length(&self) -> u16 {
        let start = offset_of!(IndexEntryHeader, index_entry_length);
        LittleEndian::read_u16(&self.slice[start..])
    }

    /// Returns the structured value of the key of this Index Entry,
    /// or `None` if this Index Entry has no key.
    /// The last Index Entry never has a key.
    pub fn key(&self) -> Option<Result<E::KeyType>> {
        // The key/stream is only set when the last entry flag is not set.
        // https://flatcap.org/linux-ntfs/ntfs/concepts/index_entry.html
        if self.key_length() == 0 || self.flags().contains(NtfsIndexEntryFlags::LAST_ENTRY) {
            return None;
        }

        let start = INDEX_ENTRY_HEADER_SIZE as usize;
        let end = start + self.key_length() as usize;
        let position = self.position + start as u64;

        let key = iter_try!(E::KeyType::key_from_slice(
            &self.slice[start..end],
            position
        ));
        Some(Ok(key))
    }

    pub fn key_length(&self) -> u16 {
        let start = offset_of!(IndexEntryHeader, key_length);
        LittleEndian::read_u16(&self.slice[start..])
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

    /// Returns an [`NtfsFile`] for the file referenced by this index entry.
    pub fn to_file<'n, T>(&self, ntfs: &'n Ntfs, fs: &mut T) -> Result<NtfsFile<'n>>
    where
        E: NtfsIndexEntryHasFileReference,
        T: Read + Seek,
    {
        self.file_reference().to_file(ntfs, fs)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IndexNodeEntryRanges<E>
where
    E: NtfsIndexEntryType,
{
    data: Vec<u8>,
    range: Range<usize>,
    position: u64,
    entry_type: PhantomData<E>,
}

impl<E> IndexNodeEntryRanges<E>
where
    E: NtfsIndexEntryType,
{
    pub(crate) fn new(data: Vec<u8>, range: Range<usize>, position: u64) -> Self {
        debug_assert!(range.end <= data.len());
        let entry_type = PhantomData;

        Self {
            data,
            range,
            position,
            entry_type,
        }
    }

    pub(crate) fn data<'d>(&'d self) -> &'d [u8] {
        &self.data
    }
}

impl<E> Iterator for IndexNodeEntryRanges<E>
where
    E: NtfsIndexEntryType,
{
    type Item = IndexEntryRange<E>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.is_empty() {
            return None;
        }

        // Get the current entry.
        let start = self.range.start;
        let position = self.position;
        let entry = NtfsIndexEntry::<E>::new(&self.data[start..], position);
        let end = start + entry.index_entry_length() as usize;

        if entry.flags().contains(NtfsIndexEntryFlags::LAST_ENTRY) {
            // This is the last entry.
            // Ensure that we don't read any other entries by advancing `self.range.start` to the end.
            self.range.start = self.data.len();
        } else {
            // This is not the last entry.
            // Advance our iterator to the next entry.
            self.range.start = end;
            self.position += entry.index_entry_length() as u64;
        }

        Some(IndexEntryRange::new(start..end, position))
    }
}

impl<E> FusedIterator for IndexNodeEntryRanges<E> where E: NtfsIndexEntryType {}

#[derive(Clone, Debug)]
pub struct NtfsIndexNodeEntries<'s, E>
where
    E: NtfsIndexEntryType,
{
    slice: &'s [u8],
    position: u64,
    entry_type: PhantomData<E>,
}

impl<'s, E> NtfsIndexNodeEntries<'s, E>
where
    E: NtfsIndexEntryType,
{
    pub(crate) fn new(slice: &'s [u8], position: u64) -> Self {
        let entry_type = PhantomData;
        Self {
            slice,
            position,
            entry_type,
        }
    }
}

impl<'s, E> Iterator for NtfsIndexNodeEntries<'s, E>
where
    E: NtfsIndexEntryType,
{
    type Item = NtfsIndexEntry<'s, E>;

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

impl<'s, E> FusedIterator for NtfsIndexNodeEntries<'s, E> where E: NtfsIndexEntryType {}
