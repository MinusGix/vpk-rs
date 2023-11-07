use binread::BinRead;
use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::Arc;

use crate::VPK;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VPKEntry {
    pub dir_entry: VPKDirectoryEntry,
    pub archive_path: Arc<str>,
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

        let mut buf = vec![0; self.dir_entry.file_length as usize];
        let archive_path: &str = &self.archive_path;
        let mut file = File::open(archive_path)?;
        file.seek(SeekFrom::Start(self.dir_entry.archive_offset as u64))?;
        file.read_exact(&mut buf)?;
        Ok(Cow::Owned(buf))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct VPKDirectoryEntry {
    pub crc32: u32,
    pub preload_length: u16,
    pub archive_index: u16,
    pub archive_offset: u32,
    pub file_length: u32,
    pub suffix: u16,
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
