// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::attribute::NtfsAttributeType;
use crate::error::{NtfsError, Result};
use crate::guid::{NtfsGuid, GUID_SIZE};
use crate::structured_values::{NtfsStructuredValue, NtfsStructuredValueFromSlice};
use binread::io::Cursor;
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

impl NtfsStructuredValue for NtfsObjectId {
    const TY: NtfsAttributeType = NtfsAttributeType::ObjectId;
}

impl<'s> NtfsStructuredValueFromSlice<'s> for NtfsObjectId {
    fn from_slice(slice: &'s [u8], position: u64) -> Result<Self> {
        if slice.len() < GUID_SIZE {
            return Err(NtfsError::InvalidStructuredValueSize {
                position,
                ty: NtfsAttributeType::ObjectId,
                expected: GUID_SIZE,
                actual: slice.len(),
            });
        }

        let mut cursor = Cursor::new(slice);
        let object_id = cursor.read_le::<NtfsGuid>()?;

        let mut birth_volume_id = None;
        if slice.len() >= 2 * GUID_SIZE {
            birth_volume_id = Some(cursor.read_le::<NtfsGuid>()?);
        }

        let mut birth_object_id = None;
        if slice.len() >= 3 * GUID_SIZE {
            birth_object_id = Some(cursor.read_le::<NtfsGuid>()?);
        }

        let mut domain_id = None;
        if slice.len() >= 4 * GUID_SIZE {
            domain_id = Some(cursor.read_le::<NtfsGuid>()?);
        }

        Ok(Self {
            object_id,
            birth_volume_id,
            birth_object_id,
            domain_id,
        })
    }
}
