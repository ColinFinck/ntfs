// Copyright 2021-2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::cmp;

use zerocopy::{FromBytes, Immutable, KnownLayout, Unaligned};

use crate::error::Result;
use crate::io;
use crate::io::Read;

macro_rules! iter_try {
    ($e:expr) => {
        match $e {
            Ok(x) => x,
            Err(e) => return Some(Err(e.into())),
        }
    };
}

/// A simplified `std::io::Cursor`-like type that implements only `Read` but not `Seek`.
///
/// This is all we need inside this crate.
pub(crate) struct ReadOnlyCursor<'a>(&'a [u8]);

impl<'a> ReadOnlyCursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self(data)
    }
}

impl<'a> Read for ReadOnlyCursor<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_to_read = cmp::min(self.0.len(), buf.len());
        buf[..bytes_to_read].copy_from_slice(&self.0[..bytes_to_read]);
        self.0 = &self.0[bytes_to_read..];

        Ok(bytes_to_read)
    }
}

/// Reads a plain old data structure that implements `zerocopy` traits via `crate::io::Read`.
#[inline(always)]
#[track_caller]
pub(crate) fn read_pod<T, Pod, const LEN: usize>(r: &mut T) -> Result<Pod>
where
    T: Read,
    Pod: FromBytes + Immutable + KnownLayout + Unaligned,
{
    let mut bytes = [0u8; LEN];
    r.read_exact(&mut bytes)?;
    Ok(Pod::read_from_bytes(&bytes).unwrap())
}

#[cfg(test)]
pub mod tests {
    use std::fs::File;
    use std::io::{Cursor, Read};

    pub fn testfs1() -> Cursor<Vec<u8>> {
        let mut buffer = Vec::new();
        File::open("testdata/testfs1")
            .unwrap()
            .read_to_end(&mut buffer)
            .unwrap();
        Cursor::new(buffer)
    }
}
