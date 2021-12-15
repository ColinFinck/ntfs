// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later
//
//! Supplementary helper types.

use crate::error::{NtfsError, Result};
use crate::ntfs::Ntfs;
use binread::BinRead;
use derive_more::{Binary, Display, From, LowerHex, Octal, UpperHex};

/// A Logical Cluster Number (LCN).
///
/// NTFS divides a filesystem into clusters of a given size (power of two), see [`Ntfs::cluster_size`].
/// The LCN is an absolute cluster index into the filesystem.
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
    /// Performs a checked addition of the given Virtual Cluster Number (VCN), returning a new LCN.
    pub fn checked_add(&self, vcn: Vcn) -> Option<Lcn> {
        if vcn.0 >= 0 {
            self.0.checked_add(vcn.0 as u64).map(Into::into)
        } else {
            self.0
                .checked_sub(vcn.0.wrapping_neg() as u64)
                .map(Into::into)
        }
    }

    /// Returns the absolute byte position of this LCN within the filesystem.
    pub fn position(&self, ntfs: &Ntfs) -> Result<u64> {
        self.0
            .checked_mul(ntfs.cluster_size() as u64)
            .ok_or(NtfsError::LcnTooBig { lcn: *self })
    }
}

/// A Virtual Cluster Number (VCN).
///
/// NTFS divides a filesystem into clusters of a given size (power of two), see [`Ntfs::cluster_size`].
/// The VCN is a cluster index into the filesystem that is relative to a Logical Cluster Number (LCN)
/// or relative to the start of an attribute value.
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
    /// Converts this VCN into a byte offset (with respect to the cluster size of the provided [`Ntfs`] filesystem).
    pub fn offset(&self, ntfs: &Ntfs) -> Result<i64> {
        self.0
            .checked_mul(ntfs.cluster_size() as i64)
            .ok_or(NtfsError::VcnTooBig { vcn: *self })
    }
}
