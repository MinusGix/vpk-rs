use crate::entry::*;
use crate::structs::*;
use crate::Error;

use binread::BinReaderExt;
use indexmap::Equivalent;
use indexmap::IndexMap;
use std::borrow::Cow;

use std::hash::Hash;
use std::hash::Hasher;
use std::io::Cursor;
use std::io::{Seek, SeekFrom};
use std::mem;
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

#[derive(Clone)]
pub struct VPK {
    pub header_length: u32,
    pub header: VPKHeader,
    pub header_v2: Option<VPKHeaderV2>,
    pub header_v2_checksum: Option<VPKHeaderV2Checksum>,
    // pub tree: HashMap<String, VPKEntry>,
    pub tree: VPKTree,

    /// The data in a dir is usually pretty small, so just keeping the loaded file
    /// is cheaper than reading out isolated preload data vecs and the like.
    pub(crate) data: Vec<u8>,
}

impl VPK {
    pub fn read(dir_path: &Path) -> Result<VPK, Error> {
        // Read the file into memory. Dir vpks are usually pretty small.
        let file = std::fs::read(dir_path)?;

        let mut reader = Cursor::new(file.as_slice());

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
            data: Vec::new(),
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

        let root_str: Arc<str> = Arc::from("");

        // Read index tree
        loop {
            let ext = read_cstring(&mut reader)?;
            if ext.is_empty() {
                break;
            }

            let ext = Ext::from_ext_str(ext);

            loop {
                let path = read_cstring(&mut reader)?;
                if path.is_empty() {
                    break;
                }

                // TODO: is it correct to turn this lowercase?
                // There are various paths that are sometimes UPPERCASE, and sometimes not,
                // so I expect it is fine.
                // TODO: if it is correct, then should we also lowercase non-ascii text? Windows
                // does that.
                let path: Arc<str> = if path == " " {
                    root_str.clone()
                } else {
                    let path = path.to_ascii_lowercase();
                    Arc::from(path)
                };

                loop {
                    let name = read_cstring(&mut reader)?;
                    if name.is_empty() {
                        break;
                    }

                    let name = name.to_lowercase();

                    let mut dir_entry: VPKDirectoryEntry = reader.read_le()?;

                    if dir_entry.suffix != 0xffff {
                        return Err(Error::MalformedIndex);
                    }

                    if dir_entry.archive_index == 0x7fff {
                        dir_entry.archive_offset += vpk.header_length + vpk.header.tree_length;
                    }

                    let _dir_path = dir_path.to_str().unwrap();
                    let archive_path =
                        _dir_path.replace("dir.", &format!("{:03}.", dir_entry.archive_index));
                    let vpk_entry = VPKEntry {
                        dir_entry,
                        archive_path,
                        // This can't be >usize becuase we're reading from a vec
                        preload_start: reader.position() as usize,
                    };

                    vpk.tree.insert(&ext, path.clone(), name, vpk_entry);

                    // let name = if path != " " {
                    //     format!("{path}/{name}.{ext}")
                    // } else {
                    //     format!("{name}.{ext}")
                    // };

                    // vpk.tree.insert(name, vpk_entry);
                }
            }
        }

        vpk.data = file;

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

// TODO: allow customization of hasher?

// VPK Files are a tree three levels deep.
// THe first level is file extensions (vmt, vtf, etc.)
// Then directory paths (materials/brick, materials/decals/asphalt, etc.)
// The third level is file names, file information, and preload data

fn hash_bytes<H: Hasher>(state: &mut H, bytes: &[u8]) {
    // We can't trust that the hash implementation doesn't do a slice of bytes differently from
    // writing bytes individually, and we need to write some bytes individually in some of the hash
    //impls
    for v in bytes {
        state.write_u8(*v);
    }
}

fn hash_bytes_as_lowercase<H: Hasher>(state: &mut H, bytes: &[u8]) {
    for v in bytes {
        state.write_u8(v.to_ascii_lowercase());
    }
}

fn hash_str<H: Hasher>(state: &mut H, s: &str) {
    hash_bytes(state, s.as_bytes());
    state.write_u8(0xff);
}

fn hash_str_as_lowercase<H: Hasher>(state: &mut H, s: &str) {
    hash_bytes_as_lowercase(state, s.as_bytes());
    state.write_u8(0xff);
}

/// A reference to a specific (dir, filename), without the extension.  
/// This should be lowercase!
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirFile {
    pub dir: Arc<str>,
    pub filename: String,
}
impl DirFile {
    pub fn new(dir: Arc<str>, filename: String) -> DirFile {
        DirFile { dir, filename }
    }
}
// We have to implement hash manually to ensure consistent behavior
// because currently the comment for the unstable `Hasher::write_str` says that the default
// hash for str is not decided.
impl Hash for DirFile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_str(state, &self.dir);
        hash_str(state, &self.filename);
    }
}

/// A reference to a specific (dir, filename), without the extension.  
/// This should be lowercase!
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirFileRef<'a> {
    pub dir: &'a str,
    pub filename: &'a str,
}
impl<'a> DirFileRef<'a> {
    pub fn new(dir: &'a str, filename: &'a str) -> DirFileRef<'a> {
        DirFileRef { dir, filename }
    }
}
impl Equivalent<DirFile> for DirFileRef<'_> {
    fn equivalent(&self, key: &DirFile) -> bool {
        self.dir == &*key.dir && self.filename == key.filename
    }
}
impl Hash for DirFileRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_str(state, self.dir);
        hash_str(state, self.filename);
    }
}

/// A dir file ref where the dir/filename may not be lowercase
/// and so must be proactively compared as if they were lowercase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirFileRefLowercase<'a> {
    pub dir: &'a str,
    pub filename: &'a str,
}
impl<'a> DirFileRefLowercase<'a> {
    pub fn new(dir: &'a str, filename: &'a str) -> DirFileRefLowercase<'a> {
        DirFileRefLowercase { dir, filename }
    }
}
impl Equivalent<DirFile> for DirFileRefLowercase<'_> {
    fn equivalent(&self, key: &DirFile) -> bool {
        self.dir.eq_ignore_ascii_case(&key.dir) && self.filename.eq_ignore_ascii_case(&key.filename)
    }
}
impl Hash for DirFileRefLowercase<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_str_as_lowercase(state, self.dir);
        hash_str_as_lowercase(state, self.filename);
    }
}

/// A dir file ref to a specific (dir, filename), without the extension.
/// This should be lowercase!
/// The filename is potentially 'big', and is broken apart if needed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirFileBigRef<'a> {
    /// Should *not* end with a '/'
    pub dir: &'a str,
    /// Should *not* start with a '/'
    pub extra_dir: &'a str,
    pub filename: &'a str,
}
impl<'a> DirFileBigRef<'a> {
    pub fn new(dir: &'a str, big_filename: &'a str) -> DirFileBigRef<'a> {
        let res = big_filename.rsplit_once('/');
        let (extra_dir, filename) = match res {
            Some(v) => v,
            None => ("", big_filename),
        };

        DirFileBigRef {
            dir,
            extra_dir,
            filename,
        }
    }
}
// TODO: write some tests for this
impl Equivalent<DirFile> for DirFileBigRef<'_> {
    fn equivalent(&self, key: &DirFile) -> bool {
        let dir_size = self.dir.len();
        let total_size = dir_size + self.extra_dir.len();
        if total_size > key.dir.len() {
            return false;
        }

        let start_dir = &key.dir[..dir_size];
        if start_dir != self.dir {
            return false;
        }

        let rem_dir = key.dir.get(dir_size..).unwrap_or("");
        if self.extra_dir.is_empty() {
            rem_dir.is_empty() && self.filename == key.filename
        } else if let Some(rem_dir) = rem_dir.strip_prefix('/') {
            rem_dir == self.extra_dir && self.filename == key.filename
        } else {
            rem_dir == self.extra_dir && self.filename == key.filename
        }
    }
}
impl Hash for DirFileBigRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_bytes(state, self.dir.as_bytes());
        if !self.extra_dir.is_empty() {
            if !self.dir.is_empty() {
                state.write_u8(b'/');
            }
            hash_bytes(state, self.extra_dir.as_bytes());
        }
        state.write_u8(0xff);
        hash_str(state, self.filename);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirFileBigRefLowercase<'a> {
    /// Should *not* end with a '/'
    pub dir: &'a str,
    /// Should *not* start with a '/'
    pub extra_dir: &'a str,
    pub filename: &'a str,
}
impl<'a> DirFileBigRefLowercase<'a> {
    pub fn new(dir: &'a str, big_filename: &'a str) -> DirFileBigRefLowercase<'a> {
        let res = big_filename.rsplit_once('/');
        let (extra_dir, filename) = match res {
            Some(v) => v,
            None => ("", big_filename),
        };

        DirFileBigRefLowercase {
            dir,
            extra_dir,
            filename,
        }
    }
}
impl Equivalent<DirFile> for DirFileBigRefLowercase<'_> {
    fn equivalent(&self, key: &DirFile) -> bool {
        let dir_size = self.dir.len();
        let total_size = dir_size + self.extra_dir.len();
        if total_size > key.dir.len() {
            return false;
        }

        let start_dir = &key.dir[..dir_size];
        if !start_dir.eq_ignore_ascii_case(self.dir) {
            return false;
        }

        let rem_dir = key.dir.get(dir_size..).unwrap_or("");
        if self.extra_dir.is_empty() {
            rem_dir.is_empty() && self.filename.eq_ignore_ascii_case(&key.filename)
        } else if let Some(rem_dir) = rem_dir.strip_prefix('/') {
            rem_dir.eq_ignore_ascii_case(self.extra_dir)
                && self.filename.eq_ignore_ascii_case(&key.filename)
        } else {
            rem_dir.eq_ignore_ascii_case(self.extra_dir)
                && self.filename.eq_ignore_ascii_case(&key.filename)
        }
    }
}
impl Hash for DirFileBigRefLowercase<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_bytes_as_lowercase(state, self.dir.as_bytes());
        if !self.extra_dir.is_empty() {
            if !self.dir.is_empty() {
                state.write_u8(b'/');
            }
            hash_bytes_as_lowercase(state, self.extra_dir.as_bytes());
        }
        state.write_u8(0xff);
        hash_str_as_lowercase(state, self.filename);
    }
}

/// (Dir, Filename) -> VPKEntry
/// This uses a tuple because you rarely need to iterate over all the entries in a directory.
type DirFileEntryMap = IndexMap<DirFile, VPKEntry>;

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

    pub fn insert(&mut self, ext: &Ext<'_>, dir: Arc<str>, filename: String, entry: VPKEntry) {
        let re = DirFile::new(dir, filename.to_string());

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
    use super::*;
    use std::io::Cursor;

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

    #[track_caller]
    fn a_eq<T: Equivalent<DirFile> + Hash + std::fmt::Debug>(a: &DirFile, b: T) {
        assert!(b.equivalent(a), "expected {:?} == {:?}", a, b);

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        a.hash(&mut hasher);
        let a_hash = hasher.finish();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        b.hash(&mut hasher);
        let b_hash = hasher.finish();

        assert_eq!(a_hash, b_hash, "a: {a:?}; b: {b:?}");
    }

    #[track_caller]
    fn a_neq<T: Equivalent<DirFile> + std::fmt::Debug>(a: &DirFile, b: T) {
        assert!(!b.equivalent(a), "expected {:?} != {:?}", a, b);
    }

    #[test]
    fn dir_file_big() {
        let a = DirFile::new(Arc::from("materials"), "concrete".to_string());
        a_eq(&a, DirFileBigRef::new("materials", "concrete"));
        a_eq(&a, DirFileBigRefLowercase::new("materials", "concrete"));
        a_eq(&a, DirFileBigRefLowercase::new("mAterials", "CONCrete"));

        let a = DirFile::new(
            Arc::from("materials/concrete"),
            "concretefloor001a".to_string(),
        );

        a_eq(
            &a,
            DirFileBigRef::new("materials", "concrete/concretefloor001a"),
        );
        a_eq(
            &a,
            DirFileBigRefLowercase::new("materials", "concrete/concretefloor001a"),
        );
        a_eq(
            &a,
            DirFileBigRefLowercase::new("materiaLs", "cOncrete/concretefloor001A"),
        );

        a_neq(
            &a,
            DirFileBigRef::new("materials", "concrete/concretefloor001b"),
        );
        a_neq(
            &a,
            DirFileBigRef::new("materials", "concrete/concretefloor001"),
        );
        a_eq(
            &a,
            DirFileBigRef::new("materials/concrete", "concretefloor001a"),
        );
        a_neq(
            &a,
            DirFileBigRefLowercase::new("materials", "concrete/concretefloor001b"),
        );
        a_neq(
            &a,
            DirFileBigRefLowercase::new("materials", "concrete/concretefloor001"),
        );
        a_eq(
            &a,
            DirFileBigRefLowercase::new("materials/concrete", "concretefloor001a"),
        );

        let a = DirFile::new(
            Arc::from("materials/concrete/concretefloor001a"),
            "concretefloor001a".to_string(),
        );
        a_eq(
            &a,
            DirFileBigRef::new("materials", "concrete/concretefloor001a/concretefloor001a"),
        );
        a_eq(
            &a,
            DirFileBigRefLowercase::new(
                "materials",
                "concrete/concretefloor001a/concretefloor001a",
            ),
        );
        a_eq(
            &a,
            DirFileBigRefLowercase::new(
                "materiAls",
                "Concrete/concretefloor001A/Concretefloor001a",
            ),
        );

        let a = DirFile::new(
            Arc::from("materials/concrete"),
            "computerwall003".to_string(),
        );
        let b = DirFileBigRefLowercase::new("materials", "CONCRETE/COMPUTERWALL003");
        a_eq(&a, DirFileRef::new("materials/concrete", "computerwall003"));
        a_eq(&a, b);
    }
}
