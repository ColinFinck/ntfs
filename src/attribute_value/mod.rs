// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

mod attribute_list_non_resident;
mod non_resident;
mod resident;

pub use attribute_list_non_resident::*;
pub use non_resident::*;
pub use resident::*;

use binread::io;
use binread::io::{Read, Seek, SeekFrom};

use crate::error::{NtfsError, Result};
use crate::traits::NtfsReadSeek;

#[derive(Clone, Debug)]
pub enum NtfsAttributeValue<'n, 'f> {
    Resident(NtfsResidentAttributeValue<'f>),
    NonResident(NtfsNonResidentAttributeValue<'n, 'f>),
    AttributeListNonResident(NtfsAttributeListNonResidentAttributeValue<'n, 'f>),
}

impl<'n, 'f> NtfsAttributeValue<'n, 'f> {
    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsAttributeValueAttached<'n, 'f, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsAttributeValueAttached::new(fs, self)
    }

    pub fn data_position(&self) -> Option<u64> {
        match self {
            Self::Resident(inner) => inner.data_position(),
            Self::NonResident(inner) => inner.data_position(),
            Self::AttributeListNonResident(inner) => inner.data_position(),
        }
    }

    pub fn len(&self) -> u64 {
        match self {
            Self::Resident(inner) => inner.len(),
            Self::NonResident(inner) => inner.len(),
            Self::AttributeListNonResident(inner) => inner.len(),
        }
    }
}

impl<'n, 'f> NtfsReadSeek for NtfsAttributeValue<'n, 'f> {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        match self {
            Self::Resident(inner) => inner.read(fs, buf),
            Self::NonResident(inner) => inner.read(fs, buf),
            Self::AttributeListNonResident(inner) => inner.read(fs, buf),
        }
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        match self {
            Self::Resident(inner) => inner.seek(fs, pos),
            Self::NonResident(inner) => inner.seek(fs, pos),
            Self::AttributeListNonResident(inner) => inner.seek(fs, pos),
        }
    }

    fn stream_position(&self) -> u64 {
        match self {
            Self::Resident(inner) => inner.stream_position(),
            Self::NonResident(inner) => inner.stream_position(),
            Self::AttributeListNonResident(inner) => inner.stream_position(),
        }
    }
}

pub struct NtfsAttributeValueAttached<'n, 'f, 'a, T: Read + Seek> {
    fs: &'a mut T,
    value: NtfsAttributeValue<'n, 'f>,
}

impl<'n, 'f, 'a, T> NtfsAttributeValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, value: NtfsAttributeValue<'n, 'f>) -> Self {
        Self { fs, value }
    }

    pub fn data_position(&self) -> Option<u64> {
        self.value.data_position()
    }

    pub fn detach(self) -> NtfsAttributeValue<'n, 'f> {
        self.value
    }

    pub fn len(&self) -> u64 {
        self.value.len()
    }
}

impl<'n, 'f, 'a, T> Read for NtfsAttributeValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.value.read(self.fs, buf).map_err(io::Error::from)
    }
}

impl<'n, 'f, 'a, T> Seek for NtfsAttributeValueAttached<'n, 'f, 'a, T>
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
