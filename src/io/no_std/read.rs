// Copyright 2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! Mostly imported from https://github.com/rust-lang/rust/blob/561364e4d5ccc506f610208a4989e91fdbdc8ca7/library/std/src/io/mod.rs

use super::{Error, ErrorKind, Result};

/// Simplified version of [`std::io::Read`] for `no_std` environments.
///
/// See its documentation for more details.
pub trait Read {
    /// See [`std::io::Read::read`].
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// See [`std::io::Read::read_exact`].
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            Err(Error::new(ErrorKind::UnexpectedEof, ()))
        } else {
            Ok(())
        }
    }
}
