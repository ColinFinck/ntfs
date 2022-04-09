// Copyright 2021-2022 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::mem;

use binread::io::{Read, Seek};

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::file::KnownNtfsFileRecordNumber;
use crate::ntfs::Ntfs;
use crate::traits::NtfsReadSeek;

/// The Upcase Table contains an uppercase character for each Unicode character of the Basic Multilingual Plane.
const UPCASE_CHARACTER_COUNT: usize = 65536;

/// Hence, the table has a size of 128 KiB.
const UPCASE_TABLE_SIZE: u64 = (UPCASE_CHARACTER_COUNT * mem::size_of::<u16>()) as u64;

/// Manages a table for converting characters to uppercase.
/// This table is used for case-insensitive file name comparisons.
///
/// NTFS stores such a table in the special $UpCase file on every filesystem.
/// As this table is slightly different depending on the Windows version used for creating the filesystem,
/// it is very important to always read the table from the filesystem itself.
/// Hence, this table is not hardcoded into the crate.
#[derive(Clone, Debug)]
pub(crate) struct UpcaseTable {
    uppercase_characters: Vec<u16>,
}

impl UpcaseTable {
    /// Reads the $UpCase file from the given filesystem into a new [`UpcaseTable`] object.
    pub(crate) fn read<T>(ntfs: &Ntfs, fs: &mut T) -> Result<Self>
    where
        T: Read + Seek,
    {
        // Lookup the $UpCase file and its $DATA attribute.
        let upcase_file = ntfs.file(fs, KnownNtfsFileRecordNumber::UpCase as u64)?;
        let data_item = upcase_file
            .data(fs, "")
            .ok_or(NtfsError::AttributeNotFound {
                position: upcase_file.position(),
                ty: NtfsAttributeType::Data,
            })??;

        let data_attribute = data_item.to_attribute();
        if data_attribute.value_length() != UPCASE_TABLE_SIZE {
            return Err(NtfsError::InvalidUpcaseTableSize {
                expected: UPCASE_TABLE_SIZE,
                actual: data_attribute.value_length(),
            });
        }

        // Read the entire raw data from the $DATA attribute.
        let mut data_value = data_attribute.value(fs)?;
        let mut data = vec![0u8; UPCASE_TABLE_SIZE as usize];
        data_value.read_exact(fs, &mut data)?;

        // Store it in an array of `u16` uppercase characters.
        // Any endianness conversion is done here once, which makes `u16_to_uppercase` fast.
        let uppercase_characters = data
            .chunks_exact(2)
            .map(|two_bytes| u16::from_le_bytes(two_bytes.try_into().unwrap()))
            .collect();

        Ok(Self {
            uppercase_characters,
        })
    }

    /// Returns the uppercase variant of the given UCS-2 character (i.e. a Unicode character
    /// from the Basic Multilingual Plane) based on the stored conversion table.
    /// A character without an uppercase equivalent is returned as-is.
    pub(crate) fn u16_to_uppercase(&self, character: u16) -> u16 {
        self.uppercase_characters[character as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upcase_table() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let upcase_table = UpcaseTable::read(&ntfs, &mut testfs1).unwrap();

        // Prove that at least the lowercase English characters are mapped to their uppercase equivalents.
        // It makes no sense to check everything here.
        for (lowercase, uppercase) in (b'a'..=b'z').zip(b'A'..=b'Z') {
            assert_eq!(
                upcase_table.u16_to_uppercase(lowercase as u16),
                uppercase as u16
            );
        }
    }
}
