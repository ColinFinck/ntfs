// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::attribute_value::NtfsAttributeValueAttached;
use crate::error::{NtfsError, Result};
use crate::guid::{NtfsGuid, GUID_SIZE};
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
    pub(crate) fn new<T>(
        attribute_position: u64,
        mut value_attached: NtfsAttributeValueAttached<'_, '_, T>,
    ) -> Result<Self>
    where
        T: Read + Seek,
    {
        if value_attached.len() < GUID_SIZE {
            return Err(NtfsError::InvalidAttributeSize {
                position: attribute_position,
                ty: NtfsAttributeType::ObjectId,
                expected: GUID_SIZE,
                actual: value_attached.len(),
            });
        }

        let object_id = value_attached.read_le::<NtfsGuid>()?;

        let mut birth_volume_id = None;
        if value_attached.len() >= 2 * GUID_SIZE {
            birth_volume_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        let mut birth_object_id = None;
        if value_attached.len() >= 3 * GUID_SIZE {
            birth_object_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        let mut domain_id = None;
        if value_attached.len() >= 4 * GUID_SIZE {
            domain_id = Some(value_attached.read_le::<NtfsGuid>()?);
        }

        Ok(Self {
            object_id,
            birth_volume_id,
            birth_object_id,
            domain_id,
        })
    }

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
