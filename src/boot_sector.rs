// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::error::{NtfsError, Result};
use crate::types::Lcn;
use binread::BinRead;
use core::ops::Range;
use memoffset::offset_of;

// Sources:
// - https://en.wikipedia.org/wiki/NTFS#Partition_Boot_Sector_(VBR)
// - https://en.wikipedia.org/wiki/BIOS_parameter_block#NTFS
// - https://wiki.osdev.org/NTFS
// - The iBored tool from https://apps.tempel.org/iBored/
#[allow(unused)]
#[derive(BinRead)]
pub(crate) struct BiosParameterBlock {
    sector_size: u16,
    sectors_per_cluster: u8,
    zeros_1: [u8; 7],
    media: u8,
    zeros_2: [u8; 2],
    dummy_sectors_per_track: u16,
    dummy_heads: u16,
    hidden_sectors: u32,
    zeros_3: u32,
    physical_drive_number: u8,
    flags: u8,
    extended_boot_signature: u8,
    reserved: u8,
    total_sectors: u64,
    mft_lcn: Lcn,
    mft_mirror_lcn: Lcn,
    file_record_size_info: i8,
    zeros_4: [u8; 3],
    index_record_size_info: i8,
    zeros_5: [u8; 3],
    serial_number: u64,
    checksum: u32,
}

impl BiosParameterBlock {
    /// Returns the size of a single cluster, in bytes.
    pub(crate) fn cluster_size(&self) -> Result<u32> {
        /// The maximum cluster size supported by Windows is 2 MiB.
        /// Source: https://en.wikipedia.org/wiki/NTFS
        const MAXIMUM_CLUSTER_SIZE: u32 = 2097152;

        let cluster_size = self.sectors_per_cluster() * self.sector_size as u32;
        if cluster_size > MAXIMUM_CLUSTER_SIZE || !cluster_size.is_power_of_two() {
            return Err(NtfsError::UnsupportedClusterSize {
                expected: MAXIMUM_CLUSTER_SIZE,
                actual: cluster_size,
            });
        }

        Ok(cluster_size)
    }

    fn sectors_per_cluster(&self) -> u32 {
        if self.sectors_per_cluster > 128 {
            1 << (256 - self.sectors_per_cluster as u32)
        } else {
            self.sectors_per_cluster as u32
        }
    }

    pub(crate) fn file_record_size(&self) -> Result<u32> {
        self.record_size(self.file_record_size_info)
    }

    /// Returns the Logical Cluster Number (LCN) to the beginning of the Master File Table (MFT).
    pub(crate) fn mft_lcn(&self) -> Lcn {
        self.mft_lcn
    }

    /// Source: https://en.wikipedia.org/wiki/NTFS#Partition_Boot_Sector_(VBR)
    fn record_size(&self, size_info: i8) -> Result<u32> {
        /// The usual exponent of `BiosParameterBlock::file_record_size_info` is 10 (2^10 = 1024 bytes).
        /// Exponents < 10 have never been seen and are denied to guarantee that every record header
        /// fits into a record.
        const MINIMUM_SIZE_INFO_EXPONENT: u32 = 10;

        /// Exponents > 10 would come as a surprise, but our code should still be able to handle those.
        /// Exponents > 31 (2^31 = 2 GiB) would make no sense, exceed a u32, and must be outright denied.
        const MAXIMUM_SIZE_INFO_EXPONENT: u32 = 31;

        const SIZE_INFO_RANGE: Range<u32> = MINIMUM_SIZE_INFO_EXPONENT..MAXIMUM_SIZE_INFO_EXPONENT;

        let cluster_size = self.cluster_size()?;

        if size_info > 0 {
            // The size field denotes a cluster count.
            cluster_size
                .checked_mul(size_info as u32)
                .ok_or(NtfsError::InvalidRecordSizeInfo {
                    size_info,
                    cluster_size,
                })
        } else {
            // The size field denotes a binary exponent after negation.
            let exponent = (-size_info) as u32;

            if !SIZE_INFO_RANGE.contains(&exponent) {
                return Err(NtfsError::InvalidRecordSizeInfo {
                    size_info,
                    cluster_size,
                });
            }

            Ok(1 << exponent)
        }
    }

    pub(crate) fn sector_size(&self) -> Result<u16> {
        // NTFS-3G supports more sector sizes, but I haven't got Windows to accept an NTFS partition
        // with a sector size other than 512 bytes.
        // This restriction is arbitrary and can be lifted once you show me a Windows NTFS partition
        // with a different sector size.
        const SUPPORTED_SECTOR_SIZE: u16 = 512;

        if self.sector_size != SUPPORTED_SECTOR_SIZE {
            return Err(NtfsError::UnsupportedSectorSize {
                expected: SUPPORTED_SECTOR_SIZE,
                actual: self.sector_size,
            });
        }

        Ok(self.sector_size)
    }

    pub(crate) fn serial_number(&self) -> u64 {
        self.serial_number
    }

    pub(crate) fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}

#[allow(unused)]
#[derive(BinRead)]
pub(crate) struct BootSector {
    bootjmp: [u8; 3],
    oem_name: [u8; 8],
    bpb: BiosParameterBlock,
    boot_code: [u8; 426],
    signature: [u8; 2],
}

impl BootSector {
    pub(crate) fn bpb(&self) -> &BiosParameterBlock {
        &self.bpb
    }

    pub(crate) fn validate(&self) -> Result<()> {
        // Validate the infamous [0x55, 0xAA] signature at the end of the boot sector.
        let expected_signature = &[0x55, 0xAA];
        if &self.signature != expected_signature {
            return Err(NtfsError::InvalidTwoByteSignature {
                position: offset_of!(BootSector, signature) as u64,
                expected: expected_signature,
                actual: self.signature,
            });
        }

        Ok(())
    }
}
