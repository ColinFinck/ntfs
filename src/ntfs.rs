// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::boot_sector::{BiosParameterBlock, BootSector};
//use crate::dir::Dir;
use crate::error::{NtfsError, Result};
use crate::ntfs_file::NtfsFile;
use binread::io::{Read, Seek, SeekFrom};
use binread::BinReaderExt;

/// The maximum cluster size supported by Windows.
/// Source: https://support.microsoft.com/en-us/topic/default-cluster-size-for-ntfs-fat-and-exfat-9772e6f1-e31a-00d7-e18f-73169155af95
const MAXIMUM_CLUSTER_SIZE: u32 = 65536;

pub struct Ntfs {
    /// How many bytes a sector occupies. This is usually 512.
    bytes_per_sector: u16,
    /// How many sectors a cluster occupies. This is usually 8.
    sectors_per_cluster: u8,
    /// Size of the filesystem, in bytes.
    size: u64,
    /// Absolute position of the Master File Table (MFT), in bytes.
    mft_position: u64,
    /// Size of a single file record, in bytes.
    pub(crate) file_record_size: u32,
    /// Serial number of the NTFS volume.
    serial_number: u64,
}

impl Ntfs {
    pub fn new<T>(fs: &mut T) -> Result<Self>
    where
        T: Read + Seek,
    {
        // Read and validate the boot sector.
        fs.seek(SeekFrom::Start(0))?;
        let boot_sector = fs.read_le::<BootSector>()?;
        boot_sector.validate()?;

        let bytes_per_sector = boot_sector.bpb.bytes_per_sector;
        let sectors_per_cluster = boot_sector.bpb.sectors_per_cluster;
        let bytes_per_cluster = sectors_per_cluster as u32 * bytes_per_sector as u32;
        if bytes_per_cluster > MAXIMUM_CLUSTER_SIZE {
            return Err(NtfsError::UnsupportedClusterSize {
                expected: MAXIMUM_CLUSTER_SIZE,
                actual: bytes_per_cluster,
            });
        }

        let size = boot_sector.bpb.total_sectors * bytes_per_sector as u64;
        let mft_position = boot_sector.bpb.mft_lcn * bytes_per_cluster as u64;
        let file_record_size = BiosParameterBlock::record_size(
            boot_sector.bpb.file_record_size_info,
            bytes_per_cluster,
        )?;
        let serial_number = boot_sector.bpb.serial_number;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            size,
            mft_position,
            file_record_size,
            serial_number,
        })
    }

    /// Returns the size of a single cluster, in bytes.
    pub fn cluster_size(&self) -> u16 {
        self.bytes_per_sector * self.sectors_per_cluster as u16
    }

    /// Returns the [`NtfsFile`] for the `n`-th NTFS file record.
    ///
    /// The first few NTFS files have fixed indexes and contain filesystem
    /// management information (see the [`KnownNtfsFile`] enum).
    ///
    /// TODO:
    /// - Check if `n` can be u32 instead of u64.
    /// - Check if `n` should be in a newtype, with easier conversion from
    ///   KnownNtfsFile.
    pub fn ntfs_file<T>(&self, fs: &mut T, n: u64) -> Result<NtfsFile>
    where
        T: Read + Seek,
    {
        let offset = n
            .checked_mul(self.file_record_size as u64)
            .ok_or(NtfsError::InvalidNtfsFile { n })?;
        let position = self
            .mft_position
            .checked_add(offset)
            .ok_or(NtfsError::InvalidNtfsFile { n })?;
        NtfsFile::new(fs, position)
    }

    /// Returns the root [`Dir`] of this NTFS volume.
    pub fn root_dir(&self) -> ! {
        panic!("TODO")
    }

    /// Returns the size of a single sector in bytes.
    pub fn sector_size(&self) -> u16 {
        self.bytes_per_sector
    }

    /// Returns the 64-bit serial number of this NTFS volume.
    pub fn serial_number(&self) -> u64 {
        self.serial_number
    }

    /// Returns the partition size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntfs() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        assert_eq!(ntfs.cluster_size(), 512);
        assert_eq!(ntfs.sector_size(), 512);
        assert_eq!(ntfs.size(), 1049088);
    }
}
