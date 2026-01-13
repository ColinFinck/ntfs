// Copyright 2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! Mostly imported from https://github.com/rust-lang/rust/blob/561364e4d5ccc506f610208a4989e91fdbdc8ca7/library/std/src/io/error.rs

use core::fmt;

/// A specialized [`Result`] type for I/O operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Simplified version of [`std::io::Error`] for `no_std` environments.
///
/// See its documentation for more details.
#[derive(Debug)]
pub struct Error(ErrorKind);

impl Error {
    /// Creates a new I/O error from a known kind of error.
    ///
    /// The second parameter is always ignored in this simplified `no_std` version of `Error`.
    pub fn new<E>(kind: ErrorKind, _error: E) -> Error {
        Self(kind)
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    #[inline]
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.0
    }
}

/// Simplified version of [`std::io::ErrorKind`] for `no_std` environments.
///
/// See its documentation for more details.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A parameter was incorrect.
    InvalidInput,
    /// This operation was interrupted.
    Interrupted,
    /// An error returned when an operation could not be completed because an "end of file" was reached prematurely.
    UnexpectedEof,
    /// A custom error that does not fall under any other I/O error kind.
    Other,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
