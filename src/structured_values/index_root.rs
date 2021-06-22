// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::index_entry::NtfsIndexNodeEntries;
use crate::index_record::{IndexNodeHeader, INDEX_NODE_HEADER_SIZE};
use crate::ntfs::Ntfs;
use crate::structured_values::NewNtfsStructuredValue;
use crate::traits::NtfsReadSeek;
use binread::io::{Read, Seek, SeekFrom};
use binread::{BinRead, BinReaderExt};

/// Size of all [`IndexRootHeader`] fields plus some reserved bytes.
const INDEX_ROOT_HEADER_SIZE: u64 = 16;

#[derive(BinRead, Clone, Debug)]
struct IndexRootHeader {
    ty: u32,
    collation_rule: u32,
    index_record_size: u32,
    clusters_per_index_record: i8,
}

#[derive(Clone, Debug)]
pub struct NtfsIndexRoot<'n> {
    ntfs: &'n Ntfs,
    value: NtfsAttributeValue<'n>,
    index_root_header: IndexRootHeader,
    index_node_header: IndexNodeHeader,
}

const LARGE_INDEX_FLAG: u8 = 0x01;

impl<'n> NtfsIndexRoot<'n> {
    pub fn index_allocated_size(&self) -> u32 {
        self.index_node_header.allocated_size
    }

    pub fn entries<K, T>(&self, fs: &mut T) -> Result<NtfsIndexNodeEntries<'n, K>>
    where
        K: NewNtfsStructuredValue<'n>,
        T: Read + Seek,
    {
        let offset = self.value.stream_position() + INDEX_ROOT_HEADER_SIZE as u64;
        let start = offset + self.index_node_header.entries_offset as u64;
        let end = offset + self.index_used_size() as u64;

        let mut value = self.value.clone();
        value.seek(fs, SeekFrom::Start(start))?;

        Ok(NtfsIndexNodeEntries::new(self.ntfs, value, end))
    }

    pub fn index_record_size(&self) -> u32 {
        self.index_root_header.index_record_size
    }

    pub fn index_used_size(&self) -> u32 {
        self.index_node_header.index_size
    }

    /// Returns whether the index belonging to this Index Root is large enough
    /// to need an extra Index Allocation attribute.
    /// Otherwise, the entire index information is stored in this Index Root.
    pub fn is_large_index(&self) -> bool {
        (self.index_node_header.flags & LARGE_INDEX_FLAG) != 0
    }
}

impl<'n> NewNtfsStructuredValue<'n> for NtfsIndexRoot<'n> {
    fn new<T>(
        ntfs: &'n Ntfs,
        fs: &mut T,
        value: NtfsAttributeValue<'n>,
        _length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value.len() < INDEX_ROOT_HEADER_SIZE + INDEX_NODE_HEADER_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::IndexRoot,
                expected: INDEX_ROOT_HEADER_SIZE,
                actual: value.len(),
            });
        }

        let mut value_attached = value.clone().attach(fs);
        let index_root_header = value_attached.read_le::<IndexRootHeader>()?;
        value_attached.seek(SeekFrom::Start(INDEX_ROOT_HEADER_SIZE))?;
        let index_node_header = value_attached.read_le::<IndexNodeHeader>()?;

        Ok(Self {
            ntfs,
            value,
            index_root_header,
            index_node_header,
        })
    }
}
