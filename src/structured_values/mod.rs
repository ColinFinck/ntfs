// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

mod attribute_list;
mod file_name;
mod index_allocation;
mod index_root;
mod object_id;
mod security_descriptor;
mod standard_information;
mod volume_information;
mod volume_name;

pub use attribute_list::*;
pub use file_name::*;
pub use index_allocation::*;
pub use index_root::*;
pub use object_id::*;
pub use security_descriptor::*;
pub use standard_information::*;
pub use volume_information::*;
pub use volume_name::*;

use crate::attribute_value::NtfsAttributeValue;
use crate::error::Result;
use binread::io::{Read, Seek};
use bitflags::bitflags;

bitflags! {
    pub struct NtfsFileAttributeFlags: u32 {
        const READ_ONLY = 0x0001;
        const HIDDEN = 0x0002;
        const SYSTEM = 0x0004;
        const ARCHIVE = 0x0020;
        const DEVICE = 0x0040;
        const NORMAL = 0x0080;
        const TEMPORARY = 0x0100;
        const SPARSE_FILE = 0x0200;
        const REPARSE_POINT = 0x0400;
        const COMPRESSED = 0x0800;
        const OFFLINE = 0x1000;
        const NOT_CONTENT_INDEXED = 0x2000;
        const ENCRYPTED = 0x4000;
    }
}

#[derive(Clone, Debug)]
pub enum NtfsStructuredValue<'n> {
    StandardInformation(NtfsStandardInformation),
    FileName(NtfsFileName<'n>),
    ObjectId(NtfsObjectId),
    VolumeInformation(NtfsVolumeInformation),
    VolumeName(NtfsVolumeName<'n>),
    IndexRoot(NtfsIndexRoot<'n>),
    IndexAllocation(NtfsIndexAllocation<'n>),
}

pub trait NewNtfsStructuredValue<'n>: Sized {
    fn new<T>(fs: &mut T, value: NtfsAttributeValue<'n>, length: u64) -> Result<Self>
    where
        T: Read + Seek;
}
