use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::parse::{read_u16, read_u32};
use crate::VPK;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VPKEntry {
    pub dir_entry: VPKDirectoryEntry,
    pub preload_start: usize,
}

impl VPKEntry {
    pub fn preload_interval(&self) -> Range<usize> {
        let start = self.preload_start;
        let end = start + self.dir_entry.preload_length as usize;
        start..end
    }

    pub fn get<'v>(&self, parent: &'v VPK) -> Result<Cow<'v, [u8]>, Error> {
        if self.dir_entry.archive_index == 0x7fff {
            let preload_data = &parent.data[self.preload_interval()];
            return Ok(Cow::Borrowed(preload_data));
        }

        let archive_path = &parent.archive_paths[usize::from(self.dir_entry.archive_index)];
        let mut buf = vec![0; self.dir_entry.file_length as usize];
        let mut file = File::open(archive_path)?;
        file.seek(SeekFrom::Start(self.dir_entry.archive_offset as u64))?;
        file.read_exact(&mut buf)?;
        Ok(Cow::Owned(buf))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VPKDirectoryEntry {
    pub crc32: u32,
    pub preload_length: u16,
    pub archive_index: u16,
    pub archive_offset: u32,
    pub file_length: u32,
    pub suffix: u16,
}
impl VPKDirectoryEntry {
    pub fn read_le(r: &mut impl Read) -> std::io::Result<Self> {
        let crc32 = read_u32(r)?;
        let preload_length = read_u16(r)?;
        let archive_index = read_u16(r)?;
        let archive_offset = read_u32(r)?;
        let file_length = read_u32(r)?;
        let suffix = read_u16(r)?;

        Ok(Self {
            crc32,
            preload_length,
            archive_index,
            archive_offset,
            file_length,
            suffix,
        })
    }
}

/// A handle holds both the [`VPK`] and a held [`VPKEntry`].
/// This is useful for [`VPKEntry::get`] where the [`VPKEntry`] needs to know
/// the parent data.
#[derive(Debug)]
pub struct VPKEntryHandle<'a> {
    /// The [`VPK`] that holds this [`VPKEntry`]
    pub vpk: &'a VPK,
    pub entry: &'a VPKEntry,
}
impl<'a> VPKEntryHandle<'a> {
    pub fn get(&self) -> Result<Cow<'a, [u8]>, Error> {
        self.entry.get(self.vpk)
    }
}
