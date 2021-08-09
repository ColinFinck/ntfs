// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use binread::io::{Read, Seek};
use binread::BinRead;

#[derive(BinRead, Clone, Copy, Debug)]
pub struct NtfsFileReference([u8; 8]);

impl NtfsFileReference {
    pub(crate) const fn new(file_reference_bytes: [u8; 8]) -> Self {
        Self(file_reference_bytes)
    }

    pub fn file_record_number(&self) -> u64 {
        u64::from_le_bytes(self.0) & 0xffff_ffff_ffff
    }

    pub fn sequence_number(&self) -> u16 {
        (u64::from_le_bytes(self.0) >> 48) as u16
    }
}
