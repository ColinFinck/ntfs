// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::index_entry::NtfsIndexEntries;
use crate::ntfs::Ntfs;
use crate::record::RecordHeader;
use crate::structured_values::NewNtfsStructuredValue;
use crate::traits::NtfsReadSeek;
use crate::types::Vcn;
use binread::io::{Read, Seek, SeekFrom};
use binread::{BinRead, BinReaderExt};

/// Size of all [`IndexRecordHeader`] fields.
const INDEX_RECORD_HEADER_SIZE: u32 = 24;

#[allow(unused)]
#[derive(BinRead, Clone, Debug)]
struct IndexRecordHeader {
    record_header: RecordHeader,
    vcn: Vcn,
}

/// Size of all [`IndexNodeHeader`] fields plus some reserved bytes.
pub(crate) const INDEX_NODE_HEADER_SIZE: u64 = 16;

#[derive(BinRead, Clone, Debug)]
pub(crate) struct IndexNodeHeader {
    pub(crate) entries_offset: u32,
    pub(crate) index_size: u32,
    pub(crate) allocated_size: u32,
    pub(crate) flags: u8,
}

#[derive(Clone, Debug)]
pub struct NtfsIndexRecord<'n> {
    ntfs: &'n Ntfs,
    value: NtfsAttributeValue<'n>,
    index_record_header: IndexRecordHeader,
    index_node_header: IndexNodeHeader,
}

const HAS_SUBNODES_FLAG: u8 = 0x01;

impl<'n> NtfsIndexRecord<'n> {
    pub(crate) fn new<T>(
        ntfs: &'n Ntfs,
        fs: &mut T,
        value: NtfsAttributeValue<'n>,
        index_record_size: u32,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        let mut value_attached = value.clone().attach(fs);
        let index_record_header = value_attached.read_le::<IndexRecordHeader>()?;
        let index_node_header = value_attached.read_le::<IndexNodeHeader>()?;

        let index_record = Self {
            ntfs,
            value,
            index_record_header,
            index_node_header,
        };
        index_record.validate_signature()?;
        index_record.validate_sizes(index_record_size)?;

        Ok(index_record)
    }

    pub fn entries<K, T>(&self, fs: &mut T) -> Result<NtfsIndexEntries<'n, K>>
    where
        K: NewNtfsStructuredValue<'n>,
        T: Read + Seek,
    {
        let offset = self.value.stream_position() + INDEX_RECORD_HEADER_SIZE as u64;
        let start = offset + self.index_node_header.entries_offset as u64;
        let end = offset + self.index_used_size() as u64;

        let mut value = self.value.clone();
        value.seek(fs, SeekFrom::Start(start))?;

        Ok(NtfsIndexEntries::new(self.ntfs, value, end))
    }

    /// Returns whether this index node has sub-nodes.
    /// Otherwise, this index node is a leaf node.
    pub fn has_subnodes(&self) -> bool {
        (self.index_node_header.flags & HAS_SUBNODES_FLAG) != 0
    }

    pub fn index_allocated_size(&self) -> u32 {
        self.index_node_header.allocated_size
    }

    pub fn index_used_size(&self) -> u32 {
        self.index_node_header.index_size
    }

    fn validate_signature(&self) -> Result<()> {
        let signature = &self.index_record_header.record_header.signature;
        let expected = b"INDX";

        if signature == expected {
            Ok(())
        } else {
            Err(NtfsError::InvalidNtfsIndexSignature {
                position: self.value.data_position().unwrap(),
                expected,
                actual: *signature,
            })
        }
    }

    fn validate_sizes(&self, index_record_size: u32) -> Result<()> {
        // The total size allocated for this index record must not be larger than
        // the size defined for all index records of this index.
        let total_allocated_size = INDEX_RECORD_HEADER_SIZE + self.index_allocated_size();
        if total_allocated_size > index_record_size {
            return Err(NtfsError::InvalidNtfsIndexSize {
                position: self.value.data_position().unwrap(),
                expected: index_record_size,
                actual: total_allocated_size,
            });
        }

        // Furthermore, the total used size for this index record must not be
        // larger than the total allocated size.
        let total_used_size = INDEX_RECORD_HEADER_SIZE
            + self
                .index_record_header
                .record_header
                .update_sequence_array_size()
            + self.index_used_size();
        if total_used_size > total_allocated_size {
            return Err(NtfsError::InvalidNtfsIndexSize {
                position: self.value.data_position().unwrap(),
                expected: total_allocated_size,
                actual: total_used_size,
            });
        }

        Ok(())
    }

    pub fn vcn(&self) -> Vcn {
        self.index_record_header.vcn
    }
}
