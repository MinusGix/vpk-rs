pub mod access;
pub mod entry;
pub mod structs;
pub mod vpk;

pub use crate::vpk::VPK;

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error while trying to read data: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Expected null terminator at end of cstring")]
    ExpectedNullTerminator,
    #[error("Error while trying to read data: {0}")]
    BinReadError(#[from] binread::Error),
    #[error("Invalid signature, provided file is not a VPK file")]
    InvalidSignature,
    #[error("Unsupported VPK version({0}), only version 2 and low")]
    UnsupportedVersion(u32),
    #[error("Mismatched size for hashes section")]
    HashSizeMismatch,
    #[error("Malformed index encountered while parsing")]
    MalformedIndex,
}

pub fn from_path(path: impl AsRef<Path>) -> Result<VPK, Error> {
    let path = path.as_ref();
    let vpk = VPK::read(path)?;

    Ok(vpk)
}
