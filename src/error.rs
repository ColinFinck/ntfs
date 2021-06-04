// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use displaydoc::Display;

/// Central result type of ntfs.
pub type Result<T, E = NtfsError> = core::result::Result<T, E>;

/// Central error type of ntfs.
#[derive(Debug, Display)]
pub enum NtfsError {
    /// The NTFS file at byte position {position:#010x} has no attribute of type {ty:?}, but it was expected
    AttributeNotFound {
        position: u64,
        ty: NtfsAttributeType,
    },
    /// The given buffer should have at least {expected} bytes, but it only has {actual} bytes
    BufferTooSmall { expected: usize, actual: usize },
    /// The header of an NTFS data run should indicate a maximum byte count of {expected},
    /// but the header at byte position {position:#010x} indicates a byte count of {actual}
    InvalidByteCountInDataRunHeader {
        position: u64,
        expected: u8,
        actual: u8,
    },
    /// An invalid LCN position {lcn_position} was calculated from the NTFS data run header at
    /// byte position {position:#010x} (and previous data runs)
    InvalidLcnPositionInDataRunHeader { position: u64, lcn_position: i64 },
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
    /// A record size field in the BIOS Parameter Block denotes {size_info}, which is invalid considering the cluster size of {cluster_size} bytes
    InvalidRecordSizeInfo { size_info: i8, cluster_size: u32 },
    /// The NTFS structured value at byte position {position:#010x} of type {ty:?} has {actual} bytes where {expected} bytes were expected
    InvalidStructuredValueSize {
        position: u64,
        ty: NtfsAttributeType,
        expected: u64,
        actual: u64,
    },
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

// To stay compatible with standardized interfaces (e.g. io::Read, io::Seek),
// we sometimes need to convert from NtfsError to io::Error.
impl From<NtfsError> for binread::io::Error {
    fn from(error: NtfsError) -> Self {
        if let NtfsError::Io(io_error) = error {
            io_error
        } else {
            binread::io::Error::new(binread::io::ErrorKind::Other, error)
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NtfsError {}
