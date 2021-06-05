// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValue;
use crate::error::{NtfsError, Result};
use crate::guid::{NtfsGuid, GUID_SIZE};
use crate::ntfs::Ntfs;
use crate::structured_values::NewNtfsStructuredValue;
use binread::io::{Read, Seek};
use binread::BinReaderExt;

#[derive(Clone, Debug)]
pub struct NtfsObjectId {
    object_id: NtfsGuid,
    birth_volume_id: Option<NtfsGuid>,
    birth_object_id: Option<NtfsGuid>,
    domain_id: Option<NtfsGuid>,
}

impl NtfsObjectId {
    pub fn birth_object_id(&self) -> Option<&NtfsGuid> {
        self.birth_object_id.as_ref()
    }

    pub fn birth_volume_id(&self) -> Option<&NtfsGuid> {
        self.birth_volume_id.as_ref()
    }

    pub fn domain_id(&self) -> Option<&NtfsGuid> {
        self.domain_id.as_ref()
    }

    pub fn object_id(&self) -> &NtfsGuid {
        &self.object_id
    }
}

impl<'n> NewNtfsStructuredValue<'n> for NtfsObjectId {
    fn new<T>(
        _ntfs: &'n Ntfs,
        fs: &mut T,
        value: NtfsAttributeValue<'n>,
        length: u64,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if length < GUID_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position: value.data_position().unwrap(),
                ty: NtfsAttributeType::ObjectId,
                expected: GUID_SIZE,
                actual: length,
            });
        }

        let mut value_attached = value.attach(fs);
        let object_id = value_attached.read_le::<NtfsGuid>()?;

        let mut birth_volume_id = None;
        if length >= 2 * GUID_SIZE {
            birth_volume_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        let mut birth_object_id = None;
        if length >= 3 * GUID_SIZE {
            birth_object_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        let mut domain_id = None;
        if length >= 4 * GUID_SIZE {
            domain_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        Ok(Self {
            object_id,
            birth_volume_id,
            birth_object_id,
            domain_id,
        })
    }
}
