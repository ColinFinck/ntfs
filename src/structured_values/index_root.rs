// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use binread::io::{Read, Seek};
use binread::{BinRead, BinReaderExt};

/// Size of all [`IndexRootHeader`] fields plus some reserved bytes.
const INDEX_ROOT_HEADER_SIZE: u64 = 32;

#[derive(BinRead, Clone, Debug)]
struct IndexHeader {
    entries_offset: u32,
    index_size: u32,
    allocated_size: u32,
    flags: u8,
}

#[derive(BinRead, Clone, Debug)]
struct IndexRootHeader {
    ty: u32,
    collation_rule: u32,
    index_block_size: u32,
    clusters_per_index_block: i8,
    reserved: [u8; 3],
    index: IndexHeader,
}

#[derive(Clone, Debug)]
pub struct NtfsIndexRoot {
    header: IndexRootHeader,
}

impl NtfsIndexRoot {
    pub(crate) fn new<T>(
        attribute_position: u64,
        mut value_attached: NtfsAttributeValueAttached<'_, '_, T>,
        value_length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_length < INDEX_ROOT_HEADER_SIZE {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::IndexRoot,
                expected: INDEX_ROOT_HEADER_SIZE,
                actual: value_length,
            });
        }

        let header = value_attached.read_le::<IndexRootHeader>()?;

        Ok(Self { header })
    }
}
