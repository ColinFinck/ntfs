// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use displaydoc::Display;

/// Central result type of ntfs.
pub type Result<T, E = NtfsError> = core::result::Result<T, E>;

/// Central error type of ntfs.
#[derive(Debug, Display)]
pub enum NtfsError {
    /// The given buffer should have at least {expected} bytes, but it only has {actual} bytes
    BufferTooSmall { expected: usize, actual: usize },
    /// The NTFS attribute at byte position {position:#010x} of type {ty:?} should have at least {expected} bytes, but it only has {actual} bytes
    InvalidAttributeSize {
        position: u64,
        ty: NtfsAttributeType,
        expected: u64,
        actual: u64,
    },
    /// The requested NTFS file {n} is invalid
    InvalidNtfsFile { n: u64 },
    /// The NTFS file at byte position {position:#010x} should have signature {expected:?}, but it has signature {actual:?}
    InvalidNtfsFileSignature {
        position: u64,
        expected: &'static [u8],
        actual: [u8; 4],
    },
    /// The given time can't be represented as an NtfsTime
    InvalidNtfsTime,
    /// A record size field in the BIOS Parameter Block denotes the exponent {actual}, but the maximum valid one is {expected}
    InvalidRecordSizeExponent { expected: u32, actual: u32 },
    /// The 2-byte signature field at byte position {position:#010x} should contain {expected:?}, but it contains {actual:?}
    InvalidTwoByteSignature {
        position: u64,
        expected: &'static [u8],
        actual: [u8; 2],
    },
    /// I/O error: {0:?}
    Io(binread::io::Error),
    /// The cluster size is {actual} bytes, but the maximum supported one is {expected}
    UnsupportedClusterSize { expected: u32, actual: u32 },
    /// The type of the NTFS attribute at byte position {position:#010x} is {actual:#010x}, which is not supported
    UnsupportedNtfsAttributeType { position: u64, actual: u32 },
    /// The namespace of the NTFS file name starting at byte position {position:#010x} is {actual}, which is not supported
    UnsupportedNtfsFileNamespace { position: u64, actual: u8 },
    /// The NTFS attribute at byte position {position:#010x} has type {ty:?}, which cannot be read as a structured value
    UnsupportedStructuredValue {
        position: u64,
        ty: NtfsAttributeType,
    },
}

impl From<binread::error::Error> for NtfsError {
    fn from(error: binread::error::Error) -> Self {
        if let binread::error::Error::Io(io_error) = error {
            Self::Io(io_error)
        } else {
            // We don't use any binread attributes that result in other errors.
            unreachable!("Got a binread error of unexpected type: {:?}", error);
        }
    }
}

impl From<binread::io::Error> for NtfsError {
    fn from(error: binread::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NtfsError {}
