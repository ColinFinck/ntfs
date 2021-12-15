// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use binread::BinRead;
use derive_more::From;

#[cfg(any(feature = "chrono", feature = "std"))]
use core::convert::TryFrom;

#[cfg(feature = "chrono")]
use {
    crate::error::NtfsError,
    chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc},
};

#[cfg(feature = "std")]
use std::time::{SystemTime, SystemTimeError};

/// How many days we have between 0001-01-01 and 1601-01-01.
#[cfg(feature = "chrono")]
const DAYS_FROM_0001_TO_1601: i32 = 584389;

/// Difference in 100-nanosecond intervals between the Windows/NTFS epoch (1601-01-01) and the Unix epoch (1970-01-01).
#[cfg(feature = "std")]
const EPOCH_DIFFERENCE_IN_INTERVALS: i64 = 116_444_736_000_000_000;

/// Number of 100-nanosecond intervals in a second.
#[cfg(any(feature = "chrono", feature = "std"))]
const INTERVALS_PER_SECOND: u64 = 10_000_000;

/// Number of 100-nanosecond intervals in a day.
#[cfg(feature = "chrono")]
const INTERVALS_PER_DAY: u64 = 24 * 60 * 60 * INTERVALS_PER_SECOND;

/// An NTFS timestamp, used for expressing file times.
///
/// NTFS (and the Windows NT line of operating systems) represent time as an unsigned 64-bit integer
/// counting the number of 100-nanosecond intervals since January 1, 1601.
#[derive(BinRead, Clone, Copy, Debug, Eq, From, Ord, PartialEq, PartialOrd)]
pub struct NtfsTime(u64);

impl NtfsTime {
    /// Returns the stored NT timestamp (number of 100-nanosecond intervals since January 1, 1601).
    pub fn nt_timestamp(&self) -> u64 {
        self.0
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl TryFrom<DateTime<Utc>> for NtfsTime {
    type Error = NtfsError;

    fn try_from(dt: DateTime<Utc>) -> Result<Self, Self::Error> {
        // First do the time calculations, which safely fit into a u64.
        let mut intervals = dt.hour() as u64;

        intervals *= 60;
        intervals += dt.minute() as u64;

        intervals *= 60;
        intervals += dt.second() as u64;

        intervals *= INTERVALS_PER_SECOND;
        intervals += dt.nanosecond() as u64 / 100;

        // Now do checked arithmetics for the day calculations, which may
        // exceed the lower bounds (years before 1601) or upper bounds
        // (dates after approximately 28 May 60056).
        let num_days_from_ce = dt.num_days_from_ce();
        let num_days_from_1601 = num_days_from_ce
            .checked_sub(DAYS_FROM_0001_TO_1601)
            .ok_or(NtfsError::InvalidTime)?;
        let intervals_days = INTERVALS_PER_DAY
            .checked_mul(num_days_from_1601 as u64)
            .ok_or(NtfsError::InvalidTime)?;
        intervals = intervals
            .checked_add(intervals_days)
            .ok_or(NtfsError::InvalidTime)?;

        Ok(Self(intervals))
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl From<NtfsTime> for DateTime<Utc> {
    fn from(nt: NtfsTime) -> DateTime<Utc> {
        let mut remainder = nt.nt_timestamp();

        let nano = (remainder % INTERVALS_PER_SECOND) as u32 * 100;
        remainder /= INTERVALS_PER_SECOND;

        let sec = (remainder % 60) as u32;
        remainder /= 60;

        let min = (remainder % 60) as u32;
        remainder /= 60;

        let hour = (remainder % 24) as u32;
        remainder /= 24;

        let num_days_from_1601 = remainder as i32;
        let num_days_from_ce = num_days_from_1601 + DAYS_FROM_0001_TO_1601;

        let ndt =
            NaiveDate::from_num_days_from_ce(num_days_from_ce).and_hms_nano(hour, min, sec, nano);
        DateTime::<Utc>::from_utc(ndt, Utc)
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl TryFrom<SystemTime> for NtfsTime {
    type Error = SystemTimeError;

    fn try_from(st: SystemTime) -> Result<Self, Self::Error> {
        let duration_since_unix_epoch = st.duration_since(SystemTime::UNIX_EPOCH)?;
        let intervals_since_unix_epoch = duration_since_unix_epoch.as_secs() as u64
            * INTERVALS_PER_SECOND
            + duration_since_unix_epoch.subsec_nanos() as u64 / 100;
        let intervals_since_windows_epoch =
            intervals_since_unix_epoch + EPOCH_DIFFERENCE_IN_INTERVALS as u64;
        Ok(Self(intervals_since_windows_epoch))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[cfg(feature = "chrono")]
    use chrono::TimeZone;

    pub(crate) const NT_TIMESTAMP_2021_01_01: u64 = 132539328000000000u64;

    #[cfg(feature = "chrono")]
    #[test]
    fn test_chrono() {
        let dt = Utc.ymd(2013, 1, 5).and_hms(18, 15, 00);
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(*nt, 130018833000000000u64);

        let dt2 = DateTime::<Utc>::from(nt);
        assert_eq!(dt, dt2);

        let dt = Utc.ymd(1601, 1, 1).and_hms(0, 0, 0);
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(*nt, 0u64);

        let dt = Utc.ymd(1600, 12, 31).and_hms(23, 59, 59);
        assert!(NtfsTime::try_from(dt).is_err());

        let dt = Utc.ymd(60056, 5, 28).and_hms(0, 0, 0);
        assert!(NtfsTime::try_from(dt).is_ok());

        let dt = Utc.ymd(60056, 5, 29).and_hms(0, 0, 0);
        assert!(NtfsTime::try_from(dt).is_err());
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_systemtime() {
        let st = SystemTime::now();
        let nt = NtfsTime::try_from(st).unwrap();
        assert!(nt.nt_timestamp() > NT_TIMESTAMP_2021_01_01);
    }
}
