use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::parse::{read_u16, read_u32};
use crate::VPK;

pub trait VpkReaderProvider {
    type Reader<'a>: Read + Seek + 'a
    where
        Self: 'a;

    /// Return a reader for the given archive index.  
    /// Note: if you want the read to continue despite returning an error, then you should just
    /// ignore the error and return `None`. Any erros will be returned by the `get` function.
    fn vpk_reader(&self, archive_index: u16) -> std::io::Result<Option<Self::Reader<'_>>>;
}

// I hate this
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

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

    pub fn archive_index(&self) -> u16 {
        self.dir_entry.archive_index
    }

    /// Get the data in the [`VPKEntry`]
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.
    /// For other files, it has to open the resident archive file and read the requisite data.
    ///
    /// If `files` does not contain the archive file for this entry, then it will open the archive
    /// file.
    /// If `files` does contain the archive file for this entry, then it will use that file. This is
    /// useful if you want to read multiple files from the same archive file.
    pub fn get_with_files<'v>(
        &self,
        parent: &'v VPK,
        prov: &impl VpkReaderProvider,
    ) -> Result<Cow<'v, [u8]>, Error> {
        if self.dir_entry.archive_index == 0x7fff {
            self.get(parent)
        } else {
            let archive_index = self.archive_index();
            let archive_reader = prov.vpk_reader(archive_index)?;

            self.get_with_file(parent, archive_reader)
        }
    }

    /// Get the data in the [`VPKEntry`]  
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.  
    /// For other files, it has to open the resident archive file and read the requisite data.  
    ///   
    /// If `file` is `None`, then it will open the archive file.
    /// If `file` is `Some`, then it will use that file. This is useful if you want to read multiple
    /// files from the same archive file.
    pub fn get_with_file<'v, R: Read + Seek>(
        &self,
        parent: &'v VPK,
        mut reader: Option<R>,
    ) -> Result<Cow<'v, [u8]>, Error> {
        if self.dir_entry.archive_index == 0x7fff {
            let preload_data = &parent.data[self.preload_interval()];
            return Ok(Cow::Borrowed(preload_data));
        }

        let mut buf = vec![0; self.dir_entry.file_length as usize];
        let mut tmp;
        let file: &mut dyn ReadSeek = if let Some(file) = reader.as_mut() {
            &mut *file
        } else {
            let archive_path = &parent.archive_paths[usize::from(self.dir_entry.archive_index)];
            tmp = File::open(archive_path)?;
            &mut tmp
        };
        file.seek(SeekFrom::Start(self.dir_entry.archive_offset as u64))?;
        file.read_exact(&mut buf)?;
        Ok(Cow::Owned(buf))
    }

    /// Get the data in the [`VPKEntry`]
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.
    /// For other files, it has to open the resident archive file and read the requisite data.
    pub fn get<'v>(&self, parent: &'v VPK) -> Result<Cow<'v, [u8]>, Error> {
        self.get_with_file::<File>(parent, None)
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
    /// Get the data in the [`VPKEntry`]
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.
    /// For other files, it has to open the resident archive file and read the requisite data.
    ///
    /// If `files` does not contain the archive file for this entry, then it will open the archive
    /// file.
    /// If `files` does contain the archive file for this entry, then it will use that file. This is
    /// useful if you want to read multiple files from the same archive file.
    pub fn get_with_files(&self, prov: &impl VpkReaderProvider) -> Result<Cow<'a, [u8]>, Error> {
        self.entry.get_with_files(self.vpk, prov)
    }

    /// Get the data in the [`VPKEntry`]  
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.  
    /// For other files, it has to open the resident archive file and read the requisite data.  
    ///   
    /// If `file` is `None`, then it will open the archive file.
    /// If `file` is `Some`, then it will use that file. This is useful if you want to read multiple
    /// files from the same archive file.
    pub fn get_with_file<R: Read + Seek>(&self, file: Option<R>) -> Result<Cow<'a, [u8]>, Error> {
        self.entry.get_with_file(self.vpk, file)
    }

    /// Get the data in the [`VPKEntry`]
    /// If this is preloaded data, aka the data is stored in the directory file, then it can easily
    /// return a `Cow::Borrowed`. Typically this is only small files, like `vmt`s.
    /// For other files, it has to open the resident archive file and read the requisite data.
    pub fn get(&self) -> Result<Cow<'a, [u8]>, Error> {
        self.entry.get(self.vpk)
    }

    pub fn archive_index(&self) -> u16 {
        self.entry.archive_index()
    }

    /// Only returns `None` if the `archive_index` is `0x7fff`  
    ///   
    /// # Panics
    /// If the archive index is not `0x7fff`, and it does not exist in `vpk`.  
    /// This should *only* happen if there was a bug in the parsing logic, or some vpk entries were
    /// manually constructed with invalid archive indices.
    pub fn archive_path(&self) -> Option<&str> {
        if self.entry.dir_entry.archive_index == 0x7fff {
            return None;
        }

        let archive_index = usize::from(self.entry.dir_entry.archive_index);
        Some(&self.vpk.archive_paths[archive_index])
    }
}
