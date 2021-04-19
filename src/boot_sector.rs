// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use binread::BinRead;
use memoffset::offset_of;

/// The usual exponent of `BiosParameterBlock::file_record_size_info` is 10 (2^10 = 1024 bytes).
/// Exponents > 10 would come as a surprise, but our code should still be able to handle those.
/// Exponents > 32 (2^32 = 4 GiB) would make no sense, exceed a u32, and must be outright denied.
const MAXIMUM_SIZE_INFO_EXPONENT: u32 = 32;

// Sources:
// - https://en.wikipedia.org/wiki/NTFS#Partition_Boot_Sector_(VBR)
// - https://en.wikipedia.org/wiki/BIOS_parameter_block#NTFS
// - https://wiki.osdev.org/NTFS
// - The iBored tool from https://apps.tempel.org/iBored/
#[allow(unused)]
#[derive(BinRead)]
pub(crate) struct BiosParameterBlock {
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
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
    pub(crate) total_sectors: u64,
    /// Logical Cluster Number (LCN) to the beginning of the Master File Table (MFT).
    pub(crate) mft_lcn: u64,
    mft_mirror_lcn: u64,
    pub(crate) file_record_size_info: i8,
    zeros_4: [u8; 3],
    index_record_size_info: i8,
    zeros_5: [u8; 3],
    pub(crate) serial_number: u64,
    checksum: u32,
}

impl BiosParameterBlock {
    /// Source: https://en.wikipedia.org/wiki/NTFS#Partition_Boot_Sector_(VBR)
    pub(crate) fn record_size(size_info_value: i8, bytes_per_cluster: u32) -> Result<u32> {
        if size_info_value > 0 {
            // The size field denotes a cluster count.
            Ok(size_info_value as u32 * bytes_per_cluster as u32)
        } else {
            // The size field denotes a binary exponent after negation.
            let exponent = (-size_info_value) as u32;
            if exponent > MAXIMUM_SIZE_INFO_EXPONENT {
                return Err(NtfsError::InvalidRecordSizeExponent {
                    expected: MAXIMUM_SIZE_INFO_EXPONENT,
                    actual: exponent,
                });
            }

            Ok(1 << exponent)
        }
    }
}

#[allow(unused)]
#[derive(BinRead)]
pub(crate) struct BootSector {
    bootjmp: [u8; 3],
    oem_name: [u8; 8],
    pub(crate) bpb: BiosParameterBlock,
    boot_code: [u8; 426],
    signature: [u8; 2],
}

impl BootSector {
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
