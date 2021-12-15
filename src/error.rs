// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::attribute::NtfsAttributeType;
use crate::types::{Lcn, Vcn};
use core::ops::Range;
use displaydoc::Display;

/// Central result type of ntfs.
pub type Result<T, E = NtfsError> = core::result::Result<T, E>;

/// Central error type of ntfs.
#[derive(Debug, Display)]
#[non_exhaustive]
pub enum NtfsError {
    /// The NTFS file at byte position {position:#010x} has no attribute of type {ty:?}, but it was expected
    AttributeNotFound {
        position: u64,
        ty: NtfsAttributeType,
    },
    /// The NTFS Attribute at byte position {position:#010x} should have type {expected:?}, but it actually has type {actual:?}
    AttributeOfDifferentType {
        position: u64,
        expected: NtfsAttributeType,
        actual: NtfsAttributeType,
    },
    /// The given buffer should have at least {expected} bytes, but it only has {actual} bytes
    BufferTooSmall { expected: usize, actual: usize },
    /// The NTFS Attribute at byte position {position:#010x} indicates a name length up to offset {expected}, but the attribute only has a size of {actual} bytes
    InvalidAttributeNameLength {
        position: u64,
        expected: usize,
        actual: u32,
    },
    /// The NTFS Attribute at byte position {position:#010x} indicates that its name starts at offset {expected}, but the attribute only has a size of {actual} bytes
    InvalidAttributeNameOffset {
        position: u64,
        expected: u16,
        actual: u32,
    },
    /// The NTFS Data Run header at byte position {position:#010x} indicates a maximum byte count of {expected}, but {actual} is the limit
    InvalidByteCountInDataRunHeader {
        position: u64,
        expected: u8,
        actual: u8,
    },
    /// The cluster count {cluster_count} is too big
    InvalidClusterCount { cluster_count: u64 },
    /// The NTFS File Record at byte position {position:#010x} indicates an allocated size of {expected} bytes, but the record only has a size of {actual} bytes
    InvalidFileAllocatedSize {
        position: u64,
        expected: u32,
        actual: u32,
    },
    /// The requested NTFS File Record Number {file_record_number} is invalid
    InvalidFileRecordNumber { file_record_number: u64 },
    /// The NTFS File Record at byte position {position:#010x} should have signature {expected:?}, but it has signature {actual:?}
    InvalidFileSignature {
        position: u64,
        expected: &'static [u8],
        actual: [u8; 4],
    },
    /// The NTFS File Record at byte position {position:#010x} indicates a used size of {expected} bytes, but only {actual} bytes are allocated
    InvalidFileUsedSize {
        position: u64,
        expected: u32,
        actual: u32,
    },
    /// The NTFS Index Record at byte position {position:#010x} indicates an allocated size of {expected} bytes, but the record only has a size of {actual} bytes
    InvalidIndexAllocatedSize {
        position: u64,
        expected: u32,
        actual: u32,
    },
    /// The NTFS Index Entry at byte position {position:#010x} references a data field in the range {range:?}, but the entry only has a size of {size} bytes
    InvalidIndexEntryDataRange {
        position: u64,
        range: Range<usize>,
        size: u16,
    },
    /// The NTFS Index Entry at byte position {position:#010x} reports a size of {expected} bytes, but it only has {actual} bytes
    InvalidIndexEntrySize {
        position: u64,
        expected: u16,
        actual: u16,
    },
    /// The NTFS index root at byte position {position:#010x} indicates that its entries start at offset {expected}, but the index root only has a size of {actual} bytes
    InvalidIndexRootEntriesOffset {
        position: u64,
        expected: usize,
        actual: usize,
    },
    /// The NTFS index root at byte position {position:#010x} indicates a used size up to offset {expected}, but the index root only has a size of {actual} bytes
    InvalidIndexRootUsedSize {
        position: u64,
        expected: usize,
        actual: usize,
    },
    /// The NTFS Index Record at byte position {position:#010x} should have signature {expected:?}, but it has signature {actual:?}
    InvalidIndexSignature {
        position: u64,
        expected: &'static [u8],
        actual: [u8; 4],
    },
    /// The NTFS Index Record at byte position {position:#010x} indicates a used size of {expected} bytes, but only {actual} bytes are allocated
    InvalidIndexUsedSize {
        position: u64,
        expected: u32,
        actual: u32,
    },
    /// The resident NTFS Attribute at byte position {position:#010x} indicates a value length up to offset {expected}, but the attribute only has a size of {actual} bytes
    InvalidResidentAttributeValueLength {
        position: u64,
        expected: u32,
        actual: u32,
    },
    /// The resident NTFS Attribute at byte position {position:#010x} indicates that its value starts at offset {expected}, but the attribute only has a size of {actual} bytes
    InvalidResidentAttributeValueOffset {
        position: u64,
        expected: u16,
        actual: u32,
    },
    /// A record size field in the BIOS Parameter Block denotes {size_info}, which is invalid considering the cluster size of {cluster_size} bytes
    InvalidRecordSizeInfo { size_info: i8, cluster_size: u32 },
    /// The NTFS structured value at byte position {position:#010x} of type {ty:?} has {actual} bytes where {expected} bytes were expected
    InvalidStructuredValueSize {
        position: u64,
        ty: NtfsAttributeType,
        expected: u64,
        actual: u64,
    },
    /// The given time can't be represented as an NtfsTime
    InvalidTime,
    /// The 2-byte signature field at byte position {position:#010x} should contain {expected:?}, but it contains {actual:?}
    InvalidTwoByteSignature {
        position: u64,
        expected: &'static [u8],
        actual: [u8; 2],
    },
    /// The Upcase Table should have a size of {expected} bytes, but it has {actual} bytes
    InvalidUpcaseTableSize { expected: u64, actual: u64 },
    /// The VCN {vcn} read from the NTFS Data Run header at byte position {position:#010x} cannot be added to the LCN {previous_lcn} calculated from previous data runs
    InvalidVcnInDataRunHeader {
        position: u64,
        vcn: Vcn,
        previous_lcn: Lcn,
    },
    /// I/O error: {0:?}
    Io(binread::io::Error),
    /// The Logical Cluster Number (LCN) {lcn} is too big to be processed
    LcnTooBig { lcn: Lcn },
    /// The index root at byte position {position:#010x} is a large index, but no matching index allocation attribute was provided
    MissingIndexAllocation { position: u64 },
    /// The NTFS file at byte position {position:#010x} is not a directory.
    NotADirectory { position: u64 },
    /// The NTFS Attribute at byte position {position:#010x} should not belong to an Attribute List, but it does
    UnexpectedAttributeListAttribute { position: u64 },
    /// The NTFS Attribute at byte position {position:#010x} should be resident, but it is non-resident
    UnexpectedNonResidentAttribute { position: u64 },
    /// The NTFS Attribute at byte position {position:#010x} should be non-resident, but it is resident
    UnexpectedResidentAttribute { position: u64 },
    /// The type of the NTFS Attribute at byte position {position:#010x} is {actual:#010x}, which is not supported
    UnsupportedAttributeType { position: u64, actual: u32 },
    /// The cluster size is {actual} bytes, but the maximum supported one is {expected}
    UnsupportedClusterSize { expected: u32, actual: u32 },
    /// The namespace of the NTFS file name starting at byte position {position:#010x} is {actual}, which is not supported
    UnsupportedFileNamespace { position: u64, actual: u8 },
    /// The Update Sequence Array (USA) of the record at byte position {position:#010x} has entries for {array_count} sectors of {sector_size} bytes, but the record is only {record_size} bytes long
    UpdateSequenceArrayExceedsRecordSize {
        position: u64,
        array_count: u16,
        sector_size: u16,
        record_size: usize,
    },
    /// Sector corruption: The 2 bytes at byte position {position:#010x} should match the Update Sequence Number (USN) {expected:?}, but they are {actual:?}
    UpdateSequenceNumberMismatch {
        position: u64,
        expected: [u8; 2],
        actual: [u8; 2],
    },
    /// The index allocation at byte position {position:#010x} references a Virtual Cluster Number (VCN) {expected}, but a record with VCN {actual} is found at that offset
    VcnMismatchInIndexAllocation {
        position: u64,
        expected: Vcn,
        actual: Vcn,
    },
    /// The index allocation at byte position {position:#010x} references a Virtual Cluster Number (VCN) {vcn}, but this VCN exceeds the boundaries of the filesystem.
    VcnOutOfBoundsInIndexAllocation { position: u64, vcn: Vcn },
    /// The Virtual Cluster Number (VCN) {vcn} is too big to be processed
    VcnTooBig { vcn: Vcn },
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
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl std::error::Error for NtfsError {}
