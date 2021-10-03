// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::boot_sector::BootSector;
use crate::error::{NtfsError, Result};
use crate::file::{KnownNtfsFileRecordNumber, NtfsFile};
use crate::structured_values::{NtfsVolumeInformation, NtfsVolumeName};
use crate::traits::NtfsReadSeek;
use crate::upcase_table::UpcaseTable;
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
    /// Table of Unicode uppercase characters (only required for case-insensitive comparisons).
    upcase_table: Option<UpcaseTable>,
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
        let upcase_table = None;

        let mut ntfs = Self {
            cluster_size,
            sector_size,
            size,
            mft_position,
            file_record_size,
            serial_number,
            upcase_table,
        };
        ntfs.mft_position = bpb.mft_lcn().position(&ntfs)?;

        Ok(ntfs)
    }

    /// Returns the size of a single cluster, in bytes.
    pub fn cluster_size(&self) -> u32 {
        self.cluster_size
    }

    /// Returns the [`NtfsFile`] for the given NTFS file record number.
    ///
    /// The first few NTFS files have fixed indexes and contain filesystem
    /// management information (see the [`KnownNtfsFileRecordNumber`] enum).
    pub fn file<'n, T>(&'n self, fs: &mut T, file_record_number: u64) -> Result<NtfsFile<'n>>
    where
        T: Read + Seek,
    {
        let offset = file_record_number
            .checked_mul(self.file_record_size as u64)
            .ok_or(NtfsError::InvalidFileRecordNumber { file_record_number })?;

        // The MFT may be split into multiple data runs, referenced by its $DATA attribute.
        // We therefore read it just like any other non-resident attribute value.
        // However, this code assumes that the MFT does not have an AttributeList!
        let mft = NtfsFile::new(&self, fs, self.mft_position, 0)?;
        let mft_data_attribute = mft
            .attributes_raw()
            .find(|attribute| {
                attribute
                    .ty()
                    .map(|ty| ty == NtfsAttributeType::Data)
                    .unwrap_or(false)
            })
            .ok_or(NtfsError::AttributeNotFound {
                position: self.mft_position,
                ty: NtfsAttributeType::Data,
            })?;
        let mut mft_data_value = mft_data_attribute.value()?;

        mft_data_value.seek(fs, SeekFrom::Start(offset))?;
        let position = mft_data_value
            .data_position()
            .ok_or(NtfsError::InvalidFileRecordNumber { file_record_number })?;

        NtfsFile::new(&self, fs, position, file_record_number)
    }

    pub fn file_record_size(&self) -> u32 {
        self.file_record_size
    }

    /// Returns the absolute byte position of the Master File Table (MFT).
    pub fn mft_position(&self) -> u64 {
        self.mft_position
    }

    /// Reads the $UpCase file from the filesystem and stores it in this [`Ntfs`] object.
    ///
    /// This function only needs to be called if case-insensitive comparisons are later performed
    /// (i.e. finding files).
    pub fn read_upcase_table<T>(&mut self, fs: &mut T) -> Result<()>
    where
        T: Read + Seek,
    {
        let upcase_table = UpcaseTable::read(self, fs)?;
        self.upcase_table = Some(upcase_table);
        Ok(())
    }

    /// Returns the root directory of this NTFS volume as an [`NtfsFile`].
    pub fn root_directory<'n, T>(&'n self, fs: &mut T) -> Result<NtfsFile<'n>>
    where
        T: Read + Seek,
    {
        self.file(fs, KnownNtfsFileRecordNumber::RootDirectory as u64)
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

    /// Returns the stored [`UpcaseTable`].
    ///
    /// # Panics
    ///
    /// Panics if [`read_upcase_table`][Ntfs::read_upcase_table] had not been called.
    pub(crate) fn upcase_table(&self) -> &UpcaseTable {
        self.upcase_table
            .as_ref()
            .expect("You need to call read_upcase_table first")
    }

    /// Returns an [`NtfsVolumeInformation`] containing general information about
    /// the volume, like the NTFS version.
    pub fn volume_info<T>(&self, fs: &mut T) -> Result<NtfsVolumeInformation>
    where
        T: Read + Seek,
    {
        let volume_file = self.file(fs, KnownNtfsFileRecordNumber::Volume as u64)?;
        let attribute = volume_file.attribute_by_ty(NtfsAttributeType::VolumeInformation)?;
        attribute.resident_structured_value::<NtfsVolumeInformation>()
    }

    /// Returns an [`NtfsVolumeName`] to read the volume name (also called volume label)
    /// of this NTFS volume.
    /// Note that a volume may also have no label, which is why the return value is further
    /// encapsulated in an `Option`.
    pub fn volume_name<T>(&self, fs: &mut T) -> Option<Result<NtfsVolumeName>>
    where
        T: Read + Seek,
    {
        let volume_file = iter_try!(self.file(fs, KnownNtfsFileRecordNumber::Volume as u64));
        let attribute = volume_file
            .attribute_by_ty(NtfsAttributeType::VolumeName)
            .ok()?;
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
        assert_eq!(ntfs.size(), 2096640);
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
