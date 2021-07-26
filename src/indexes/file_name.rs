// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::indexes::{NtfsIndexEntryHasFileReference, NtfsIndexEntryType};
use crate::structured_values::NtfsFileName;

#[derive(Debug)]
pub struct NtfsFileNameIndex {}

impl NtfsIndexEntryType for NtfsFileNameIndex {
    type KeyType = NtfsFileName;
}

impl NtfsIndexEntryHasFileReference for NtfsFileNameIndex {}
