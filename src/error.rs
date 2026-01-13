// Copyright 2021-2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ops::Range;

use thiserror::Error;

use crate::attribute::NtfsAttributeType;
use crate::io;
use crate::types::NtfsPosition;
use crate::types::{Lcn, Vcn};

/// Central result type of ntfs.
pub type Result<T, E = NtfsError> = core::result::Result<T, E>;

/// Central error type of ntfs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NtfsError {
    #[error("The NTFS file at byte position {position:#x} has no attribute of type {ty:?}, but it was expected")]
    AttributeNotFound {
        position: NtfsPosition,
        ty: NtfsAttributeType,
    },
    #[error("The NTFS Attribute at byte position {position:#x} should have type {expected:?}, but it actually has type {actual:?}")]
    AttributeOfDifferentType {
        position: NtfsPosition,
        expected: NtfsAttributeType,
        actual: NtfsAttributeType,
    },
    #[error(
        "The given buffer should have at least {expected} bytes, but it only has {actual} bytes"
    )]
    BufferTooSmall { expected: usize, actual: usize },
    #[error("The NTFS Attribute at byte position {position:#x} has a length of {expected} bytes, but only {actual} bytes are left in the record")]
    InvalidAttributeLength {
        position: NtfsPosition,
        expected: usize,
        actual: usize,
    },
    #[error("The NTFS Attribute at byte position {position:#x} indicates a name length up to offset {expected}, but the attribute only has a size of {actual} bytes")]
    InvalidAttributeNameLength {
        position: NtfsPosition,
        expected: usize,
        actual: u32,
    },
    #[error("The NTFS Attribute at byte position {position:#x} indicates that its name starts at offset {expected}, but the attribute only has a size of {actual} bytes")]
    InvalidAttributeNameOffset {
        position: NtfsPosition,
        expected: u16,
        actual: u32,
    },
    #[error("The NTFS Data Run header at byte position {position:#x} indicates a maximum byte count of {expected}, but {actual} is the limit")]
    InvalidByteCountInDataRunHeader {
        position: NtfsPosition,
        expected: u8,
        actual: u8,
    },
    #[error("The cluster count {cluster_count} read from the NTFS Data Run header at byte position {position:#x} is invalid")]
    InvalidClusterCountInDataRunHeader {
        position: NtfsPosition,
        cluster_count: u64,
    },
    #[error("The NTFS File Record at byte position {position:#x} indicates an allocated size of {expected} bytes, but the record only has a size of {actual} bytes")]
    InvalidFileAllocatedSize {
        position: NtfsPosition,
        expected: u32,
        actual: u32,
    },
    #[error("The requested NTFS File Record Number {file_record_number} is invalid")]
    InvalidFileRecordNumber { file_record_number: u64 },
    #[error("The NTFS File Record at byte position {position:#x} should have signature {expected:?}, but it has signature {actual:?}")]
    InvalidFileSignature {
        position: NtfsPosition,
        expected: &'static [u8],
        actual: [u8; 4],
    },
    #[error("The NTFS File Record at byte position {position:#x} indicates a used size of {expected} bytes, but only {actual} bytes are allocated")]
    InvalidFileUsedSize {
        position: NtfsPosition,
        expected: u32,
        actual: u32,
    },
    #[error("The NTFS Index Record at byte position {position:#x} indicates an allocated size of {expected} bytes, but the record only has a size of {actual} bytes")]
    InvalidIndexAllocatedSize {
        position: NtfsPosition,
        expected: u32,
        actual: u32,
    },
    #[error("The NTFS Index Entry at byte position {position:#x} references a data field in the range {range:?}, but the entry only has a size of {size} bytes")]
    InvalidIndexEntryDataRange {
        position: NtfsPosition,
        range: Range<usize>,
        size: u16,
    },
    #[error("The NTFS Index Entry at byte position {position:#x} reports a size of {expected} bytes, but it only has {actual} bytes")]
    InvalidIndexEntrySize {
        position: NtfsPosition,
        expected: u16,
        actual: u16,
    },
    #[error("The NTFS index root at byte position {position:#x} indicates that its entries start at offset {expected}, but the index root only has a size of {actual} bytes")]
    InvalidIndexRootEntriesOffset {
        position: NtfsPosition,
        expected: usize,
        actual: usize,
    },
    #[error("The NTFS index root at byte position {position:#x} indicates a used size up to offset {expected}, but the index root only has a size of {actual} bytes")]
    InvalidIndexRootUsedSize {
        position: NtfsPosition,
        expected: usize,
        actual: usize,
    },
    #[error("The NTFS Index Record at byte position {position:#x} should have signature {expected:?}, but it has signature {actual:?}")]
    InvalidIndexSignature {
        position: NtfsPosition,
        expected: &'static [u8],
        actual: [u8; 4],
    },
    #[error("The NTFS Index Record at byte position {position:#x} indicates a used size of {expected} bytes, but only {actual} bytes are allocated")]
    InvalidIndexUsedSize {
        position: NtfsPosition,
        expected: u32,
        actual: u32,
    },
    #[error("The MFT LCN in the BIOS Parameter Block of the NTFS filesystem is invalid.")]
    InvalidMftLcn,
    #[error("The NTFS Non Resident Value Data at byte position {position:#x} references a data field in the range {range:?}, but the entry only has a size of {size} bytes")]
    InvalidNonResidentValueDataRange {
        position: NtfsPosition,
        range: Range<usize>,
        size: usize,
    },
    #[error("The resident NTFS Attribute at byte position {position:#x} indicates a value length of {length} starting at offset {offset}, but the attribute only has a size of {actual} bytes")]
    InvalidResidentAttributeValueLength {
        position: NtfsPosition,
        length: u32,
        offset: u16,
        actual: u32,
    },
    #[error("The resident NTFS Attribute at byte position {position:#x} indicates that its value starts at offset {expected}, but the attribute only has a size of {actual} bytes")]
    InvalidResidentAttributeValueOffset {
        position: NtfsPosition,
        expected: u16,
        actual: u32,
    },
    #[error("A record size field in the BIOS Parameter Block denotes {size_info}, which is invalid considering the cluster size of {cluster_size} bytes")]
    InvalidRecordSizeInfo { size_info: i8, cluster_size: u32 },
    #[error("The sectors per cluster field in the BIOS Parameter Block denotes {sectors_per_cluster:#04x}, which is invalid")]
    InvalidSectorsPerCluster { sectors_per_cluster: u8 },
    #[error("The NTFS structured value at byte position {position:#x} of type {ty:?} has {actual} bytes where {expected} bytes were expected")]
    InvalidStructuredValueSize {
        position: NtfsPosition,
        ty: NtfsAttributeType,
        expected: u64,
        actual: u64,
    },
    #[error("The given time can't be represented as an NtfsTime")]
    InvalidTime,
    #[error("The 2-byte signature field at byte position {position:#x} should contain {expected:?}, but it contains {actual:?}")]
    InvalidTwoByteSignature {
        position: NtfsPosition,
        expected: &'static [u8],
        actual: [u8; 2],
    },
    #[error("The Upcase Table should have a size of {expected} bytes, but it has {actual} bytes")]
    InvalidUpcaseTableSize { expected: u64, actual: u64 },
    #[error("The NTFS Update Sequence Count of the record at byte position {position:#x} has the invalid value {update_sequence_count}")]
    InvalidUpdateSequenceCount {
        position: NtfsPosition,
        update_sequence_count: u16,
    },
    #[error("The NTFS Update Sequence Number of the record at byte position {position:#x} references a data field in the range {range:?}, but the entry only has a size of {size} bytes")]
    InvalidUpdateSequenceNumberRange {
        position: NtfsPosition,
        range: Range<usize>,
        size: usize,
    },
    #[error("The VCN {vcn} read from the NTFS Data Run header at byte position {position:#x} cannot be added to the LCN {previous_lcn} calculated from previous data runs")]
    InvalidVcnInDataRunHeader {
        position: NtfsPosition,
        vcn: Vcn,
        previous_lcn: Lcn,
    },
    #[error("I/O error: {0:?}")]
    Io(io::Error),
    #[error(
        "The Logical Cluster Number (LCN) {lcn} is too big to be multiplied by the cluster size"
    )]
    LcnTooBig { lcn: Lcn },
    #[error("The index root at byte position {position:#x} is a large index, but no matching index allocation attribute was provided")]
    MissingIndexAllocation { position: NtfsPosition },
    #[error("The NTFS file at byte position {position:#x} is not a directory")]
    NotADirectory { position: NtfsPosition },
    #[error(
        "The total sector count {total_sectors} is too big to be multiplied by the sector size"
    )]
    TotalSectorsTooBig { total_sectors: u64 },
    #[error("The NTFS Attribute at byte position {position:#x} should not belong to an Attribute List, but it does")]
    UnexpectedAttributeListAttribute { position: NtfsPosition },
    #[error("The NTFS Attribute at byte position {position:#x} should be resident, but it is non-resident")]
    UnexpectedNonResidentAttribute { position: NtfsPosition },
    #[error("The NTFS Attribute at byte position {position:#x} should be non-resident, but it is resident")]
    UnexpectedResidentAttribute { position: NtfsPosition },
    #[error("The type of the NTFS Attribute at byte position {position:#x} is {actual:#010x}, which is not supported")]
    UnsupportedAttributeType { position: NtfsPosition, actual: u32 },
    #[error("The cluster size is {actual} bytes, but it needs to be between {min} and {max}")]
    UnsupportedClusterSize { min: u32, max: u32, actual: u32 },
    #[error("The namespace of the NTFS file name starting at byte position {position:#x} is {actual}, which is not supported")]
    UnsupportedFileNamespace { position: NtfsPosition, actual: u8 },
    #[error("The sector size is {actual} bytes, but it needs to be between {min} and {max}")]
    UnsupportedSectorSize { min: u16, max: u16, actual: u16 },
    #[error("The Update Sequence Array (USA) of the record at byte position {position:#x} has entries for {array_count} blocks of 512 bytes, but the record is only {record_size} bytes long")]
    UpdateSequenceArrayExceedsRecordSize {
        position: NtfsPosition,
        array_count: u16,
        record_size: usize,
    },
    #[error("Sector corruption: The 2 bytes at byte position {position:#x} should match the Update Sequence Number (USN) {expected:?}, but they are {actual:?}")]
    UpdateSequenceNumberMismatch {
        position: NtfsPosition,
        expected: [u8; 2],
        actual: [u8; 2],
    },
    #[error("The index allocation at byte position {position:#x} references a Virtual Cluster Number (VCN) {expected}, but a record with VCN {actual} is found at that offset")]
    VcnMismatchInIndexAllocation {
        position: NtfsPosition,
        expected: Vcn,
        actual: Vcn,
    },
    #[error("The index allocation at byte position {position:#x} references a Virtual Cluster Number (VCN) {vcn}, but this VCN exceeds the boundaries of the filesystem")]
    VcnOutOfBoundsInIndexAllocation { position: NtfsPosition, vcn: Vcn },
    #[error(
        "The Virtual Cluster Number (VCN) {vcn} is too big to be multiplied by the cluster size"
    )]
    VcnTooBig { vcn: Vcn },
}

impl From<io::Error> for NtfsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

// To stay compatible with standardized interfaces (e.g. io::Read, io::Seek),
// we sometimes need to convert from NtfsError to io::Error.
impl From<NtfsError> for io::Error {
    fn from(error: NtfsError) -> Self {
        if let NtfsError::Io(io_error) = error {
            io_error
        } else {
            io::Error::new(io::ErrorKind::Other, error)
        }
    }
}
