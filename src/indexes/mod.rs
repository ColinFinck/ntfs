// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

mod file_name;

pub use file_name::*;

use crate::error::Result;
use core::fmt;

pub trait NtfsIndexEntryType: fmt::Debug {
    type KeyType: NtfsIndexEntryKey;
}

pub trait NtfsIndexEntryKey: fmt::Debug + Sized {
    fn key_from_slice(slice: &[u8], position: u64) -> Result<Self>;
}

/// Indicates that the index entry type has additional data.
// This would benefit from negative trait bounds, as this trait and `NtfsIndexEntryHasFileReference` are mutually exclusive!
pub trait NtfsIndexEntryHasData: NtfsIndexEntryType {
    type DataType: NtfsIndexEntryData;
}

pub trait NtfsIndexEntryData: fmt::Debug + Sized {
    fn data_from_slice(slice: &[u8], position: u64) -> Result<Self>;
}

/// Indicates that the index entry type has a file reference.
// This would benefit from negative trait bounds, as this trait and `NtfsIndexEntryHasData` are mutually exclusive!
pub trait NtfsIndexEntryHasFileReference: NtfsIndexEntryType {}
