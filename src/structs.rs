use crate::parse::{read_u128, read_u32};
use std::io::Read;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VPKHeader {
    pub signature: u32,
    pub version: u32,
    pub tree_length: u32,
}
impl VPKHeader {
    pub fn read_le(r: &mut impl Read) -> std::io::Result<Self> {
        let signature = read_u32(r)?;
        let version = read_u32(r)?;
        let tree_length = read_u32(r)?;

        Ok(Self {
            signature,
            version,
            tree_length,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VPKHeaderV2 {
    pub embed_chunk_length: u32,
    pub chunk_hashes_length: u32,
    pub self_hashes_length: u32,
    pub signature_length: u32,
}
impl VPKHeaderV2 {
    pub fn read_le(r: &mut impl Read) -> std::io::Result<Self> {
        let embed_chunk_length = read_u32(r)?;
        let chunk_hashes_length = read_u32(r)?;
        let self_hashes_length = read_u32(r)?;
        let signature_length = read_u32(r)?;

        Ok(Self {
            embed_chunk_length,
            chunk_hashes_length,
            self_hashes_length,
            signature_length,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VPKHeaderV2Checksum {
    pub tree_checksum: u128,
    pub chunk_hashes_checksum: u128,
    pub file_checksum: u128,
}
impl VPKHeaderV2Checksum {
    pub fn read_le(r: &mut impl Read) -> std::io::Result<Self> {
        let tree_checksum = read_u128(r)?;
        let chunk_hashes_checksum = read_u128(r)?;
        let file_checksum = read_u128(r)?;

        Ok(Self {
            tree_checksum,
            chunk_hashes_checksum,
            file_checksum,
        })
    }
}
