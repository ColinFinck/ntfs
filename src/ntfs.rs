// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::boot_sector::BootSector;
use crate::error::{NtfsError, Result};
use crate::ntfs_file::{KnownNtfsFile, NtfsFile};
use crate::structured_values::{NtfsVolumeInformation, NtfsVolumeName};
use binread::io::{Read, Seek, SeekFrom};
use binread::BinReaderExt;

#[derive(Debug)]
pub struct Ntfs {
    /// The size of a single cluster, in bytes. This is usually 4096.
    cluster_size: u32,
    /// The size of a single sector, in bytes. This is usually 512.
    sector_size: u16,
    /// Size of the filesystem, in bytes.
    size: u64,
    /// Absolute position of the Master File Table (MFT), in bytes.
    mft_position: u64,
    /// Size of a single file record, in bytes.
    file_record_size: u32,
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

        let bpb = boot_sector.bpb();
        let cluster_size = bpb.cluster_size()?;
        let sector_size = bpb.sector_size();
        let size = bpb.total_sectors() * sector_size as u64;
        let mft_position = 0;
        let file_record_size = bpb.file_record_size()?;
        let serial_number = bpb.serial_number();

        let mut ntfs = Self {
            cluster_size,
            sector_size,
            size,
            mft_position,
            file_record_size,
            serial_number,
        };
        ntfs.mft_position = bpb.mft_lcn().position(&ntfs)?;

        Ok(ntfs)
    }

    /// Returns the size of a single cluster, in bytes.
    pub fn cluster_size(&self) -> u32 {
        self.cluster_size
    }

    pub fn file_record_size(&self) -> u32 {
        self.file_record_size
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
    pub fn ntfs_file<'n, T>(&'n self, fs: &mut T, n: u64) -> Result<NtfsFile<'n>>
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
        NtfsFile::new(&self, fs, position)
    }

    /// Returns the root [`Dir`] of this NTFS volume.
    pub fn root_dir(&self) -> ! {
        panic!("TODO")
    }

    /// Returns the size of a single sector in bytes.
    pub fn sector_size(&self) -> u16 {
        self.sector_size
    }

    /// Returns the 64-bit serial number of this NTFS volume.
    pub fn serial_number(&self) -> u64 {
        self.serial_number
    }

    /// Returns the partition size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns an [`NtfsVolumeInformation`] containing general information about
    /// the volume, like the NTFS version.
    pub fn volume_info<T>(&self, fs: &mut T) -> Result<NtfsVolumeInformation>
    where
        T: Read + Seek,
    {
        let volume_file = self.ntfs_file(fs, KnownNtfsFile::Volume as u64)?;
        let attribute = volume_file
            .attributes()
            .find(|attribute| {
                // TODO: Replace by attribute.ty().contains() once https://github.com/rust-lang/rust/issues/62358 has landed.
                attribute
                    .ty()
                    .map(|ty| ty == NtfsAttributeType::VolumeInformation)
                    .unwrap_or(false)
            })
            .ok_or(NtfsError::AttributeNotFound {
                position: volume_file.position(),
                ty: NtfsAttributeType::VolumeName,
            })?;
        attribute.resident_structured_value::<NtfsVolumeInformation>()
    }

    /// Returns an [`NtfsVolumeName`] to read the volume name (also called volume label)
    /// of this NTFS volume.
    /// Note that a volume may also have no label, which is why the return value is further
    /// encapsulated in an `Option`.
    pub fn volume_name<'d, T>(&self, fs: &mut T) -> Option<Result<NtfsVolumeName>>
    where
        T: Read + Seek,
    {
        let volume_file = iter_try!(self.ntfs_file(fs, KnownNtfsFile::Volume as u64));
        let attribute = volume_file.attributes().find(|attribute| {
            // TODO: Replace by attribute.ty().contains() once https://github.com/rust-lang/rust/issues/62358 has landed.
            attribute
                .ty()
                .map(|ty| ty == NtfsAttributeType::VolumeName)
                .unwrap_or(false)
        })?;
        let volume_name = iter_try!(attribute.resident_structured_value::<NtfsVolumeName>());

        Some(Ok(volume_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basics() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        assert_eq!(ntfs.cluster_size(), 512);
        assert_eq!(ntfs.sector_size(), 512);
        assert_eq!(ntfs.size(), 1049088);
    }

    #[test]
    fn test_volume_info() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let volume_info = ntfs.volume_info(&mut testfs1).unwrap();
        assert_eq!(volume_info.major_version(), 3);
        assert_eq!(volume_info.minor_version(), 1);
    }

    #[test]
    fn test_volume_name() {
        let mut testfs1 = crate::helpers::tests::testfs1();
        let ntfs = Ntfs::new(&mut testfs1).unwrap();
        let volume_name = ntfs.volume_name(&mut testfs1).unwrap().unwrap();
        assert_eq!(volume_name.name_length(), 14);
        assert_eq!(volume_name.name(), "mylabel");
    }
}
