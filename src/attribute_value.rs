// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use crate::traits::NtfsReadSeek;
use crate::types::{Lcn, Vcn};
use binread::io;
use binread::io::Cursor;
use binread::io::{Read, Seek, SeekFrom};
use binread::BinRead;
use core::convert::TryFrom;
use core::iter::FusedIterator;
use core::{cmp, mem};

#[derive(Clone, Debug)]
pub enum NtfsAttributeValue<'n, 'f> {
    Resident(NtfsResidentAttributeValue<'f>),
    NonResident(NtfsNonResidentAttributeValue<'n, 'f>),
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
        }
    }

    pub fn len(&self) -> u64 {
        match self {
            Self::Resident(inner) => inner.len(),
            Self::NonResident(inner) => inner.len(),
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
        }
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        match self {
            Self::Resident(inner) => inner.seek(fs, pos),
            Self::NonResident(inner) => inner.seek(fs, pos),
        }
    }

    fn stream_position(&self) -> u64 {
        match self {
            Self::Resident(inner) => inner.stream_position(),
            Self::NonResident(inner) => inner.stream_position(),
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

#[derive(Clone, Debug)]
pub struct NtfsDataRun {
    /// Absolute position of the attribute's value within the filesystem, in bytes.
    /// This may be zero if this is a "sparse" data run.
    position: u64,
    /// Total length of the attribute's value, in bytes.
    length: u64,
    /// Current relative position within the value, in bytes.
    stream_position: u64,
}

impl NtfsDataRun {
    pub(crate) fn new(ntfs: &Ntfs, lcn: Lcn, cluster_count: u64) -> Result<Self> {
        let position = lcn.position(ntfs)?;
        let length = cluster_count
            .checked_mul(ntfs.cluster_size() as u64)
            .ok_or(NtfsError::InvalidClusterCount { cluster_count })?;

        Ok(Self {
            position,
            length,
            stream_position: 0,
        })
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if:
    ///   * The current seek position is outside the valid range, or
    ///   * The data run is a "sparse" data run
    pub fn data_position(&self) -> Option<u64> {
        if self.position > 0 && self.stream_position < self.len() {
            Some(self.position + self.stream_position)
        } else {
            None
        }
    }

    pub fn len(&self) -> u64 {
        self.length
    }

    fn remaining_len(&self) -> u64 {
        self.len().saturating_sub(self.stream_position)
    }
}

impl NtfsReadSeek for NtfsDataRun {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        if self.remaining_len() == 0 {
            return Ok(0);
        }

        let bytes_to_read = cmp::min(buf.len(), self.remaining_len() as usize);
        let work_slice = &mut buf[..bytes_to_read];

        if self.position == 0 {
            // This is a sparse data run.
            work_slice.fill(0);
        } else {
            // This data run contains "real" data.
            // We have already performed all necessary sanity checks above, so we can just unwrap here.
            fs.seek(SeekFrom::Start(self.data_position().unwrap()))?;
            fs.read(work_slice)?;
        }

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

#[derive(Clone, Debug)]
pub struct NtfsDataRuns<'n, 'f> {
    ntfs: &'n Ntfs,
    data: &'f [u8],
    position: u64,
    previous_lcn: Lcn,
}

impl<'n, 'f> NtfsDataRuns<'n, 'f> {
    fn new(ntfs: &'n Ntfs, data: &'f [u8], position: u64) -> Self {
        Self {
            ntfs,
            data,
            position,
            previous_lcn: Lcn::from(0),
        }
    }

    fn read_variable_length_bytes(
        &self,
        cursor: &mut Cursor<&[u8]>,
        byte_count: u8,
    ) -> Result<[u8; 8]> {
        const MAX_BYTE_COUNT: u8 = mem::size_of::<u64>() as u8;

        if byte_count > MAX_BYTE_COUNT {
            return Err(NtfsError::InvalidByteCountInDataRunHeader {
                position: self.position,
                expected: byte_count,
                actual: MAX_BYTE_COUNT,
            });
        }

        let mut buf = [0u8; MAX_BYTE_COUNT as usize];
        cursor.read_exact(&mut buf[..byte_count as usize])?;

        Ok(buf)
    }

    fn read_variable_length_signed_integer(
        &self,
        cursor: &mut Cursor<&[u8]>,
        byte_count: u8,
    ) -> Result<i64> {
        let buf = self.read_variable_length_bytes(cursor, byte_count)?;
        let mut integer = i64::from_le_bytes(buf);

        // We have read `byte_count` bytes into a zeroed buffer and just interpreted that as an `i64`.
        // Sign-extend `integer` to make it replicate the proper value.
        let unused_bits = (mem::size_of::<i64>() as u32 - byte_count as u32) * 8;
        integer = integer.wrapping_shl(unused_bits).wrapping_shr(unused_bits);

        Ok(integer)
    }

    fn read_variable_length_unsigned_integer(
        &self,
        cursor: &mut Cursor<&[u8]>,
        byte_count: u8,
    ) -> Result<u64> {
        let buf = self.read_variable_length_bytes(cursor, byte_count)?;
        let integer = u64::from_le_bytes(buf);
        Ok(integer)
    }
}

impl<'n, 'f> Iterator for NtfsDataRuns<'n, 'f> {
    type Item = Result<NtfsDataRun>;

    fn next(&mut self) -> Option<Result<NtfsDataRun>> {
        if self.data.is_empty() {
            return None;
        }

        // Read the single header byte.
        let mut cursor = Cursor::new(self.data);
        let header = iter_try!(u8::read(&mut cursor));

        // A zero byte marks the end of the data runs.
        if header == 0 {
            // Ensure `self.data.is_empty` returns true, so any further call uses the fast path above.
            self.data = &[];
            return None;
        }

        // The lower nibble indicates the length of the following cluster count variable length integer.
        let cluster_count_byte_count = header & 0x0f;
        let cluster_count = iter_try!(
            self.read_variable_length_unsigned_integer(&mut cursor, cluster_count_byte_count)
        );

        // The upper nibble indicates the length of the following VCN variable length integer.
        let vcn_byte_count = (header & 0xf0) >> 4;
        let vcn = Vcn::from(iter_try!(
            self.read_variable_length_signed_integer(&mut cursor, vcn_byte_count)
        ));

        // Turn the read VCN into an absolute LCN.
        let lcn = iter_try!(self.previous_lcn.checked_add(vcn).ok_or({
            NtfsError::InvalidVcnInDataRunHeader {
                position: self.position,
                vcn,
                previous_lcn: self.previous_lcn,
            }
        }));
        self.previous_lcn = lcn;

        // Only advance after having checked for success.
        // In case of an error, a subsequent call shall output the same error again.
        let bytes_to_advance = cursor.stream_position().unwrap();
        self.data = &self.data[bytes_to_advance as usize..];
        self.position += bytes_to_advance;

        let data_run = iter_try!(NtfsDataRun::new(self.ntfs, lcn, cluster_count));
        Some(Ok(data_run))
    }
}

impl<'n, 'f> FusedIterator for NtfsDataRuns<'n, 'f> {}

#[derive(Clone, Debug)]
pub struct NtfsNonResidentAttributeValue<'n, 'f> {
    /// Reference to the base `Ntfs` object of this filesystem.
    ntfs: &'n Ntfs,
    /// Attribute bytes where the data run information of this non-resident value is stored on the filesystem.
    data: &'f [u8],
    /// Absolute position of the data run information within the filesystem, in bytes.
    position: u64,
    /// Total size of the data spread among all data runs, in bytes.
    data_size: u64,
    /// Iterator of data runs used for reading/seeking.
    stream_data_runs: NtfsDataRuns<'n, 'f>,
    /// Current data run we are reading from.
    stream_data_run: Option<NtfsDataRun>,
    /// Total stream position, in bytes.
    stream_position: u64,
}

impl<'n, 'f> NtfsNonResidentAttributeValue<'n, 'f> {
    pub(crate) fn new(
        ntfs: &'n Ntfs,
        data: &'f [u8],
        position: u64,
        data_size: u64,
    ) -> Result<Self> {
        let mut stream_data_runs = NtfsDataRuns::new(ntfs, data, position);

        // Get the first data run already here to let `data_position` return something meaningful.
        let stream_data_run = match stream_data_runs.next() {
            Some(Ok(data_run)) => Some(data_run),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        Ok(Self {
            ntfs,
            data,
            position,
            data_size,
            stream_data_runs,
            stream_data_run,
            stream_position: 0,
        })
    }

    pub fn attach<'a, T>(
        self,
        fs: &'a mut T,
    ) -> NtfsNonResidentAttributeValueAttached<'n, 'f, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsNonResidentAttributeValueAttached::new(fs, self)
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if:
    ///   * The current seek position is outside the valid range, or
    ///   * The current data run is a "sparse" data run
    pub fn data_position(&self) -> Option<u64> {
        let stream_data_run = self.stream_data_run.as_ref()?;
        stream_data_run.data_position()
    }

    pub fn data_runs(&self) -> NtfsDataRuns<'n, 'f> {
        NtfsDataRuns::new(self.ntfs, self.data, self.position)
    }

    pub fn len(&self) -> u64 {
        self.data_size
    }

    pub fn ntfs(&self) -> &'n Ntfs {
        self.ntfs
    }

    /// Returns the absolute position of the data run information within the filesystem, in bytes.
    pub fn position(&self) -> u64 {
        self.position
    }

    fn do_seek<T>(&mut self, fs: &mut T, mut bytes_to_seek: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        // Translate `SeekFrom::Start(n)` into a more efficient `SeekFrom::Current`
        // if n >= self.stream_position.
        // We don't need to traverse data runs from the very beginning then.
        if let SeekFrom::Start(n) = bytes_to_seek {
            if let Some(n_from_current) = n.checked_sub(self.stream_position) {
                if let Ok(signed_n_from_current) = i64::try_from(n_from_current) {
                    bytes_to_seek = SeekFrom::Current(signed_n_from_current);
                }
            }
        }

        let mut bytes_left_to_seek = match bytes_to_seek {
            SeekFrom::Start(n) => {
                // Reset `stream_data_runs` and `stream_data_run` to read from the very beginning.
                self.stream_data_runs = NtfsDataRuns::new(self.ntfs, self.data, self.position);
                self.stream_data_run = None;
                n
            }
            SeekFrom::Current(n) if n >= 0 => n as u64,
            _ => panic!("do_seek only accepts positive seeks from Start or Current!"),
        };

        while bytes_left_to_seek > 0 {
            if let Some(data_run) = &mut self.stream_data_run {
                if bytes_left_to_seek < data_run.remaining_len() {
                    // We have found the right data run, now we have to seek inside the data run.
                    //
                    // If we were called to seek from the very beginning, we can be sure that this
                    // data run is also seeked from the beginning.
                    // Hence, we can use SeekFrom::Start and use the full u64 range.
                    //
                    // If we were called to seek from the current position, we have to use
                    // SeekFrom::Current and can only use the positive part of the i64 range.
                    // This is no problem though, as `bytes_left_to_seek` was also created from a
                    // positive i64 value in that case.
                    let pos = match bytes_to_seek {
                        SeekFrom::Start(_) => SeekFrom::Start(bytes_left_to_seek),
                        SeekFrom::Current(_) => SeekFrom::Current(bytes_left_to_seek as i64),
                        _ => unreachable!(),
                    };

                    data_run.seek(fs, pos)?;
                    break;
                } else {
                    // We can skip the entire data run.
                    bytes_left_to_seek -= data_run.remaining_len();
                }
            }

            match self.stream_data_runs.next() {
                Some(Ok(data_run)) => self.stream_data_run = Some(data_run),
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }

        match bytes_to_seek {
            SeekFrom::Start(n) => self.stream_position = n,
            SeekFrom::Current(n) => self.stream_position += n as u64,
            _ => unreachable!(),
        }

        Ok(self.stream_position)
    }
}

impl<'n, 'f> NtfsReadSeek for NtfsNonResidentAttributeValue<'n, 'f> {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        let mut bytes_read = 0usize;

        while bytes_read < buf.len() {
            if let Some(data_run) = &mut self.stream_data_run {
                if data_run.stream_position() < data_run.len() {
                    let bytes_read_in_data_run = data_run.read(fs, &mut buf[bytes_read..])?;
                    bytes_read += bytes_read_in_data_run;
                    self.stream_position += bytes_read_in_data_run as u64;
                    continue;
                }
            }

            // We still have bytes to read, but no data run or the previous data run has been read to its end.
            // Get the next data run and try again.
            match self.stream_data_runs.next() {
                Some(Ok(data_run)) => self.stream_data_run = Some(data_run),
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }

        Ok(bytes_read)
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        match pos {
            SeekFrom::Start(n) => {
                // Seek n bytes from the very beginning.
                return self.do_seek(fs, SeekFrom::Start(n));
            }
            SeekFrom::End(n) => {
                if n >= 0 {
                    if let Some(bytes_to_seek) = self.data_size.checked_add(n as u64) {
                        // Seek data_size + n bytes from the very beginning.
                        return self.do_seek(fs, SeekFrom::Start(bytes_to_seek));
                    }
                } else {
                    if let Some(bytes_to_seek) = self.data_size.checked_sub(n.wrapping_neg() as u64)
                    {
                        // Seek data_size + n bytes (with n being negative) from the very beginning.
                        return self.do_seek(fs, SeekFrom::Start(bytes_to_seek));
                    }
                }
            }
            SeekFrom::Current(n) => {
                if n >= 0 {
                    if self.stream_position.checked_add(n as u64).is_some() {
                        // Seek n bytes from the current position.
                        // This is an optimization for the common case, as we don't need to traverse all
                        // data runs from the very beginning.
                        return self.do_seek(fs, SeekFrom::Current(n));
                    }
                } else {
                    if let Some(bytes_to_seek) =
                        self.stream_position.checked_sub(n.wrapping_neg() as u64)
                    {
                        // Seek stream_position + n bytes (with n being negative) from the very beginning.
                        return self.do_seek(fs, SeekFrom::Start(bytes_to_seek));
                    }
                }
            }
        }

        Err(NtfsError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowing position",
        )))
    }

    fn stream_position(&self) -> u64 {
        self.stream_position
    }
}

pub struct NtfsNonResidentAttributeValueAttached<'n, 'f, 'a, T: Read + Seek> {
    fs: &'a mut T,
    value: NtfsNonResidentAttributeValue<'n, 'f>,
}

impl<'n, 'f, 'a, T> NtfsNonResidentAttributeValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, value: NtfsNonResidentAttributeValue<'n, 'f>) -> Self {
        Self { fs, value }
    }

    pub fn data_position(&self) -> Option<u64> {
        self.value.data_position()
    }

    pub fn detach(self) -> NtfsNonResidentAttributeValue<'n, 'f> {
        self.value
    }

    pub fn len(&self) -> u64 {
        self.value.len()
    }
}

impl<'n, 'f, 'a, T> Read for NtfsNonResidentAttributeValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.value.read(self.fs, buf).map_err(io::Error::from)
    }
}

impl<'n, 'f, 'a, T> Seek for NtfsNonResidentAttributeValueAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.value.seek(self.fs, pos).map_err(io::Error::from)
    }
}

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

    pub fn data(&self) -> &'f [u8] {
        self.data
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if the current seek position is outside the valid range.
    pub fn data_position(&self) -> Option<u64> {
        if self.stream_position < self.len() {
            Some(self.position + self.stream_position)
        } else {
            None
        }
    }

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

        let bytes_to_read = cmp::min(buf.len(), self.remaining_len() as usize);
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

fn seek_contiguous(stream_position: &mut u64, length: u64, pos: SeekFrom) -> Result<u64> {
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
