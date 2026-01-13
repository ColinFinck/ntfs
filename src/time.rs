// Copyright 2021-2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use zerocopy::{FromBytes, Immutable, KnownLayout, LittleEndian, Unaligned, U64};

/// Difference in 100-nanosecond intervals between the Windows/NTFS epoch (1601-01-01) and the Unix epoch (1970-01-01).
#[cfg(any(feature = "chrono", feature = "time", feature = "std"))]
const EPOCH_DIFFERENCE_IN_INTERVALS: i64 = 116_444_736_000_000_000;

/// Number of 100-nanosecond intervals in a second.
#[cfg(any(feature = "chrono", feature = "std"))]
const INTERVALS_PER_SECOND: u64 = 10_000_000;

/// Difference in seconds between the Windows/NTFS epoch (1601-01-01) and the Unix epoch (1970-01-01).
#[cfg(feature = "chrono")]
const EPOCH_DIFFERENCE_IN_SECONDS: i64 =
    EPOCH_DIFFERENCE_IN_INTERVALS / (INTERVALS_PER_SECOND as i64);

/// An NTFS timestamp, used for expressing file times.
///
/// NTFS (and the Windows NT line of operating systems) represent time as an unsigned 64-bit integer
/// counting the number of 100-nanosecond intervals since January 1, 1601.
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, KnownLayout, Ord, PartialEq, PartialOrd, Unaligned,
)]
#[repr(transparent)]
pub struct NtfsTime(U64<LittleEndian>);

impl NtfsTime {
    /// Returns the stored NT timestamp (number of 100-nanosecond intervals since January 1, 1601).
    pub fn nt_timestamp(&self) -> u64 {
        self.0.get()
    }
}

impl From<u64> for NtfsTime {
    fn from(value: u64) -> Self {
        Self(U64::new(value))
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl<Tz: chrono::TimeZone> TryFrom<chrono::DateTime<Tz>> for NtfsTime {
    type Error = crate::error::NtfsError;

    fn try_from(dt: chrono::DateTime<Tz>) -> Result<Self, Self::Error> {
        let seconds_since_unix_epoch = dt.timestamp();
        let seconds_since_windows_epoch = seconds_since_unix_epoch
            .checked_add(EPOCH_DIFFERENCE_IN_SECONDS)
            .ok_or(crate::error::NtfsError::InvalidTime)?;
        let seconds_since_windows_epoch = u64::try_from(seconds_since_windows_epoch)
            .map_err(|_| crate::error::NtfsError::InvalidTime)?;
        let intervals_since_windows_epoch = seconds_since_windows_epoch
            .checked_mul(INTERVALS_PER_SECOND)
            .ok_or(crate::error::NtfsError::InvalidTime)?;
        let intervals_since_windows_epoch = intervals_since_windows_epoch
            .checked_add(u64::from(dt.timestamp_subsec_nanos()) / 100)
            .ok_or(crate::error::NtfsError::InvalidTime)?;

        Ok(Self::from(intervals_since_windows_epoch))
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl From<NtfsTime> for chrono::DateTime<chrono::Utc> {
    fn from(nt: NtfsTime) -> Self {
        let seconds_since_windows_epoch = (nt.nt_timestamp() / INTERVALS_PER_SECOND) as i64;
        let seconds_since_unix_epoch = seconds_since_windows_epoch - EPOCH_DIFFERENCE_IN_SECONDS;

        let subintervals = (nt.nt_timestamp() % INTERVALS_PER_SECOND) as u32;
        let subsec_nanos = subintervals * 100;

        Self::from_timestamp(seconds_since_unix_epoch, subsec_nanos).unwrap()
    }
}

#[cfg(feature = "time")]
#[cfg_attr(docsrs, doc(cfg(feature = "time")))]
impl TryFrom<time::OffsetDateTime> for NtfsTime {
    type Error = crate::error::NtfsError;

    fn try_from(dt: time::OffsetDateTime) -> Result<Self, Self::Error> {
        let nanos_since_unix_epoch = dt.unix_timestamp_nanos();
        let intervals_since_unix_epoch = nanos_since_unix_epoch / 100;
        let intervals_since_windows_epoch =
            intervals_since_unix_epoch + i128::from(EPOCH_DIFFERENCE_IN_INTERVALS);
        let nt_timestamp = u64::try_from(intervals_since_windows_epoch)
            .map_err(|_| crate::error::NtfsError::InvalidTime)?;

        Ok(Self::from(nt_timestamp))
    }
}

#[cfg(feature = "time")]
#[cfg_attr(docsrs, doc(cfg(feature = "time")))]
impl From<NtfsTime> for time::OffsetDateTime {
    fn from(nt: NtfsTime) -> time::OffsetDateTime {
        let intervals_since_windows_epoch = i128::from(nt.nt_timestamp());
        let intervals_since_unix_epoch =
            intervals_since_windows_epoch - i128::from(EPOCH_DIFFERENCE_IN_INTERVALS);
        let nanos_since_unix_epoch = intervals_since_unix_epoch * 100;

        time::OffsetDateTime::from_unix_timestamp_nanos(nanos_since_unix_epoch).unwrap()
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl TryFrom<std::time::SystemTime> for NtfsTime {
    type Error = crate::error::NtfsError;

    fn try_from(st: std::time::SystemTime) -> Result<Self, Self::Error> {
        let duration_since_unix_epoch = st
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| crate::error::NtfsError::InvalidTime)?;
        let intervals_since_unix_epoch = duration_since_unix_epoch
            .as_secs()
            .checked_mul(INTERVALS_PER_SECOND)
            .ok_or(crate::error::NtfsError::InvalidTime)?;
        let intervals_since_unix_epoch = intervals_since_unix_epoch
            .checked_add(duration_since_unix_epoch.subsec_nanos() as u64 / 100)
            .ok_or(crate::error::NtfsError::InvalidTime)?;
        let intervals_since_windows_epoch = intervals_since_unix_epoch
            .checked_add(EPOCH_DIFFERENCE_IN_INTERVALS as u64)
            .ok_or(crate::error::NtfsError::InvalidTime)?;

        Ok(Self::from(intervals_since_windows_epoch))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const NT_TIMESTAMP_2021_01_01: u64 = 132539328000000000u64;

    #[cfg(feature = "chrono")]
    #[test]
    fn test_chrono_datetime() {
        let dt = chrono::DateTime::parse_from_rfc3339("2013-01-05T18:15:00Z").unwrap();
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(nt.nt_timestamp(), 130018833000000000u64);

        let dt2 = chrono::DateTime::from(nt);
        assert_eq!(dt, dt2);

        // Minimum date/time supported by NT.
        let dt = chrono::DateTime::parse_from_rfc3339("1601-01-01T00:00:00Z").unwrap();
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(nt.nt_timestamp(), 0u64);

        let dt = chrono::DateTime::parse_from_rfc3339("1600-12-31T23:59:59Z").unwrap();
        assert!(NtfsTime::try_from(dt).is_err());

        let dt =
            chrono::DateTime::parse_from_str("+60056-05-28 00:00:00+00", "%Y-%m-%d %T%#z").unwrap();
        assert!(NtfsTime::try_from(dt).is_ok());

        let dt =
            chrono::DateTime::parse_from_str("+60056-05-29 00:00:00+00", "%Y-%m-%d %T%#z").unwrap();
        assert!(NtfsTime::try_from(dt).is_err());
    }

    #[cfg(feature = "time")]
    #[test]
    fn test_time_offsetdatetime() {
        use time::macros::datetime;

        let dt = datetime!(2013-01-05 18:15 UTC);
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(nt.nt_timestamp(), 130018833000000000u64);

        let dt2 = time::OffsetDateTime::from(nt);
        assert_eq!(dt, dt2);

        // Minimum date/time supported by NT.
        let dt = datetime!(1601-01-01 0:00 UTC);
        let nt = NtfsTime::try_from(dt).unwrap();
        assert_eq!(nt.nt_timestamp(), 0u64);

        let dt = datetime!(1600-12-31 23:59:59 UTC);
        assert!(NtfsTime::try_from(dt).is_err());

        let dt = datetime!(+60056-05-28 0:00 UTC);
        assert!(NtfsTime::try_from(dt).is_ok());

        let dt = datetime!(+60056-05-29 0:00 UTC);
        assert!(NtfsTime::try_from(dt).is_err());
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_systemtime() {
        let st = std::time::SystemTime::now();
        let nt = NtfsTime::try_from(st).unwrap();
        assert!(nt.nt_timestamp() > NT_TIMESTAMP_2021_01_01);
    }
}
