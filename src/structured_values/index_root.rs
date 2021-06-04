// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::structured_values::NewNtfsStructuredValue;
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

impl<'n> NewNtfsStructuredValue<'n> for NtfsIndexRoot<'n> {
    fn new<T>(fs: &mut T, value: NtfsAttributeValue<'n>, _length: u64) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value.len() < INDEX_ROOT_HEADER_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::IndexRoot,
                expected: INDEX_ROOT_HEADER_SIZE,
                actual: value.len(),
            });
        }

        let mut value_attached = value.clone().attach(fs);
        let header = value_attached.read_le::<IndexRootHeader>()?;

        Ok(Self { header })
    }
}
