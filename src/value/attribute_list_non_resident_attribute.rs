// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later
//
//! This module implements a reader for a non-resident attribute value that is part of an AttributeList.
//! Such values are not only split up into data runs, but may also be continued by connected attributes which are listed in the same AttributeList.
//! This reader provides one contiguous data stream for all data runs in all connected attributes.
//
// It is important to note that `NtfsAttributeListNonResidentAttributeValue` can't just encapsulate `NtfsNonResidentAttributeValue` and provide one
// layer on top to connect the attributes!
// Connected attributes are stored in a way that the first attribute reports the entire data size and all further attributes report a zero value length.
// We have to go down to the data run level to get trustable lengths again, and this is what `NtfsAttributeListNonResidentAttributeValue` does here.

use crate::attribute::{NtfsAttribute, NtfsAttributeType};
use crate::error::{NtfsError, Result};
use crate::file::NtfsFile;
use crate::ntfs::Ntfs;
use crate::structured_values::{NtfsAttributeListEntries, NtfsAttributeListEntry};
use crate::traits::NtfsReadSeek;
use crate::value::non_resident_attribute::{DataRunsState, NtfsDataRuns, StreamState};
use binread::io::{Read, Seek, SeekFrom};

#[derive(Clone, Debug)]
pub struct NtfsAttributeListNonResidentAttributeValue<'n, 'f> {
    /// Reference to the base `Ntfs` object of this filesystem.
    ntfs: &'n Ntfs,
    /// An untouched copy of the `attribute_list_entries` passed in [`Self::new`] to rewind to the beginning when desired.
    initial_attribute_list_entries: NtfsAttributeListEntries<'n, 'f>,
    /// Iterator through all connected attributes of this attribute in the AttributeList.
    connected_entries: AttributeListConnectedEntries<'n, 'f>,
    /// File, location, and data runs iteration state of the current attribute.
    attribute_state: Option<AttributeState<'n>>,
    /// Iteration state of the current data run.
    stream_state: StreamState,
}

impl<'n, 'f> NtfsAttributeListNonResidentAttributeValue<'n, 'f> {
    pub(crate) fn new(
        ntfs: &'n Ntfs,
        attribute_list_entries: NtfsAttributeListEntries<'n, 'f>,
        instance: u16,
        ty: NtfsAttributeType,
        data_size: u64,
    ) -> Self {
        let connected_entries =
            AttributeListConnectedEntries::new(attribute_list_entries.clone(), instance, ty);

        Self {
            ntfs,
            initial_attribute_list_entries: attribute_list_entries,
            connected_entries,
            attribute_state: None,
            stream_state: StreamState::new(data_size),
        }
    }

    /// Returns the absolute current data seek position within the filesystem, in bytes.
    /// This may be `None` if:
    ///   * The current seek position is outside the valid range, or
    ///   * The current data run is a "sparse" data run
    pub fn data_position(&self) -> Option<u64> {
        self.stream_state.data_position()
    }

    pub fn len(&self) -> u64 {
        self.stream_state.data_size()
    }

    /// Returns whether we got another data run.
    fn next_data_run(&mut self) -> Result<bool> {
        // Do we have a file and a (non-resident) attribute to iterate through its data runs?
        let attribute_state = match &mut self.attribute_state {
            Some(attribute_state) => attribute_state,
            None => return Ok(false),
        };

        // Get the state of the `NtfsDataRuns` iterator of that attribute.
        let data_runs_state = match attribute_state.data_runs_state.take() {
            Some(data_runs_state) => data_runs_state,
            None => return Ok(false),
        };

        // Deserialize the state into an `NtfsDataRuns` iterator.
        let attribute = NtfsAttribute::new(
            &attribute_state.file,
            attribute_state.attribute_offset,
            None,
        );
        let (data, position) = attribute.non_resident_value_data_and_position();
        let mut stream_data_runs =
            NtfsDataRuns::from_state(self.ntfs, data, position, data_runs_state);

        // Do we have a next data run? Save that.
        let stream_data_run = match stream_data_runs.next() {
            Some(stream_data_run) => stream_data_run,
            None => return Ok(false),
        };
        let stream_data_run = stream_data_run?;
        self.stream_state.set_stream_data_run(stream_data_run);

        // We got another data run, so serialize the updated `NtfsDataRuns` state for the next iteration.
        // This step is skipped when we got no data run, because it means we have fully iterated this iterator (and hence also the attribute and file).
        attribute_state.data_runs_state = Some(stream_data_runs.into_state());

        Ok(true)
    }

    /// Returns whether we got another connected attribute.
    fn next_attribute<T>(&mut self, fs: &mut T) -> Result<bool>
    where
        T: Read + Seek,
    {
        // Do we have another connected attribute?
        let entry = match self.connected_entries.next(fs) {
            Some(entry) => entry,
            None => return Ok(false),
        };

        // Read the correspoding FILE record into an `NtfsFile` and get the corresponding `NtfsAttribute`.
        let entry = entry?;
        let file = entry.to_file(self.ntfs, fs)?;
        let attribute = entry.to_attribute(&file)?;
        let attribute_offset = attribute.offset();

        // Connected attributes must always be non-resident. Verify that.
        if attribute.is_resident() {
            return Err(NtfsError::UnexpectedResidentAttribute {
                position: attribute.position(),
            });
        }

        // Get an `NtfsDataRuns` iterator for iterating through the attribute value's data runs.
        let (data, position) = attribute.non_resident_value_data_and_position();
        let mut stream_data_runs = NtfsDataRuns::new(self.ntfs, data, position);

        // Get the first data run already here to save time and let `data_position` return something meaningful.
        let stream_data_run = match stream_data_runs.next() {
            Some(stream_data_run) => stream_data_run,
            None => return Ok(false),
        };
        let stream_data_run = stream_data_run?;
        self.stream_state.set_stream_data_run(stream_data_run);

        // Store the `NtfsFile` and serialize the `NtfsDataRuns` state for a later iteration.
        let data_runs_state = Some(stream_data_runs.into_state());
        self.attribute_state = Some(AttributeState {
            file,
            attribute_offset,
            data_runs_state,
        });

        Ok(true)
    }

    pub fn ntfs(&self) -> &'n Ntfs {
        self.ntfs
    }
}

impl<'n, 'f> NtfsReadSeek for NtfsAttributeListNonResidentAttributeValue<'n, 'f> {
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

            // Move to the next data run of the current attribute.
            if self.next_data_run()? {
                // We got another data run of the current attribute, so read again.
                continue;
            }

            // Move to the first data run of the next connected attribute.
            if self.next_attribute(fs)? {
                // We got another attribute, so read again.
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
                self.connected_entries.attribute_list_entries =
                    Some(self.initial_attribute_list_entries.clone());
                self.attribute_state = None;
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

            // Move to the next data run of the current attribute.
            if self.next_data_run()? {
                // We got another data run of the current attribute, so seek some more.
                continue;
            }

            // Move to the first data run of the next connected attribute.
            if self.next_attribute(fs)? {
                // We got another connected attribute, so seek some more.
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

#[derive(Clone, Debug)]
struct AttributeListConnectedEntries<'n, 'f> {
    attribute_list_entries: Option<NtfsAttributeListEntries<'n, 'f>>,
    instance: u16,
    ty: NtfsAttributeType,
}

impl<'n, 'f> AttributeListConnectedEntries<'n, 'f> {
    fn new(
        attribute_list_entries: NtfsAttributeListEntries<'n, 'f>,
        instance: u16,
        ty: NtfsAttributeType,
    ) -> Self {
        Self {
            attribute_list_entries: Some(attribute_list_entries),
            instance,
            ty,
        }
    }

    fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsAttributeListEntry>>
    where
        T: Read + Seek,
    {
        let attribute_list_entries = self.attribute_list_entries.as_mut()?;

        let entry = iter_try!(attribute_list_entries.next(fs)?);
        if entry.instance() == self.instance && iter_try!(entry.ty()) == self.ty {
            Some(Ok(entry))
        } else {
            self.attribute_list_entries = None;
            None
        }
    }
}

#[derive(Clone, Debug)]
struct AttributeState<'n> {
    file: NtfsFile<'n>,
    attribute_offset: usize,
    /// We cannot store an `NtfsDataRuns` here, because it has a reference to the `NtfsFile` that is also stored here.
    /// This is why we have to go via `DataRunsState` in an `Option` to take() it and deserialize it into an `NtfsDataRuns` whenever necessary.
    data_runs_state: Option<DataRunsState>,
}
