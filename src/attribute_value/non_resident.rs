// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later
//
//! This module implements a reader for a non-resident attribute value (that is not part of an AttributeList).
//! Non-resident attribute values are split up into one or more data runs, which are spread across the filesystem.
//! This reader provides one contiguous data stream for all data runs.

use core::convert::TryFrom;
use core::iter::FusedIterator;
use core::mem;

use binread::io;
use binread::io::Cursor;
use binread::io::{Read, Seek, SeekFrom};
use binread::BinRead;

use super::seek_contiguous;
use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use crate::traits::NtfsReadSeek;
use crate::types::{Lcn, Vcn};

#[derive(Clone, Debug)]
pub struct NtfsNonResidentAttributeValue<'n, 'f> {
    /// Reference to the base `Ntfs` object of this filesystem.
    ntfs: &'n Ntfs,
    /// Attribute bytes where the data run information of this non-resident value is stored on the filesystem.
    data: &'f [u8],
    /// Absolute position of the data run information within the filesystem, in bytes.
    position: u64,
    /// Iterator of data runs used for reading/seeking.
    stream_data_runs: NtfsDataRuns<'n, 'f>,
    /// Iteration state of the current data run.
    stream_state: StreamState,
}

impl<'n, 'f> NtfsNonResidentAttributeValue<'n, 'f> {
    pub(crate) fn new(
        ntfs: &'n Ntfs,
        data: &'f [u8],
        position: u64,
        data_size: u64,
    ) -> Result<Self> {
        let mut stream_data_runs = NtfsDataRuns::new(ntfs, data, position);
        let mut stream_state = StreamState::new(data_size);

        // Get the first data run already here to let `data_position` return something meaningful.
        if let Some(stream_data_run) = stream_data_runs.next() {
            let stream_data_run = stream_data_run?;
            stream_state.set_stream_data_run(stream_data_run);
        }

        Ok(Self {
            ntfs,
            data,
            position,
            stream_data_runs,
            stream_state,
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
        self.stream_state.data_position()
    }

    pub fn data_runs(&self) -> NtfsDataRuns<'n, 'f> {
        NtfsDataRuns::new(self.ntfs, self.data, self.position)
    }

    pub fn len(&self) -> u64 {
        self.stream_state.data_size()
    }

    /// Returns whether we got another data run.
    fn next_data_run(&mut self) -> Result<bool> {
        let stream_data_run = match self.stream_data_runs.next() {
            Some(stream_data_run) => stream_data_run,
            None => return Ok(false),
        };
        let stream_data_run = stream_data_run?;
        self.stream_state.set_stream_data_run(stream_data_run);

        Ok(true)
    }

    pub fn ntfs(&self) -> &'n Ntfs {
        self.ntfs
    }

    /// Returns the absolute position of the data run information within the filesystem, in bytes.
    pub fn position(&self) -> u64 {
        self.position
    }
}

impl<'n, 'f> NtfsReadSeek for NtfsNonResidentAttributeValue<'n, 'f> {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek,
    {
        let mut bytes_read = 0usize;

        while bytes_read < buf.len() {
            // Read from the current data run if there is one.
            if self.stream_state.read_data_run(fs, buf, &mut bytes_read)? {
                // We read something, so check the loop condition again if we need to read more.
                continue;
            }

            // Move to the next data run.
            if self.next_data_run()? {
                // We got another data run, so read again.
                continue;
            } else {
                // We read everything we could.
                break;
            }
        }

        Ok(bytes_read)
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek,
    {
        let pos = self.stream_state.optimize_seek(pos, self.len())?;

        let mut bytes_left_to_seek = match pos {
            SeekFrom::Start(n) => {
                // Rewind to the very beginning.
                self.stream_data_runs = self.data_runs();
                self.stream_state = StreamState::new(self.len());
                n
            }
            SeekFrom::Current(n) if n >= 0 => n as u64,
            _ => unreachable!(),
        };

        while bytes_left_to_seek > 0 {
            // Seek inside the current data run if there is one.
            if self
                .stream_state
                .seek_data_run(fs, pos, &mut bytes_left_to_seek)?
            {
                // We have reached our final seek position.
                break;
            }

            // Move to the next data run.
            if self.next_data_run()? {
                // We got another data run, so seek some more.
                continue;
            } else {
                // We seeked as far as we could.
                break;
            }
        }

        match pos {
            SeekFrom::Start(n) => self.stream_state.set_stream_position(n),
            SeekFrom::Current(n) => self
                .stream_state
                .set_stream_position(self.stream_position() + n as u64),
            _ => unreachable!(),
        }

        Ok(self.stream_position())
    }

    fn stream_position(&self) -> u64 {
        self.stream_state.stream_position()
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
pub struct NtfsDataRuns<'n, 'f> {
    ntfs: &'n Ntfs,
    data: &'f [u8],
    position: u64,
    state: DataRunsState,
}

impl<'n, 'f> NtfsDataRuns<'n, 'f> {
    pub(crate) fn new(ntfs: &'n Ntfs, data: &'f [u8], position: u64) -> Self {
        let state = DataRunsState {
            offset: 0,
            previous_lcn: Lcn::from(0),
        };

        Self {
            ntfs,
            data,
            position,
            state,
        }
    }

    pub(crate) fn from_state(
        ntfs: &'n Ntfs,
        data: &'f [u8],
        position: u64,
        state: DataRunsState,
    ) -> Self {
        Self {
            ntfs,
            data,
            position,
            state,
        }
    }

    pub(crate) fn into_state(self) -> DataRunsState {
        self.state
    }

    pub fn position(&self) -> u64 {
        self.position + self.state.offset as u64
    }

    fn read_variable_length_bytes(
        &self,
        cursor: &mut Cursor<&[u8]>,
        byte_count: u8,
    ) -> Result<[u8; 8]> {
        const MAX_BYTE_COUNT: u8 = mem::size_of::<u64>() as u8;

        if byte_count > MAX_BYTE_COUNT {
            return Err(NtfsError::InvalidByteCountInDataRunHeader {
                position: self.position(),
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
        if self.state.offset >= self.data.len() {
            return None;
        }

        // Read the single header byte.
        let mut cursor = Cursor::new(&self.data[self.state.offset..]);
        let header = iter_try!(u8::read(&mut cursor));

        // A zero byte marks the end of the data runs.
        if header == 0 {
            // Ensure that any further call uses the fast path above.
            self.state.offset = self.data.len();
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
        let lcn = iter_try!(self.state.previous_lcn.checked_add(vcn).ok_or({
            NtfsError::InvalidVcnInDataRunHeader {
                position: NtfsDataRuns::position(self),
                vcn,
                previous_lcn: self.state.previous_lcn,
            }
        }));
        self.state.previous_lcn = lcn;

        // Only advance after having checked for success.
        // In case of an error, a subsequent call shall output the same error again.
        let bytes_to_advance = cursor.stream_position().unwrap() as usize;
        self.state.offset += bytes_to_advance;

        let data_run = iter_try!(NtfsDataRun::new(self.ntfs, lcn, cluster_count));
        Some(Ok(data_run))
    }
}

impl<'n, 'f> FusedIterator for NtfsDataRuns<'n, 'f> {}

#[derive(Clone, Debug)]
pub(crate) struct DataRunsState {
    offset: usize,
    previous_lcn: Lcn,
}

/// Describes a single NTFS data run, which is a continuous cluster range of a non-resident value.
///
/// A data run's size is a multiple of the cluster size configured for the filesystem.
/// However, a data run does not know about the actual size used by data. This information is only available in the corresponding attribute.
/// Keep this in mind when doing reads and seeks on data runs. You may end up on allocated but unused data.
#[derive(Clone, Debug)]
pub struct NtfsDataRun {
    /// Absolute position of the data run within the filesystem, in bytes.
    /// This may be zero if this is a "sparse" data run.
    position: u64,
    /// Total allocated size of the data run, in bytes.
    /// The actual size used by data may be lower, but a data run does not know about that.
    allocated_size: u64,
    /// Current relative position within the data run value, in bytes.
    stream_position: u64,
}

impl NtfsDataRun {
    pub(crate) fn new(ntfs: &Ntfs, lcn: Lcn, cluster_count: u64) -> Result<Self> {
        let position = lcn.position(ntfs)?;
        let allocated_size = cluster_count
            .checked_mul(ntfs.cluster_size() as u64)
            .ok_or(NtfsError::InvalidClusterCount { cluster_count })?;

        Ok(Self {
            position,
            allocated_size,
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
        self.allocated_size
    }

    pub(crate) fn remaining_len(&self) -> u64 {
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

        let bytes_to_read = usize::min(buf.len(), self.remaining_len() as usize);
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
pub(crate) struct StreamState {
    /// Current data run we are reading from.
    stream_data_run: Option<NtfsDataRun>,
    /// Current relative position within the entire value, in bytes.
    stream_position: u64,
    /// Total (used) data size, in bytes.
    data_size: u64,
}

impl StreamState {
    pub(crate) const fn new(data_size: u64) -> Self {
        Self {
            stream_data_run: None,
            stream_position: 0,
            data_size,
        }
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if:
    ///   * The current seek position is outside the valid range, or
    ///   * The current data run is a "sparse" data run
    pub(crate) fn data_position(&self) -> Option<u64> {
        let stream_data_run = self.stream_data_run.as_ref()?;
        stream_data_run.data_position()
    }

    /// Returns the total (used) data size of the value, in bytes.
    pub(crate) fn data_size(&self) -> u64 {
        self.data_size
    }

    pub(crate) fn optimize_seek(&self, pos: SeekFrom, data_size: u64) -> Result<SeekFrom> {
        let mut pos = self.simplify_seek(pos, data_size)?;

        // Translate `SeekFrom::Start(n)` into a more efficient `SeekFrom::Current` if n >= self.stream_position.
        // We don't need to traverse data runs from the very beginning then.
        if let SeekFrom::Start(n) = pos {
            if let Some(n_from_current) = n.checked_sub(self.stream_position()) {
                if let Ok(signed_n_from_current) = i64::try_from(n_from_current) {
                    pos = SeekFrom::Current(signed_n_from_current);
                }
            }
        }

        Ok(pos)
    }

    /// Simplifies any [`SeekFrom`] to the two cases [`SeekFrom::Start(n)`] and [`SeekFrom::Current(n)`], with n >= 0.
    /// This is necessary, because an NTFS data run has necessary information for the next data run, but not the other way round.
    /// Hence, we can't efficiently move backwards.
    fn simplify_seek(&self, pos: SeekFrom, data_size: u64) -> Result<SeekFrom> {
        match pos {
            SeekFrom::Start(n) => {
                // Seek n bytes from the very beginning.
                return Ok(SeekFrom::Start(n));
            }
            SeekFrom::End(n) => {
                if n >= 0 {
                    if let Some(bytes_to_seek) = data_size.checked_add(n as u64) {
                        // Seek data_size + n bytes from the very beginning.
                        return Ok(SeekFrom::Start(bytes_to_seek));
                    }
                } else {
                    if let Some(bytes_to_seek) = data_size.checked_sub(n.wrapping_neg() as u64) {
                        // Seek data_size + n bytes (with n being negative) from the very beginning.
                        return Ok(SeekFrom::Start(bytes_to_seek));
                    }
                }
            }
            SeekFrom::Current(n) => {
                if n >= 0 {
                    if self.stream_position().checked_add(n as u64).is_some() {
                        // Seek n bytes from the current position.
                        // This is an optimization for the common case, as we don't need to traverse all
                        // data runs from the very beginning.
                        return Ok(SeekFrom::Current(n));
                    }
                } else {
                    if let Some(bytes_to_seek) =
                        self.stream_position().checked_sub(n.wrapping_neg() as u64)
                    {
                        // Seek stream_position + n bytes (with n being negative) from the very beginning.
                        return Ok(SeekFrom::Start(bytes_to_seek));
                    }
                }
            }
        }

        Err(NtfsError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowing position",
        )))
    }

    /// Returns whether we read some bytes.
    pub(crate) fn read_data_run<T>(
        &mut self,
        fs: &mut T,
        buf: &mut [u8],
        bytes_read: &mut usize,
    ) -> Result<bool>
    where
        T: Read + Seek,
    {
        // Is there a data run to read from?
        let data_run = match &mut self.stream_data_run {
            Some(data_run) => data_run,
            None => return Ok(false),
        };

        // Have we already seeked past the size of the data run?
        if data_run.stream_position() >= data_run.len() {
            return Ok(false);
        }

        // We also must not read past the (used) data size of the entire value.
        // (remember that a data run only knows about its allocated size, not its used size!)
        let remaining_data_size = self.data_size.saturating_sub(self.stream_position);
        if remaining_data_size == 0 {
            return Ok(false);
        }

        // Read up to the buffer length or up to the (used) data size, whatever comes first.
        let start = *bytes_read;
        let remaining_buf_len = buf.len() - start;
        let end = start + usize::min(remaining_buf_len, remaining_data_size as usize);

        // Perform the actual read.
        let bytes_read_in_data_run = data_run.read(fs, &mut buf[start..end])?;
        *bytes_read += bytes_read_in_data_run;
        self.stream_position += bytes_read_in_data_run as u64;

        Ok(true)
    }

    /// Returns whether we have reached the final seek position within this data run and can therefore stop seeking.
    ///
    /// In all other cases, the caller should move to the next data run and seek again.
    pub(crate) fn seek_data_run<T>(
        &mut self,
        fs: &mut T,
        bytes_to_seek: SeekFrom,
        bytes_left_to_seek: &mut u64,
    ) -> Result<bool>
    where
        T: Read + Seek,
    {
        // Is there a data run to seek in?
        let data_run = match &mut self.stream_data_run {
            Some(data_run) => data_run,
            None => return Ok(false),
        };

        if *bytes_left_to_seek < data_run.remaining_len() {
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
                SeekFrom::Start(_) => SeekFrom::Start(*bytes_left_to_seek),
                SeekFrom::Current(_) => SeekFrom::Current(*bytes_left_to_seek as i64),
                _ => unreachable!(),
            };

            data_run.seek(fs, pos)?;
            Ok(true)
        } else {
            // We can skip the entire data run.
            *bytes_left_to_seek -= data_run.remaining_len();
            Ok(false)
        }
    }

    pub(crate) fn set_stream_data_run(&mut self, stream_data_run: NtfsDataRun) {
        self.stream_data_run = Some(stream_data_run);
    }

    pub(crate) fn set_stream_position(&mut self, stream_position: u64) {
        self.stream_position = stream_position;
    }

    /// Returns the current relative position within the entire value, in bytes.
    pub(crate) fn stream_position(&self) -> u64 {
        self.stream_position
    }
}
