use crate::access::DirFile;
use crate::access::DirFileBigRef;
use crate::access::DirFileBigRefLowercase;
use crate::access::DirFileEntryMap;
use crate::access::DirFileRef;
use crate::access::DirFileRefLowercase;
use crate::entry::*;
use crate::structs::*;
use crate::Error;

use binread::BinReaderExt;
use indexmap::Equivalent;
use indexmap::IndexMap;
use std::borrow::Cow;

use std::collections::HashMap;
use std::hash::Hash;
use std::io::Cursor;
use std::io::{Seek, SeekFrom};
use std::mem;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

const VPK_SIGNATURE: u32 = 0x55aa1234;
const VPK_SELF_HASHES_LENGTH: u32 = 48;

// TODO: comments about what these are
// TODO: add more, possibly remove uncommon or less useful entries
/// Extensions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ext<'a> {
    Vmt,
    Vtf,
    Mdl,
    Scr,
    Xsc,
    Gam,
    Lst,
    Dsp,
    Ico,
    Icns,
    Bmp,
    Dat,
    Other(Cow<'a, str>),
}
impl<'a> Ext<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Ext::Vmt => "vmt",
            Ext::Vtf => "vtf",
            Ext::Mdl => "mdl",
            Ext::Scr => "scr",
            Ext::Xsc => "xsc",
            Ext::Gam => "gam",
            Ext::Lst => "lst",
            Ext::Dsp => "dsp",
            Ext::Ico => "ico",
            Ext::Icns => "icns",
            Ext::Bmp => "bmp",
            Ext::Dat => "dat",
            Ext::Other(s) => s.as_ref(),
        }
    }

    pub fn as_ref(&self) -> Ext<'_> {
        match self {
            Ext::Vmt => Ext::Vmt,
            Ext::Vtf => Ext::Vtf,
            Ext::Mdl => Ext::Mdl,
            Ext::Scr => Ext::Scr,
            Ext::Xsc => Ext::Xsc,
            Ext::Gam => Ext::Gam,
            Ext::Lst => Ext::Lst,
            Ext::Dsp => Ext::Dsp,
            Ext::Ico => Ext::Ico,
            Ext::Icns => Ext::Icns,
            Ext::Bmp => Ext::Bmp,
            Ext::Dat => Ext::Dat,
            Ext::Other(s) => Ext::Other(Cow::Borrowed(s.as_ref())),
        }
    }

    pub fn from_ext_str(s: &'a str) -> Ext<'a> {
        let s = if s.chars().all(|c| c.is_ascii_lowercase()) {
            Cow::Borrowed(s)
        } else {
            Cow::Owned(s.to_ascii_lowercase())
        };
        match s.as_ref() {
            "vmt" => Ext::Vmt,
            "vtf" => Ext::Vtf,
            "mdl" => Ext::Mdl,
            "scr" => Ext::Scr,
            "xsc" => Ext::Xsc,
            "gam" => Ext::Gam,
            "lst" => Ext::Lst,
            "dsp" => Ext::Dsp,
            "ico" => Ext::Ico,
            "icns" => Ext::Icns,
            "bmp" => Ext::Bmp,
            "dat" => Ext::Dat,
            _ => Ext::Other(s),
        }
    }
}

// TODO: optionally check checksum
// TODO: Should we also lowercase non-ascii text? Windows
// does that.

#[derive(Clone)]
pub struct VPK {
    pub header_length: u32,
    pub header: VPKHeader,
    pub header_v2: Option<VPKHeaderV2>,
    pub header_v2_checksum: Option<VPKHeaderV2Checksum>,
    tree: VPKTree,

    /// The data in a dir is usually pretty small, so just keeping the loaded file
    /// is cheaper than reading out isolated preload data vecs and the like.
    pub(crate) data: Arc<[u8]>,
}

impl VPK {
    pub fn read(dir_path: &Path) -> Result<VPK, Error> {
        // Read the file into memory. Dir vpks are usually pretty small.
        let file: Arc<[u8]> = Arc::from(std::fs::read(dir_path)?);

        let mut reader = Cursor::new(file.as_ref());

        // Read main VPK header
        let header: VPKHeader = reader.read_le()?;

        if header.signature != VPK_SIGNATURE {
            return Err(Error::InvalidSignature);
        }
        if header.version > 2 {
            return Err(Error::UnsupportedVersion(header.version));
        }

        let mut vpk = VPK {
            header_length: 4 * 3,
            header,
            header_v2: None,
            header_v2_checksum: None,
            tree: VPKTree::default(),
            data: file.clone(),
        };

        if vpk.header.version == 2 {
            let header_v2: VPKHeaderV2 = reader.read_le()?;

            if header_v2.self_hashes_length != VPK_SELF_HASHES_LENGTH {
                return Err(Error::HashSizeMismatch);
            }
            vpk.header_length += 4 * 4;

            let checksum_offset: u32 = vpk.header.tree_length
                + header_v2.embed_chunk_length
                + header_v2.chunk_hashes_length;
            reader.seek(SeekFrom::Current(checksum_offset as i64))?;

            let header_v2_checksum: VPKHeaderV2Checksum = reader.read_le()?;

            vpk.header_v2 = Some(header_v2);
            vpk.header_v2_checksum = Some(header_v2_checksum);

            // Return seek to initial position - after header
            let header_length = mem::size_of::<VPKHeader>() + mem::size_of::<VPKHeaderV2>();
            reader.seek(SeekFrom::Start(header_length as u64))?;
        }

        // Read index tree
        // let mut avg_name = 0.0;
        // let mut name_count = 0;

        // Cache the archive paths for each archive index
        // This lets us share them, and also avoid formatting every time
        let mut archive_paths: HashMap<u16, Arc<str>> = HashMap::with_capacity(32);

        // let mut avg_path = 0.0;
        // let mut path_count = 0;

        // let mut avg_ext = 0.0;
        // let mut ext_count = 0;

        // let mut avg_path_count = 0.0;
        // let mut path_count_count = 0;

        // TODO: don't require this to be a str? Weird systems might have bad utf8 in the paths
        let dir_path = dir_path.to_str().unwrap();
        loop {
            // let ext_start = std::time::Instant::now();
            let ext = read_cstring(&mut reader)?;
            if ext.is_empty() {
                break;
            }

            let ext = Ext::from_ext_str(ext);

            // let mut p_count = 0;
            loop {
                // let path_start = std::time::Instant::now();

                let path_pos = reader.position();
                let path = read_cstring(&mut reader)?;
                if path.is_empty() {
                    break;
                }

                let path_end = reader.position() - 1;

                // p_count += 1;

                loop {
                    // let name_start = std::time::Instant::now();
                    let name_pos = reader.position();
                    let name = read_cstring(&mut reader)?;
                    if name.is_empty() {
                        break;
                    }

                    let name_end = reader.position() - 1;

                    // TODO: it might be possible to instead not do any str conversion
                    // and use the `&str`, or rather perhaps some reference into `&data`
                    // to avoid the conversion + allocation when this is initialized.
                    // But that would complicate things a good bit..
                    // Like, we'd need to somehow be able to get the values for hashing in the
                    // `DirFile` and also for comparison..
                    // let name = name.to_lowercase();

                    let mut dir_entry: VPKDirectoryEntry = reader.read_le()?;

                    if dir_entry.suffix != 0xffff {
                        return Err(Error::MalformedIndex);
                    }

                    if dir_entry.archive_index == 0x7fff {
                        dir_entry.archive_offset += vpk.header_length + vpk.header.tree_length;
                    }

                    let archive_path = archive_paths
                        .entry(dir_entry.archive_index)
                        .or_insert_with(|| {
                            let archive_path = dir_path
                                .replace("dir.", &format!("{:03}.", dir_entry.archive_index));
                            Arc::from(archive_path)
                        })
                        .clone();

                    let vpk_entry = VPKEntry {
                        dir_entry,
                        archive_path,
                        // This can't be >usize becuase we're reading from a vec
                        preload_start: reader.position() as usize,
                    };

                    reader.seek(SeekFrom::Current(dir_entry.preload_length as i64))?;

                    // vpk.tree.insert(&ext, path.clone(), name, vpk_entry);
                    vpk.tree.insert(
                        file.clone(),
                        &ext,
                        path_pos as usize..path_end as usize,
                        name_pos as usize..name_end as usize,
                        vpk_entry,
                    );

                    // let name_end = std::time::Instant::now();
                    // let name_time = name_end - name_start;
                    // name_count += 1;
                    // avg_name += (name_time.as_micros() as f32 - avg_name) / name_count as f32;
                }

                // let path_end = std::time::Instant::now();
                // let path_time = path_end - path_start;
                // path_count += 1;
                // avg_path += (path_time.as_micros() as f32 - avg_path) / path_count as f32;

                // path_count_count += 1;
                // avg_path_count += (p_count as f32 - avg_path_count) / path_count_count as f32;
            }

            // let ext_end = std::time::Instant::now();
            // let ext_time = ext_end - ext_start;
            // ext_count += 1;
            // avg_ext += (ext_time.as_micros() as f32 - avg_ext) / ext_count as f32;
        }

        // eprintln!("avg_ext: {} ms", avg_ext / 1000.0);
        // // microseconds
        // eprintln!("avg_path {}", avg_path);
        // eprintln!("avg_name {}", avg_name);

        // eprintln!("avg_path_count {}", avg_path_count);

        Ok(vpk)
    }

    pub fn get_direct<'s, K: Equivalent<DirFile> + Hash>(
        &'s self,
        ext: &Ext<'_>,
        re: K,
    ) -> Option<VPKEntryHandle<'s>> {
        self.tree
            .get_direct(ext, re)
            .map(|entry| VPKEntryHandle { vpk: self, entry })
    }

    pub fn get<'s>(
        &'s self,
        ext: &Ext<'_>,
        dir: &str,
        filename: &str,
    ) -> Option<VPKEntryHandle<'s>> {
        self.tree
            .get(ext, dir, filename)
            .map(|entry| VPKEntryHandle { vpk: self, entry })
    }

    pub fn get_ignore_case<'s>(
        &'s self,
        ext: &Ext<'_>,
        dir: &str,
        filename: &str,
    ) -> Option<VPKEntryHandle<'s>> {
        self.tree
            .get_ignore_case(ext, dir, filename)
            .map(|entry| VPKEntryHandle { vpk: self, entry })
    }
}

impl std::fmt::Debug for VPK {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VPK")
            .field("header_length", &self.header_length)
            .field("header", &self.header)
            .field("header_v2", &self.header_v2)
            .field("header_v2_checksum", &self.header_v2_checksum)
            .field("tree", &self.tree)
            .field("data", &self.data.len())
            .finish()
    }
}

// TODO: allow customization of indexmap hasher?

// VPK Files are a tree three levels deep.
// THe first level is file extensions (vmt, vtf, etc.)
// Then directory paths (materials/brick, materials/decals/asphalt, etc.)
// The third level is file names, file information, and preload data

/// The three level file tree of the VPK.
/// You should use [`get`] to access.
/// Exts/dirs/filenames are stored lowercase.
#[derive(Debug, Default, Clone)]
pub struct VPKTree {
    // TODO: consider whether to lowercase all paths always
    // filename!
    pub vmt: DirFileEntryMap,
    pub vtf: DirFileEntryMap,
    pub mdl: DirFileEntryMap,
    pub scr: DirFileEntryMap,
    pub xsc: DirFileEntryMap,
    pub gam: DirFileEntryMap,
    pub lst: DirFileEntryMap,
    pub dsp: DirFileEntryMap,
    pub ico: DirFileEntryMap,
    pub icns: DirFileEntryMap,
    pub bmp: DirFileEntryMap,
    pub dat: DirFileEntryMap,
    /// (ext, dir file entry map)
    pub other: IndexMap<String, DirFileEntryMap>,
}
impl VPKTree {
    pub fn for_ext(&self, ext: &Ext<'_>) -> Option<&DirFileEntryMap> {
        match ext {
            Ext::Vmt => Some(&self.vmt),
            Ext::Vtf => Some(&self.vtf),
            Ext::Mdl => Some(&self.mdl),
            Ext::Scr => Some(&self.scr),
            Ext::Xsc => Some(&self.xsc),
            Ext::Gam => Some(&self.gam),
            Ext::Lst => Some(&self.lst),
            Ext::Dsp => Some(&self.dsp),
            Ext::Ico => Some(&self.ico),
            Ext::Icns => Some(&self.icns),
            Ext::Bmp => Some(&self.bmp),
            Ext::Dat => Some(&self.dat),
            Ext::Other(ext) => self.other.get(ext.as_ref()),
        }
    }

    pub fn get_direct<K: Equivalent<DirFile> + Hash>(
        &self,
        ext: &Ext<'_>,
        re: K,
    ) -> Option<&VPKEntry> {
        self.for_ext(ext)?.get(&re)
    }

    /// Get a path that may be like:  
    /// ext: "vmt"; dir: "materials/" filename: "concrete/concretefloor001a"
    /// Essentially, it doesn't have the root dir but it does have one or more of the subdirs on it.
    pub fn get(&self, ext: &Ext<'_>, dir_start: &str, big_filename: &str) -> Option<&VPKEntry> {
        let re = DirFileBigRef::new(dir_start, big_filename);
        self.get_direct(ext, re)
    }

    /// Get a path that may be like:
    /// ex: "vmt"; dir: "materials/" filename: "concrete/concretefloor001a"
    /// Essentially, it doesn't have the root dir but it does have one or more of the subdirs on it.
    /// This version is case insensitive.
    pub fn get_ignore_case(
        &self,
        ext: &Ext<'_>,
        dir_start: &str,
        big_filename: &str,
    ) -> Option<&VPKEntry> {
        let re = DirFileBigRefLowercase::new(dir_start, big_filename);
        self.get_direct(ext, re)
    }

    pub fn getf(&self, ext: &Ext<'_>, dir: &str, filename: &str) -> Option<&VPKEntry> {
        self.get_direct(ext, DirFileRef::new(dir, filename))
    }

    pub fn getf_ignore_case(&self, ext: &Ext<'_>, dir: &str, filename: &str) -> Option<&VPKEntry> {
        self.get_direct(ext, DirFileRefLowercase::new(dir, filename))
    }

    fn insert(
        &mut self,
        data: Arc<[u8]>,
        ext: &Ext<'_>,
        dir: Range<usize>,
        filename: Range<usize>,
        entry: VPKEntry,
    ) {
        let re = DirFile::new(data, dir, filename);

        match ext {
            Ext::Vmt => self.vmt.insert(re, entry),
            Ext::Vtf => self.vtf.insert(re, entry),
            Ext::Mdl => self.mdl.insert(re, entry),
            Ext::Scr => self.scr.insert(re, entry),
            Ext::Xsc => self.xsc.insert(re, entry),
            Ext::Gam => self.gam.insert(re, entry),
            Ext::Lst => self.lst.insert(re, entry),
            Ext::Dsp => self.dsp.insert(re, entry),
            Ext::Ico => self.ico.insert(re, entry),
            Ext::Icns => self.icns.insert(re, entry),
            Ext::Bmp => self.bmp.insert(re, entry),
            Ext::Dat => self.dat.insert(re, entry),
            Ext::Other(ext) => {
                if let Some(map) = self.other.get_mut(ext.as_ref()) {
                    map.insert(re, entry);
                } else {
                    let mut map = DirFileEntryMap::default();
                    map.insert(re, entry);
                    self.other.insert(ext.as_ref().to_string(), map);
                }

                // for some reason match requires the same return type despite being used as a
                // statement...
                None
            }
        };
    }
}

fn read_cstring<'a>(reader: &mut Cursor<&'a [u8]>) -> Result<&'a str, Error> {
    // Since we know it is a cursor, we can just get the current position
    // and then search for the next null byte
    let start = reader.position() as usize;
    let data = reader.get_ref();
    let end = data[start..]
        .iter()
        .position(|&x| x == 0)
        .map(|x| start + x)
        .ok_or_else(|| {
            Error::ReadError(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Could not find null byte",
            ))
        })?;

    // let string = String::from_utf8_lossy(&data[start..end]).to_string();
    let string = std::str::from_utf8(&data[start..end]).map_err(|_| {
        Error::ReadError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Could not parse cstring",
        ))
    })?;

    // Advance past the cstring
    // end will be at the null byte
    reader.seek(SeekFrom::Start((end + 1) as u64))?;

    Ok(string)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{vpk::read_cstring, VPK};

    #[test]
    fn test_read_cstring_with_null_byte() {
        let data = b"hello\0world";
        let mut cursor = Cursor::new(data.as_ref());

        let result = read_cstring(&mut cursor).unwrap();
        let remaining_data = &data[cursor.position() as usize..];

        assert_eq!(result, "hello");
        assert_eq!(remaining_data, b"world");
    }

    #[test]
    fn test_read_cstring_without_null_byte() {
        let data = b"hello world"; // No null byte
        let mut cursor = Cursor::new(data.as_ref());

        assert!(read_cstring(&mut cursor).is_err());
    }

    #[test]
    fn test_vpk_read() {
        if let Ok(file_path) = std::env::var("VPK_FILE") {
            let file_path = std::path::Path::new(&file_path);

            let _res = VPK::read(file_path).unwrap();
        }
    }
}
