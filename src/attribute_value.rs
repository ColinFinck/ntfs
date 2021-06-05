// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use crate::traits::NtfsReadSeek;
use crate::types::Lcn;
use binread::io;
use binread::io::{Read, Seek, SeekFrom};
use binread::BinReaderExt;
use core::iter::FusedIterator;
use core::ops::Range;
use core::{cmp, mem};

#[derive(Clone, Debug)]
pub enum NtfsAttributeValue<'n> {
    Resident(NtfsDataRun),
    NonResident(NtfsAttributeNonResidentValue<'n>),
}

impl<'n> NtfsAttributeValue<'n> {
    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsAttributeValueAttached<'n, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsAttributeValueAttached::new(fs, self)
    }

    pub fn data_position(&self) -> Option<u64> {
        match self {
            Self::Resident(inner) => Some(inner.data_position()),
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

impl<'n> NtfsReadSeek for NtfsAttributeValue<'n> {
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

pub struct NtfsAttributeValueAttached<'n, 'a, T: Read + Seek> {
    fs: &'a mut T,
    value: NtfsAttributeValue<'n>,
}

impl<'n, 'a, T> NtfsAttributeValueAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, value: NtfsAttributeValue<'n>) -> Self {
        Self { fs, value }
    }

    pub fn data_position(&self) -> Option<u64> {
        self.value.data_position()
    }

    pub fn detach(self) -> NtfsAttributeValue<'n> {
        self.value
    }

    pub fn len(&self) -> u64 {
        self.value.len()
    }
}

impl<'n, 'a, T> Read for NtfsAttributeValueAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.value.read(self.fs, buf).map_err(io::Error::from)
    }
}

impl<'n, 'a, T> Seek for NtfsAttributeValueAttached<'n, 'a, T>
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
    position: u64,
    /// Total length of the attribute's value, in bytes.
    length: u64,
    /// Current relative position within the value, in bytes.
    stream_position: u64,
}

impl NtfsDataRun {
    pub(crate) fn from_byte_info(position: u64, length: u64) -> Self {
        Self {
            position,
            length,
            stream_position: 0,
        }
    }

    pub(crate) fn from_lcn_info(ntfs: &Ntfs, lcn: Lcn, cluster_count: u64) -> Result<Self> {
        let position = lcn.position(ntfs)?;
        let length = cluster_count
            .checked_mul(ntfs.cluster_size() as u64)
            .ok_or(NtfsError::InvalidClusterCount { cluster_count })?;

        Ok(Self::from_byte_info(position, length))
    }

    pub fn data_position(&self) -> u64 {
        self.position + self.stream_position
    }

    pub fn len(&self) -> u64 {
        self.length
    }

    fn remaining_len(&self) -> u64 {
        self.length.saturating_sub(self.stream_position)
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
            fs.seek(SeekFrom::Start(self.position + self.stream_position))?;
            fs.read(work_slice)?;
        }

        self.stream_position += bytes_to_read as u64;
        Ok(bytes_to_read)
    }

    fn seek<T>(&mut self, _fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        // This implementation is taken from https://github.com/rust-lang/rust/blob/18c524fbae3ab1bf6ed9196168d8c68fc6aec61a/library/std/src/io/cursor.rs
        // It handles all signed/unsigned arithmetics properly and outputs the known `io` error message.
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.stream_position = n;
                return Ok(n);
            }
            SeekFrom::End(n) => (self.length, n),
            SeekFrom::Current(n) => (self.stream_position, n),
        };

        let new_pos = if offset >= 0 {
            base_pos.checked_add(offset as u64)
        } else {
            base_pos.checked_sub(offset.wrapping_neg() as u64)
        };

        match new_pos {
            Some(n) => {
                self.stream_position = n;
                Ok(self.stream_position)
            }
            None => Err(NtfsError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            ))),
        }
    }

    fn stream_position(&self) -> u64 {
        self.stream_position
    }
}

#[derive(Clone, Debug)]
pub struct NtfsDataRuns<'n> {
    ntfs: &'n Ntfs,
    data_runs_range: Range<u64>,
    previous_lcn: Lcn,
}

impl<'n> NtfsDataRuns<'n> {
    fn new(ntfs: &'n Ntfs, data_runs_range: Range<u64>) -> Self {
        Self {
            ntfs,
            data_runs_range,
            previous_lcn: Lcn::from(0),
        }
    }

    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsDataRunsAttached<'n, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsDataRunsAttached::new(fs, self)
    }

    pub fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsDataRun>>
    where
        T: Read + Seek,
    {
        if self.data_runs_range.is_empty() {
            return None;
        }

        // Read the single header byte.
        iter_try!(fs.seek(SeekFrom::Start(self.data_runs_range.start)));
        let header = iter_try!(fs.read_le::<u8>());
        let mut size = 1u64;

        // A zero byte marks the end of the data runs.
        if header == 0 {
            // Ensure `self.data_runs_range.is_empty` returns true, so any further call uses the fast path above.
            self.data_runs_range.start = self.data_runs_range.end;
            return None;
        }

        // The lower nibble indicates the length of the following cluster count variable length integer.
        let cluster_count_byte_count = header & 0x0f;
        let cluster_count =
            iter_try!(self.read_variable_length_unsigned_integer(fs, cluster_count_byte_count));
        size += cluster_count_byte_count as u64;

        // The upper nibble indicates the length of the following VCN variable length integer.
        let vcn_byte_count = (header & 0xf0) >> 4;
        let vcn = iter_try!(self.read_variable_length_signed_integer(fs, vcn_byte_count)).into();
        size += vcn_byte_count as u64;

        // Turn the read VCN into an absolute LCN.
        let lcn = iter_try!(self.previous_lcn.checked_add(vcn).ok_or({
            NtfsError::InvalidVcnInDataRunHeader {
                position: self.data_runs_range.start,
                vcn,
                previous_lcn: self.previous_lcn,
            }
        }));
        self.previous_lcn = lcn;

        // Only increment `self.data_runs_range.start` after having checked for success.
        // In case of an error, a subsequent call shall output the same error again.
        self.data_runs_range.start += size;

        let data_run = iter_try!(NtfsDataRun::from_lcn_info(self.ntfs, lcn, cluster_count));
        Some(Ok(data_run))
    }

    fn read_variable_length_bytes<T>(&self, fs: &mut T, byte_count: u8) -> Result<[u8; 8]>
    where
        T: Read + Seek,
    {
        const MAX_BYTE_COUNT: u8 = mem::size_of::<u64>() as u8;

        if byte_count > MAX_BYTE_COUNT {
            return Err(NtfsError::InvalidByteCountInDataRunHeader {
                position: self.data_runs_range.start,
                expected: MAX_BYTE_COUNT,
                actual: byte_count,
            });
        }

        let mut buf = [0u8; MAX_BYTE_COUNT as usize];
        fs.read_exact(&mut buf[..byte_count as usize])?;

        Ok(buf)
    }

    fn read_variable_length_signed_integer<T>(&self, fs: &mut T, byte_count: u8) -> Result<i64>
    where
        T: Read + Seek,
    {
        let buf = self.read_variable_length_bytes(fs, byte_count)?;
        let mut integer = i64::from_le_bytes(buf);

        // We have read `byte_count` bytes into a zeroed buffer and just interpreted that as an `i64`.
        // Sign-extend `integer` to make it replicate the proper value.
        let unused_bits = (mem::size_of::<i64>() as u32 - byte_count as u32) * 8;
        integer = integer.wrapping_shl(unused_bits).wrapping_shr(unused_bits);

        Ok(integer)
    }

    fn read_variable_length_unsigned_integer<T>(&self, fs: &mut T, byte_count: u8) -> Result<u64>
    where
        T: Read + Seek,
    {
        let buf = self.read_variable_length_bytes(fs, byte_count)?;
        let integer = u64::from_le_bytes(buf);
        Ok(integer)
    }
}

#[derive(Debug)]
pub struct NtfsDataRunsAttached<'n, 'a, T: Read + Seek> {
    fs: &'a mut T,
    data_runs: NtfsDataRuns<'n>,
}

impl<'n, 'a, T> NtfsDataRunsAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, data_runs: NtfsDataRuns<'n>) -> Self {
        Self { fs, data_runs }
    }

    pub fn detach(self) -> NtfsDataRuns<'n> {
        self.data_runs
    }
}

impl<'n, 'a, T> Iterator for NtfsDataRunsAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    type Item = Result<NtfsDataRun>;

    fn next(&mut self) -> Option<Self::Item> {
        self.data_runs.next(self.fs)
    }
}

impl<'n, 'a, T> FusedIterator for NtfsDataRunsAttached<'n, 'a, T> where T: Read + Seek {}

#[derive(Clone, Debug)]
pub struct NtfsAttributeNonResidentValue<'n> {
    /// Reference to the base `Ntfs` object of this filesystem.
    ntfs: &'n Ntfs,
    /// Byte range where the data run information of this non-resident value is stored on the filesystem.
    data_runs_range: Range<u64>,
    /// Total size of the data spread among all data runs, in bytes.
    data_size: u64,
    /// Iterator of data runs used for reading/seeking.
    stream_data_runs: NtfsDataRuns<'n>,
    /// Current data run we are reading from.
    stream_data_run: Option<NtfsDataRun>,
    /// Total stream position, in bytes.
    stream_position: u64,
}

impl<'n> NtfsAttributeNonResidentValue<'n> {
    pub(crate) fn new<T>(
        ntfs: &'n Ntfs,
        fs: &mut T,
        data_runs_range: Range<u64>,
        data_size: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        let mut stream_data_runs = NtfsDataRuns::new(ntfs, data_runs_range.clone());

        // Get the first data run already here to let `data_position` return something meaningful.
        let stream_data_run = match stream_data_runs.next(fs) {
            Some(Ok(data_run)) => Some(data_run),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        Ok(Self {
            ntfs,
            data_runs_range,
            data_size,
            stream_data_runs,
            stream_data_run,
            stream_position: 0,
        })
    }

    pub fn data_position(&self) -> Option<u64> {
        self.stream_data_run
            .as_ref()
            .map(|data_run| data_run.data_position())
    }

    pub fn data_runs(&self) -> NtfsDataRuns<'n> {
        NtfsDataRuns::new(self.ntfs, self.data_runs_range.clone())
    }

    pub fn len(&self) -> u64 {
        self.data_size
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
                if n_from_current <= i64::MAX as u64 {
                    bytes_to_seek = SeekFrom::Current(n_from_current as i64);
                }
            }
        }

        let mut bytes_left_to_seek = match bytes_to_seek {
            SeekFrom::Start(n) => {
                // Reset `stream_data_runs` and `stream_data_run` to read from the very beginning.
                self.stream_data_runs = NtfsDataRuns::new(self.ntfs, self.data_runs_range.clone());
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

            match self.stream_data_runs.next(fs) {
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

impl<'n> NtfsReadSeek for NtfsAttributeNonResidentValue<'n> {
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
            match self.stream_data_runs.next(fs) {
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
