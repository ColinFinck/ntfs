// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::index_record::NtfsIndexRecord;
use crate::ntfs::Ntfs;
use crate::structured_values::index_root::NtfsIndexRoot;
use crate::structured_values::NewNtfsStructuredValue;
use crate::traits::NtfsReadSeek;
use crate::types::Vcn;
use binread::io::{Read, Seek, SeekFrom};
use core::iter::FusedIterator;

#[derive(Clone, Debug)]
pub struct NtfsIndexAllocation<'n> {
    ntfs: &'n Ntfs,
    value: NtfsAttributeValue<'n>,
}

impl<'n> NtfsIndexAllocation<'n> {
    pub fn iter(&self, index_root: &NtfsIndexRoot<'n>) -> NtfsIndexRecords<'n> {
        let index_record_size = index_root.index_record_size();
        NtfsIndexRecords::new(self.ntfs, self.value.clone(), index_record_size)
    }

    pub fn record_from_vcn<T>(
        &self,
        fs: &mut T,
        index_root: &NtfsIndexRoot<'n>,
        vcn: Vcn,
    ) -> Result<NtfsIndexRecord<'n>>
    where
        T: Read + Seek,
    {
        // Seek to the byte offset of the given VCN.
        let mut value = self.value.clone();
        let offset = vcn.offset(self.ntfs)?;
        value.seek(fs, SeekFrom::Current(offset))?;

        // Get the record.
        let index_record_size = index_root.index_record_size();
        let record = NtfsIndexRecord::new(self.ntfs, fs, value, index_record_size)?;

        // Validate that the VCN in the record is the requested one.
        if record.vcn() != vcn {
            return Err(NtfsError::VcnMismatch {
                requested_vcn: vcn,
                record_vcn: record.vcn(),
            });
        }

        Ok(record)
    }
}

impl<'n> NewNtfsStructuredValue<'n> for NtfsIndexAllocation<'n> {
    fn new<T>(
        ntfs: &'n Ntfs,
        _fs: &mut T,
        value: NtfsAttributeValue<'n>,
        _length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        Ok(Self { ntfs, value })
    }
}

#[derive(Clone, Debug)]
pub struct NtfsIndexRecords<'n> {
    ntfs: &'n Ntfs,
    value: NtfsAttributeValue<'n>,
    index_record_size: u32,
}

impl<'n> NtfsIndexRecords<'n> {
    fn new(ntfs: &'n Ntfs, value: NtfsAttributeValue<'n>, index_record_size: u32) -> Self {
        Self {
            ntfs,
            value,
            index_record_size,
        }
    }

    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsIndexRecordsAttached<'n, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsIndexRecordsAttached::new(fs, self)
    }

    pub fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsIndexRecord<'n>>>
    where
        T: Read + Seek,
    {
        if self.value.stream_position() >= self.value.len() {
            return None;
        }

        // Get the current record.
        let record = iter_try!(NtfsIndexRecord::new(
            self.ntfs,
            fs,
            self.value.clone(),
            self.index_record_size
        ));

        // Advance our iterator to the next record.
        iter_try!(self
            .value
            .seek(fs, SeekFrom::Current(self.index_record_size as i64)));

        Some(Ok(record))
    }
}

pub struct NtfsIndexRecordsAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    fs: &'a mut T,
    index_records: NtfsIndexRecords<'n>,
}

impl<'n, 'a, T> NtfsIndexRecordsAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, index_records: NtfsIndexRecords<'n>) -> Self {
        Self { fs, index_records }
    }

    pub fn detach(self) -> NtfsIndexRecords<'n> {
        self.index_records
    }
}

impl<'n, 'a, T> Iterator for NtfsIndexRecordsAttached<'n, 'a, T>
where
    T: Read + Seek,
{
    type Item = Result<NtfsIndexRecord<'n>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.index_records.next(self.fs)
    }
}

impl<'n, 'a, T> FusedIterator for NtfsIndexRecordsAttached<'n, 'a, T> where T: Read + Seek {}
