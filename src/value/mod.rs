// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

pub(crate) mod attribute_list_non_resident_attribute;
pub(crate) mod non_resident_attribute;
pub(crate) mod slice;

use binread::io;
use binread::io::{Read, Seek, SeekFrom};

use crate::error::{NtfsError, Result};
use crate::traits::NtfsReadSeek;
use attribute_list_non_resident_attribute::NtfsAttributeListNonResidentAttributeValue;
use non_resident_attribute::NtfsNonResidentAttributeValue;
use slice::NtfsSliceValue;

#[derive(Clone, Debug)]
pub enum NtfsValue<'n, 'f> {
    Slice(NtfsSliceValue<'f>),
    NonResidentAttribute(NtfsNonResidentAttributeValue<'n, 'f>),
    AttributeListNonResidentAttribute(NtfsAttributeListNonResidentAttributeValue<'n, 'f>),
}

impl<'n, 'f> NtfsValue<'n, 'f> {
    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsValueAttached<'n, 'f, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsValueAttached::new(fs, self)
    }

    pub fn data_position(&self) -> Option<u64> {
        match self {
            Self::Slice(inner) => inner.data_position(),
            Self::NonResidentAttribute(inner) => inner.data_position(),
            Self::AttributeListNonResidentAttribute(inner) => inner.data_position(),
        }
    }

    pub fn len(&self) -> u64 {
        match self {
            Self::Slice(inner) => inner.len(),
            Self::NonResidentAttribute(inner) => inner.len(),
            Self::AttributeListNonResidentAttribute(inner) => inner.len(),
        }
    }
}

impl<'n, 'f> NtfsReadSeek for NtfsValue<'n, 'f> {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        match self {
            Self::Slice(inner) => inner.read(fs, buf),
            Self::NonResidentAttribute(inner) => inner.read(fs, buf),
            Self::AttributeListNonResidentAttribute(inner) => inner.read(fs, buf),
        }
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        match self {
            Self::Slice(inner) => inner.seek(fs, pos),
            Self::NonResidentAttribute(inner) => inner.seek(fs, pos),
            Self::AttributeListNonResidentAttribute(inner) => inner.seek(fs, pos),
        }
    }

    fn stream_position(&self) -> u64 {
        match self {
            Self::Slice(inner) => inner.stream_position(),
            Self::NonResidentAttribute(inner) => inner.stream_position(),
            Self::AttributeListNonResidentAttribute(inner) => inner.stream_position(),
        }
    }
}

pub struct NtfsValueAttached<'n, 'f, 'a, T: Read + Seek> {
    fs: &'a mut T,
    value: NtfsValue<'n, 'f>,
}

impl<'n, 'f, 'a, T> NtfsValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, value: NtfsValue<'n, 'f>) -> Self {
        Self { fs, value }
    }

    pub fn data_position(&self) -> Option<u64> {
        self.value.data_position()
    }

    pub fn detach(self) -> NtfsValue<'n, 'f> {
        self.value
    }

    pub fn len(&self) -> u64 {
        self.value.len()
    }
}

impl<'n, 'f, 'a, T> Read for NtfsValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.value.read(self.fs, buf).map_err(io::Error::from)
    }
}

impl<'n, 'f, 'a, T> Seek for NtfsValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.value.seek(self.fs, pos).map_err(io::Error::from)
    }
}

pub(crate) fn seek_contiguous(
    stream_position: &mut u64,
    length: u64,
    pos: SeekFrom,
) -> Result<u64> {
    // This implementation is taken from https://github.com/rust-lang/rust/blob/18c524fbae3ab1bf6ed9196168d8c68fc6aec61a/library/std/src/io/cursor.rs
    // It handles all signed/unsigned arithmetics properly and outputs the known `io` error message.
    let (base_pos, offset) = match pos {
        SeekFrom::Start(n) => {
            *stream_position = n;
            return Ok(n);
        }
        SeekFrom::End(n) => (length, n),
        SeekFrom::Current(n) => (*stream_position, n),
    };

    let new_pos = if offset >= 0 {
        base_pos.checked_add(offset as u64)
    } else {
        base_pos.checked_sub(offset.wrapping_neg() as u64)
    };

    match new_pos {
        Some(n) => {
            *stream_position = n;
            Ok(*stream_position)
        }
        None => Err(NtfsError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowing position",
        ))),
    }
}
