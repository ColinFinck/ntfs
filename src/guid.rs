// Copyright 2021-2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt;

use zerocopy::{FromBytes, Immutable, KnownLayout, LittleEndian, Unaligned, U16, U32};

/// Size of a single GUID on disk (= size of all GUID fields).
pub(crate) const GUID_SIZE: usize = 16;

/// A Globally Unique Identifier (GUID), used for Object IDs in NTFS.
#[derive(Clone, Debug, Eq, FromBytes, Immutable, KnownLayout, PartialEq, Unaligned)]
#[repr(C, packed)]
pub struct NtfsGuid {
    data1: U32<LittleEndian>,
    data2: U16<LittleEndian>,
    data3: U16<LittleEndian>,
    data4: [u8; 8],
}

impl NtfsGuid {
    /// Returns the `data1` component of the GUID.
    pub fn data1(&self) -> u32 {
        self.data1.get()
    }

    /// Returns the `data2` component of the GUID.
    pub fn data2(&self) -> u16 {
        self.data2.get()
    }

    /// Returns the `data3` component of the GUID.
    pub fn data3(&self) -> u16 {
        self.data3.get()
    }

    /// Returns the `data4` component of the GUID.
    pub fn data4(&self) -> [u8; 8] {
        self.data4
    }
}

impl fmt::Display for NtfsGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:8X}-{:4X}-{:4X}-{:2X}{:2X}-{:2X}{:2X}{:2X}{:2X}{:2X}{:2X}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid() {
        let guid = NtfsGuid {
            data1: U32::new(0x67c8770b),
            data2: U16::new(0x44f1),
            data3: U16::new(0x410a),
            data4: [0xab, 0x9a, 0xf9, 0xb5, 0x44, 0x6f, 0x13, 0xee],
        };
        let guid_string = guid.to_string();
        assert_eq!(guid_string, "67C8770B-44F1-410A-AB9A-F9B5446F13EE");
    }
}
