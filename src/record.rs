// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use binread::BinRead;

const UPDATE_SEQUENCE_ELEMENT_SIZE: u32 = 2;

#[allow(unused)]
#[derive(BinRead, Clone, Debug)]
pub(crate) struct RecordHeader {
    pub(crate) signature: [u8; 4],
    update_sequence_array_offset: u16,
    update_sequence_array_count: u16,
    logfile_sequence_number: u64,
}

impl RecordHeader {
    pub(crate) fn update_sequence_array_size(&self) -> u32 {
        self.update_sequence_array_count as u32 * UPDATE_SEQUENCE_ELEMENT_SIZE
    }
}
