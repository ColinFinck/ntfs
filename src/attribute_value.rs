// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use binread::io;
use binread::io::{Error, ErrorKind, Read, Seek, SeekFrom};
use core::cmp;

pub trait NtfsAttributeRead<T>
where
    T: Read + Seek,
{
    fn read(&mut self, fs: &mut T, buf: &mut [u8]) -> io::Result<usize>;

    fn read_exact(&mut self, fs: &mut T, mut buf: &mut [u8]) -> io::Result<()> {
        // This implementation is taken from https://github.com/rust-lang/rust/blob/5662d9343f0696efcc38a1264656737c9f22d427/library/std/src/io/mod.rs
        // It handles all corner cases properly and outputs the known `io` error messages.
        while !buf.is_empty() {
            match self.read(fs, buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            Err(Error::new(
                ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        } else {
            Ok(())
        }
    }
}

pub enum NtfsAttributeValue {
    Resident(NtfsAttributeResidentValue),
    NonResident(NtfsAttributeNonResidentValue),
}

impl NtfsAttributeValue {
    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsAttributeValueAttached<'a, T>
    where
        T: Read + Seek,
    {
        NtfsAttributeValueAttached { fs, value: self }
    }

    pub(crate) fn position(&self) -> u64 {
        match self {
            Self::Resident(inner) => inner.position(),
            Self::NonResident(inner) => inner.position(),
        }
    }
}

impl<T> NtfsAttributeRead<T> for NtfsAttributeValue
where
    T: Read + Seek,
{
    fn read(&mut self, fs: &mut T, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Resident(inner) => inner.read(fs, buf),
            Self::NonResident(inner) => inner.read(fs, buf),
        }
    }
}

impl Seek for NtfsAttributeValue {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Resident(inner) => inner.seek(pos),
            Self::NonResident(inner) => inner.seek(pos),
        }
    }
}

pub struct NtfsAttributeValueAttached<'a, T: Read + Seek> {
    fs: &'a mut T,
    value: NtfsAttributeValue,
}

impl<'a, T> NtfsAttributeValueAttached<'a, T>
where
    T: Read + Seek,
{
    pub fn detach(self) -> NtfsAttributeValue {
        self.value
    }

    pub fn position(&self) -> u64 {
        self.value.position()
    }
}

impl<'a, T> Read for NtfsAttributeValueAttached<'a, T>
where
    T: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.value.read(self.fs, buf)
    }
}

impl<'a, T> Seek for NtfsAttributeValueAttached<'a, T>
where
    T: Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.value.seek(pos)
    }
}

pub struct NtfsAttributeResidentValue {
    /// Absolute position of the attribute's value within the filesystem, in bytes.
    position: u64,
    /// Total length of the attribute's value, in bytes.
    length: u32,
    /// Current relative seek position within the value, in bytes.
    seek_position: u64,
}

impl NtfsAttributeResidentValue {
    pub(crate) fn new(position: u64, length: u32) -> Self {
        Self {
            position,
            length,
            seek_position: 0,
        }
    }

    pub fn position(&self) -> u64 {
        self.position
    }
}

impl<T> NtfsAttributeRead<T> for NtfsAttributeResidentValue
where
    T: Read + Seek,
{
    fn read(&mut self, fs: &mut T, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_left = (self.length as u64).saturating_sub(self.seek_position);
        if bytes_left == 0 {
            return Ok(0);
        }

        let bytes_to_read = cmp::min(buf.len(), bytes_left as usize);

        fs.seek(SeekFrom::Start(self.position + self.seek_position))?;
        fs.read(&mut buf[..bytes_to_read])?;

        self.seek_position += bytes_to_read as u64;
        Ok(bytes_to_read)
    }
}

impl Seek for NtfsAttributeResidentValue {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        // This implementation is taken from https://github.com/rust-lang/rust/blob/18c524fbae3ab1bf6ed9196168d8c68fc6aec61a/library/std/src/io/cursor.rs
        // It handles all signed/unsigned arithmetics properly and outputs the known `io` error messages.
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.seek_position = n;
                return Ok(n);
            }
            SeekFrom::End(n) => (self.length as u64, n),
            SeekFrom::Current(n) => (self.seek_position, n),
        };

        let new_pos = if offset >= 0 {
            base_pos.checked_add(offset as u64)
        } else {
            base_pos.checked_sub(offset.wrapping_neg() as u64)
        };

        match new_pos {
            Some(n) => {
                self.seek_position = n;
                Ok(self.seek_position)
            }
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}

pub struct NtfsAttributeNonResidentValue {
    // TODO
}

impl NtfsAttributeNonResidentValue {
    pub fn position(&self) -> u64 {
        panic!("TODO")
    }
}

impl<T> NtfsAttributeRead<T> for NtfsAttributeNonResidentValue
where
    T: Read + Seek,
{
    fn read(&mut self, _fs: &mut T, _buf: &mut [u8]) -> io::Result<usize> {
        panic!("TODO")
    }
}

impl Seek for NtfsAttributeNonResidentValue {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        panic!("TODO")
    }
}
