// Copyright 2021-2022 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! This module implements a reader for a value that is already in memory and can therefore be accessed via a slice.
//! This is the case for all resident attribute values and Index Record values.
//! Such values are part of NTFS records. NTFS records can't be directly read from the filesystem, which is why they
//! are always read into a buffer first and then fixed up in memory.
//! Further accesses to the record data can then happen via slices.

use binread::io::{Read, Seek, SeekFrom};

use super::seek_contiguous;
use crate::error::Result;
use crate::traits::NtfsReadSeek;

/// Reader for a value of a resident NTFS Attribute (which is entirely contained in the NTFS File Record).
#[derive(Clone, Debug)]
pub struct NtfsResidentAttributeValue<'f> {
    data: &'f [u8],
    position: u64,
    stream_position: u64,
}

impl<'f> NtfsResidentAttributeValue<'f> {
    pub(crate) fn new(data: &'f [u8], position: u64) -> Self {
        Self {
            data,
            position,
            stream_position: 0,
        }
    }

    /// Returns a slice of the entire value data.
    ///
    /// Remember that a resident attribute fits entirely inside the NTFS File Record
    /// of the requested file.
    /// Hence, the fixed up File Record is entirely in memory at this stage and a slice
    /// to a resident attribute value can be obtained easily.
    pub fn data(&self) -> &'f [u8] {
        self.data
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if the current seek position is outside the valid range.
    pub fn data_position(&self) -> Option<u64> {
        if self.stream_position <= self.len() {
            Some(self.position + self.stream_position)
        } else {
            None
        }
    }

    /// Returns `true` if the resident attribute value contains no data.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the total length of the resident attribute value data, in bytes.
    pub fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn remaining_len(&self) -> u64 {
        self.len().saturating_sub(self.stream_position)
    }
}

impl<'f> NtfsReadSeek for NtfsResidentAttributeValue<'f> {
    fn read<T>(&mut self, _fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        if self.remaining_len() == 0 {
            return Ok(0);
        }

        let bytes_to_read = usize::min(buf.len(), self.remaining_len() as usize);
        let work_slice = &mut buf[..bytes_to_read];

        let start = self.stream_position as usize;
        let end = start + bytes_to_read;
        work_slice.copy_from_slice(&self.data[start..end]);

        self.stream_position += bytes_to_read as u64;
        Ok(bytes_to_read)
    }

    fn seek<T>(&mut self, _fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        let length = self.len();
        seek_contiguous(&mut self.stream_position, length, pos)
    }

    fn stream_position(&self) -> u64 {
        self.stream_position
    }
}
