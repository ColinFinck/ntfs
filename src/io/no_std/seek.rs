// Copyright 2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! Mostly imported from https://github.com/rust-lang/rust/blob/561364e4d5ccc506f610208a4989e91fdbdc8ca7/library/std/src/io/mod.rs

use super::Result;

/// Simplified version of [`std::io::Seek`] for `no_std` environments.
///
/// See its documentation for more details.
pub trait Seek {
    /// See [`std::io::Seek::seek`].
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;

    /// See [`std::io::Seek::rewind`].
    fn rewind(&mut self) -> Result<()> {
        self.seek(SeekFrom::Start(0))?;
        Ok(())
    }

    /// See [`std::io::Seek::stream_position`].
    fn stream_position(&mut self) -> Result<u64> {
        self.seek(SeekFrom::Current(0))
    }
}

/// Enumeration of possible methods to seek within an I/O object.
///
/// It is used by the [`Seek`] trait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeekFrom {
    /// Sets the offset to the provided number of bytes.
    Start(u64),

    /// Sets the offset to the size of this object plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of an object, but it's an error to
    /// seek before byte 0.
    End(i64),

    /// Sets the offset to the current position plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of an object, but it's an error to
    /// seek before byte 0.
    Current(i64),
}
