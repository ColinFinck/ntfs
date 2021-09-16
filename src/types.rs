// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use binread::BinRead;
use derive_more::{Binary, Display, From, LowerHex, Octal, UpperHex};

#[derive(
    Binary,
    BinRead,
    Clone,
    Copy,
    Debug,
    Display,
    Eq,
    From,
    LowerHex,
    Octal,
    Ord,
    PartialEq,
    PartialOrd,
    UpperHex,
)]
pub struct Lcn(u64);

impl Lcn {
    pub fn checked_add(&self, vcn: Vcn) -> Option<Lcn> {
        if vcn.0 >= 0 {
            self.0.checked_add(vcn.0 as u64).map(Into::into)
        } else {
            self.0
                .checked_sub(vcn.0.wrapping_neg() as u64)
                .map(Into::into)
        }
    }

    pub fn position(&self, ntfs: &Ntfs) -> Result<u64> {
        self.0
            .checked_mul(ntfs.cluster_size() as u64)
            .ok_or(NtfsError::LcnTooBig { lcn: *self })
    }
}

#[derive(
    Binary,
    BinRead,
    Clone,
    Copy,
    Debug,
    Display,
    Eq,
    From,
    LowerHex,
    Octal,
    Ord,
    PartialEq,
    PartialOrd,
    UpperHex,
)]
pub struct Vcn(i64);

impl Vcn {
    pub fn offset(&self, ntfs: &Ntfs) -> Result<i64> {
        self.0
            .checked_mul(ntfs.cluster_size() as i64)
            .ok_or(NtfsError::VcnTooBig { vcn: *self })
    }
}
