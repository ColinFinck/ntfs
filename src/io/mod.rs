// Copyright 2026 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! [`std::io`]-like types that work in both `std` and `no_std` environments.
//!
//! If the `std` features is enabled, this module simply reexports the corresponding types from `std`.
//! Otherwise, it implements simplified `no_std`-compatible versions with just enough features for this crate.

#[cfg(feature = "std")]
pub use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

#[cfg(not(feature = "std"))]
mod no_std;
#[cfg(not(feature = "std"))]
pub use no_std::*;
