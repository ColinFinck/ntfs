// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::index_record::NtfsIndexRecord;
use crate::ntfs::Ntfs;
use crate::structured_values::index_root::NtfsIndexRoot;
use crate::structured_values::NtfsStructuredValue;
use crate::traits::NtfsReadSeek;
use crate::types::Vcn;
use binread::io::{Read, Seek, SeekFrom};
use core::iter::FusedIterator;

#[derive(Clone, Debug)]
pub struct NtfsIndexAllocation<'n, 'f> {
    ntfs: &'n Ntfs,
    value: NtfsAttributeValue<'n, 'f>,
}

impl<'n, 'f> NtfsIndexAllocation<'n, 'f> {
    pub fn iter(&self, index_root: &NtfsIndexRoot) -> NtfsIndexRecords<'n, 'f> {
        let index_record_size = index_root.index_record_size();
        NtfsIndexRecords::new(self.clone(), index_record_size)
    }

    pub fn record_from_vcn<T>(
        &self,
        fs: &mut T,
        index_root: &NtfsIndexRoot,
        vcn: Vcn,
    ) -> Result<NtfsIndexRecord<'n>>
    where
        T: Read + Seek,
    {
        // Seek to the byte offset of the given VCN.
        let mut value = self.value.clone();
        let offset = vcn.offset(self.ntfs)?;
        value.seek(fs, SeekFrom::Current(offset))?;

        if value.stream_position() >= value.len() {
            return Err(NtfsError::VcnOutOfBoundsInIndexAllocation {
                position: self.value.data_position().unwrap(),
                vcn,
            });
        }

        // Get the record.
        let index_record_size = index_root.index_record_size();
        let record = NtfsIndexRecord::new(self.ntfs, fs, value, index_record_size)?;

        // Validate that the VCN in the record is the requested one.
        if record.vcn() != vcn {
            return Err(NtfsError::VcnMismatchInIndexAllocation {
                position: self.value.data_position().unwrap(),
                expected: vcn,
                actual: record.vcn(),
            });
        }

        Ok(record)
    }
}

impl<'n, 'f> NtfsStructuredValue<'n, 'f> for NtfsIndexAllocation<'n, 'f> {
    const TY: NtfsAttributeType = NtfsAttributeType::IndexAllocation;

    fn from_attribute_value<T>(_fs: &mut T, value: NtfsAttributeValue<'n, 'f>) -> Result<Self>
    where
        T: Read + Seek,
    {
        let ntfs = match &value {
            NtfsAttributeValue::AttributeListNonResident(value) => value.ntfs(),
            NtfsAttributeValue::NonResident(value) => value.ntfs(),
            NtfsAttributeValue::Resident(_) => {
                let position = value.data_position().unwrap();
                return Err(NtfsError::UnexpectedResidentAttribute { position });
            }
        };

        Ok(Self { ntfs, value })
    }
}

#[derive(Clone, Debug)]
pub struct NtfsIndexRecords<'n, 'f> {
    index_allocation: NtfsIndexAllocation<'n, 'f>,
    index_record_size: u32,
}

impl<'n, 'f> NtfsIndexRecords<'n, 'f> {
    fn new(index_allocation: NtfsIndexAllocation<'n, 'f>, index_record_size: u32) -> Self {
        Self {
            index_allocation,
            index_record_size,
        }
    }

    pub fn attach<'a, T>(self, fs: &'a mut T) -> NtfsIndexRecordsAttached<'n, 'f, 'a, T>
    where
        T: Read + Seek,
    {
        NtfsIndexRecordsAttached::new(fs, self)
    }

    pub fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsIndexRecord<'n>>>
    where
        T: Read + Seek,
    {
        if self.index_allocation.value.stream_position() >= self.index_allocation.value.len() {
            return None;
        }

        // Get the current record.
        let record = iter_try!(NtfsIndexRecord::new(
            self.index_allocation.ntfs,
            fs,
            self.index_allocation.value.clone(),
            self.index_record_size
        ));

        // Advance our iterator to the next record.
        iter_try!(self
            .index_allocation
            .value
            .seek(fs, SeekFrom::Current(self.index_record_size as i64)));

        Some(Ok(record))
    }
}

pub struct NtfsIndexRecordsAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fs: &'a mut T,
    index_records: NtfsIndexRecords<'n, 'f>,
}

impl<'n, 'f, 'a, T> NtfsIndexRecordsAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    fn new(fs: &'a mut T, index_records: NtfsIndexRecords<'n, 'f>) -> Self {
        Self { fs, index_records }
    }

    pub fn detach(self) -> NtfsIndexRecords<'n, 'f> {
        self.index_records
    }
}

impl<'n, 'f, 'a, T> Iterator for NtfsIndexRecordsAttached<'n, 'f, 'a, T>
where
    T: Read + Seek,
{
    type Item = Result<NtfsIndexRecord<'n>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.index_records.next(self.fs)
    }
}

impl<'n, 'f, 'a, T> FusedIterator for NtfsIndexRecordsAttached<'n, 'f, 'a, T> where T: Read + Seek {}
