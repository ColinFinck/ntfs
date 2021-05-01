// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use alloc::string::String;
use binread::io::{Read, Seek, SeekFrom};
use core::char;
use core::cmp::Ordering;
use core::convert::TryInto;
use core::fmt;

/// Zero-copy representation of a string stored in an NTFS filesystem structure.
#[derive(Clone, Debug, Eq)]
pub struct NtfsString<'a>(pub &'a [u8]);

impl<'a> NtfsString<'a> {
    fn cmp_iter<TI, OI>(mut this_iter: TI, mut other_iter: OI) -> Ordering
    where
        TI: Iterator<Item = u16>,
        OI: Iterator<Item = u16>,
    {
        loop {
            match (this_iter.next(), other_iter.next()) {
                (Some(this_code_unit), Some(other_code_unit)) => {
                    // We have two UTF-16 code units to compare.
                    if this_code_unit != other_code_unit {
                        return this_code_unit.cmp(&other_code_unit);
                    }
                }
                (Some(_), None) => {
                    // `this_iter` is longer than `other_iter` but otherwise equal.
                    return Ordering::Greater;
                }
                (None, Some(_)) => {
                    // `other_iter` is longer than `this_iter` but otherwise equal.
                    return Ordering::Less;
                }
                (None, None) => {
                    // We made it to the end of both strings, so they must be equal.
                    return Ordering::Equal;
                }
            }
        }
    }

    fn cmp_str(&self, other: &str) -> Ordering {
        let other_iter = other.encode_utf16();
        Self::cmp_iter(self.utf16le_iter(), other_iter)
    }

    fn utf16le_iter(&'a self) -> impl Iterator<Item = u16> + 'a {
        self.0
            .chunks_exact(2)
            .map(|two_bytes| u16::from_le_bytes(two_bytes.try_into().unwrap()))
    }

    /// Returns `true` if `self` has a length of zero bytes.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of `self`.
    ///
    /// This length is in bytes, not characters! In other words,
    /// it may not be what a human considers the length of the string.
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn read_from_fs<T>(
        fs: &mut T,
        position: u64,
        length: usize,
        buf: &'a mut [u8],
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if buf.len() < length {
            return Err(NtfsError::BufferTooSmall {
                expected: length,
                actual: buf.len(),
            });
        }

        fs.seek(SeekFrom::Start(position))?;
        fs.read_exact(&mut buf[..length])?;

        Ok(Self(&buf[..length]))
    }

    /// Attempts to convert `self` to an owned `String`.
    /// Returns `Some(String)` if all characters could be converted successfully or `None` if a decoding error occurred.
    pub fn to_string_checked(&self) -> Option<String> {
        char::decode_utf16(self.utf16le_iter())
            .map(|x| x.ok())
            .collect::<Option<String>>()
    }

    /// Converts `self` to an owned `String`, replacing invalid data with the replacement character (U+FFFD).
    pub fn to_string_lossy(&self) -> String {
        char::decode_utf16(self.utf16le_iter())
            .map(|x| x.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
}

impl<'a> fmt::Display for NtfsString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let utf16_iter = char::decode_utf16(self.utf16le_iter())
            .map(|x| x.unwrap_or(char::REPLACEMENT_CHARACTER));

        for single_char in utf16_iter {
            single_char.fmt(f)?;
        }

        Ok(())
    }
}

impl<'a> Ord for NtfsString<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        Self::cmp_iter(self.utf16le_iter(), other.utf16le_iter())
    }
}

impl<'a> PartialEq for NtfsString<'a> {
    /// Checks that two strings are a (case-sensitive!) match.
    fn eq(&self, other: &Self) -> bool {
        let ordering = self.cmp(other);
        ordering == Ordering::Equal
    }
}

impl<'a> PartialEq<str> for NtfsString<'a> {
    fn eq(&self, other: &str) -> bool {
        self.cmp_str(other) == Ordering::Equal
    }
}

impl<'a> PartialEq<NtfsString<'a>> for str {
    fn eq(&self, other: &NtfsString<'a>) -> bool {
        other.cmp_str(self) == Ordering::Equal
    }
}

impl<'a> PartialEq<&str> for NtfsString<'a> {
    fn eq(&self, other: &&str) -> bool {
        self.cmp_str(other) == Ordering::Equal
    }
}

impl<'a> PartialEq<NtfsString<'a>> for &str {
    fn eq(&self, other: &NtfsString<'a>) -> bool {
        other.cmp_str(self) == Ordering::Equal
    }
}

impl<'a> PartialOrd for NtfsString<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> PartialOrd<str> for NtfsString<'a> {
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        Some(self.cmp_str(other))
    }
}

impl<'a> PartialOrd<NtfsString<'a>> for str {
    fn partial_cmp(&self, other: &NtfsString<'a>) -> Option<Ordering> {
        Some(other.cmp_str(self))
    }
}

impl<'a> PartialOrd<&str> for NtfsString<'a> {
    fn partial_cmp(&self, other: &&str) -> Option<Ordering> {
        Some(self.cmp_str(other))
    }
}

impl<'a> PartialOrd<NtfsString<'a>> for &str {
    fn partial_cmp(&self, other: &NtfsString<'a>) -> Option<Ordering> {
        Some(other.cmp_str(self))
    }
}
