// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use binread::BinRead;

#[derive(BinRead, Clone, Debug)]
pub struct NtfsFileReference(u64);

impl NtfsFileReference {
    pub(crate) const fn new(file_reference_data: u64) -> Self {
        Self(file_reference_data)
    }

    pub fn file_record_number(&self) -> u64 {
        self.0 >> 16
    }

    pub fn sequence_number(&self) -> u16 {
        self.0 as u16
    }
}
