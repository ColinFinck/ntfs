// Copyright 2021-2022 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::char;
use core::cmp::Ordering;
use core::convert::identity;
use core::fmt;

use alloc::string::String;

use crate::ntfs::Ntfs;

/// Zero-copy representation of a string stored in an NTFS filesystem structure.
#[derive(Clone, Debug, Eq)]
pub struct NtfsString<'a>(pub &'a [u8]);

impl<'a> NtfsString<'a> {
    fn cmp_iter<TI, OI, F>(mut this_iter: TI, mut other_iter: OI, code_unit_fn: F) -> Ordering
    where
        TI: Iterator<Item = u16>,
        OI: Iterator<Item = u16>,
        F: Fn(u16) -> u16,
    {
        loop {
            match (this_iter.next(), other_iter.next()) {
                (Some(this_code_unit), Some(other_code_unit)) => {
                    // We have two UTF-16 code units to compare.
                    let this_upper = code_unit_fn(this_code_unit);
                    let other_upper = code_unit_fn(other_code_unit);

                    if this_upper != other_upper {
                        return this_upper.cmp(&other_upper);
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

    fn u16_iter(&'a self) -> impl Iterator<Item = u16> + 'a {
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

    /// Attempts to convert `self` to an owned `String`.
    /// Returns `Some(String)` if all characters could be converted successfully or `None` if a decoding error occurred.
    pub fn to_string_checked(&self) -> Option<String> {
        char::decode_utf16(self.u16_iter())
            .map(|x| x.ok())
            .collect::<Option<String>>()
    }

    /// Converts `self` to an owned `String`, replacing invalid data with the replacement character (U+FFFD).
    pub fn to_string_lossy(&self) -> String {
        char::decode_utf16(self.u16_iter())
            .map(|x| x.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
}

impl<'a> fmt::Display for NtfsString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let utf16_iter =
            char::decode_utf16(self.u16_iter()).map(|x| x.unwrap_or(char::REPLACEMENT_CHARACTER));

        for single_char in utf16_iter {
            single_char.fmt(f)?;
        }

        Ok(())
    }
}

impl<'a> Ord for NtfsString<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        NtfsString::cmp_iter(self.u16_iter(), other.u16_iter(), identity)
    }
}

impl<'a> PartialEq for NtfsString<'a> {
    /// Checks that two strings are a (case-sensitive!) match.
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<'a> PartialEq<str> for NtfsString<'a> {
    fn eq(&self, other: &str) -> bool {
        NtfsString::cmp_iter(self.u16_iter(), other.encode_utf16(), identity) == Ordering::Equal
    }
}

impl<'a> PartialEq<NtfsString<'a>> for str {
    fn eq(&self, other: &NtfsString<'a>) -> bool {
        NtfsString::cmp_iter(self.encode_utf16(), other.u16_iter(), identity) == Ordering::Equal
    }
}

impl<'a> PartialEq<&str> for NtfsString<'a> {
    fn eq(&self, other: &&str) -> bool {
        NtfsString::cmp_iter(self.u16_iter(), other.encode_utf16(), identity) == Ordering::Equal
    }
}

impl<'a> PartialEq<NtfsString<'a>> for &str {
    fn eq(&self, other: &NtfsString<'a>) -> bool {
        NtfsString::cmp_iter(self.encode_utf16(), other.u16_iter(), identity) == Ordering::Equal
    }
}

impl<'a> PartialOrd for NtfsString<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> PartialOrd<str> for NtfsString<'a> {
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        Some(NtfsString::cmp_iter(
            self.u16_iter(),
            other.encode_utf16(),
            identity,
        ))
    }
}

impl<'a> PartialOrd<NtfsString<'a>> for str {
    fn partial_cmp(&self, other: &NtfsString<'a>) -> Option<Ordering> {
        Some(NtfsString::cmp_iter(
            self.encode_utf16(),
            other.u16_iter(),
            identity,
        ))
    }
}

impl<'a> PartialOrd<&str> for NtfsString<'a> {
    fn partial_cmp(&self, other: &&str) -> Option<Ordering> {
        Some(NtfsString::cmp_iter(
            self.u16_iter(),
            other.encode_utf16(),
            identity,
        ))
    }
}

impl<'a> PartialOrd<NtfsString<'a>> for &str {
    fn partial_cmp(&self, other: &NtfsString<'a>) -> Option<Ordering> {
        Some(NtfsString::cmp_iter(
            self.encode_utf16(),
            other.u16_iter(),
            identity,
        ))
    }
}

/// Trait for a case-insensitive ordering with respect to the $UpCase table read from the filesystem.
pub trait UpcaseOrd<Rhs> {
    /// Performs a case-insensitive ordering based on the $UpCase table read from the filesystem.
    ///
    /// # Panics
    ///
    /// Panics if [`read_upcase_table`][Ntfs::read_upcase_table] had not been called on the passed [`Ntfs`] object.
    fn upcase_cmp(&self, ntfs: &Ntfs, other: &Rhs) -> Ordering;
}

impl<'a> UpcaseOrd<NtfsString<'a>> for NtfsString<'a> {
    fn upcase_cmp(&self, ntfs: &Ntfs, other: &NtfsString<'a>) -> Ordering {
        let upcase_fn = |x| ntfs.upcase_table().u16_to_uppercase(x);
        NtfsString::cmp_iter(self.u16_iter(), other.u16_iter(), upcase_fn)
    }
}

impl<'a> UpcaseOrd<&str> for NtfsString<'a> {
    fn upcase_cmp(&self, ntfs: &Ntfs, other: &&str) -> Ordering {
        let upcase_fn = |x| ntfs.upcase_table().u16_to_uppercase(x);
        NtfsString::cmp_iter(self.u16_iter(), other.encode_utf16(), upcase_fn)
    }
}

impl<'a> UpcaseOrd<NtfsString<'a>> for &str {
    fn upcase_cmp(&self, ntfs: &Ntfs, other: &NtfsString<'a>) -> Ordering {
        let upcase_fn = |x| ntfs.upcase_table().u16_to_uppercase(x);
        NtfsString::cmp_iter(self.encode_utf16(), other.u16_iter(), upcase_fn)
    }
}
