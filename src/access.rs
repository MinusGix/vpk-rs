use std::{
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};

use indexmap::{Equivalent, IndexMap};

use crate::entry::VPKEntry;

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
#[derive(Clone)]
pub struct DirFile {
    /// A copy of the data, this lets us avoid keeping a copy of `dir` or `filename`
    data: Arc<[u8]>,
    dir: Range<usize>,
    filename: Range<usize>,
}
impl DirFile {
    pub fn new(data: Arc<[u8]>, dir: Range<usize>, filename: Range<usize>) -> DirFile {
        DirFile {
            data,
            dir,
            filename,
        }
    }

    pub fn dir(&self) -> &[u8] {
        &self.data[self.dir.clone()]
    }

    pub fn filename(&self) -> &[u8] {
        &self.data[self.filename.clone()]
    }
}
// We have to implement hash manually to ensure consistent behavior
// because currently the comment for the unstable `Hasher::write_str` says that the default
// hash for str is not decided.
impl Hash for DirFile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_bytes_as_lowercase(state, self.dir());
        state.write_u8(0xff);
        hash_bytes_as_lowercase(state, self.filename());
        state.write_u8(0xff);
    }
}
impl PartialEq for DirFile {
    fn eq(&self, other: &Self) -> bool {
        self.dir().eq_ignore_ascii_case(other.dir())
            && self.filename().eq_ignore_ascii_case(other.filename())
    }
}
impl Eq for DirFile {}
impl std::fmt::Debug for DirFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dir = std::str::from_utf8(self.dir()).unwrap();
        let filename = std::str::from_utf8(self.filename()).unwrap();
        write!(f, "DirFile({:?}, {:?})", dir, filename)
    }
}

// TODO: we could just get rid of this because with the way DirFile changed, we have to use eq_ignore_ascii_case *anyway*
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
        self.dir.as_bytes().eq_ignore_ascii_case(key.dir())
            && self
                .filename
                .as_bytes()
                .eq_ignore_ascii_case(key.filename())
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
        self.dir.as_bytes().eq_ignore_ascii_case(key.dir())
            && self
                .filename
                .as_bytes()
                .eq_ignore_ascii_case(key.filename())
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

        let key_dir = key.dir();
        let start_dir = &key_dir[..dir_size];
        if !start_dir.eq_ignore_ascii_case(self.dir.as_bytes()) {
            return false;
        }

        let rem_dir = key_dir.get(dir_size..).unwrap_or(b"");
        if self.extra_dir.is_empty() {
            rem_dir.is_empty()
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
        } else if let Some(rem_dir) = rem_dir.strip_prefix(b"/") {
            rem_dir.eq_ignore_ascii_case(self.extra_dir.as_bytes())
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
        } else {
            rem_dir.eq_ignore_ascii_case(self.extra_dir.as_bytes())
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
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

        let key_dir = key.dir();
        let start_dir = &key_dir[..dir_size];
        if !start_dir.eq_ignore_ascii_case(self.dir.as_bytes()) {
            return false;
        }

        let rem_dir = key_dir.get(dir_size..).unwrap_or(b"");
        if self.extra_dir.is_empty() {
            rem_dir.is_empty()
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
        } else if let Some(rem_dir) = rem_dir.strip_prefix(b"/") {
            rem_dir.eq_ignore_ascii_case(self.extra_dir.as_bytes())
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
        } else {
            rem_dir.eq_ignore_ascii_case(self.extra_dir.as_bytes())
                && self
                    .filename
                    .as_bytes()
                    .eq_ignore_ascii_case(key.filename())
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
pub type DirFileEntryMap = IndexMap<DirFile, VPKEntry>;

#[cfg(test)]
mod tests {
    use std::{
        hash::{Hash, Hasher},
        sync::Arc,
    };

    use indexmap::Equivalent;

    use super::{DirFile, DirFileBigRef, DirFileBigRefLowercase, DirFileRef};

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
        let data = b"materials;concrete";
        let data: Arc<[u8]> = Arc::from(*data);
        let a = DirFile::new(data.clone(), 0..9, 10..18);
        a_eq(&a, DirFileBigRef::new("materials", "concrete"));
        a_eq(&a, DirFileBigRefLowercase::new("materials", "concrete"));
        a_eq(&a, DirFileBigRefLowercase::new("mAterials", "CONCrete"));

        let data = b"materials/concrete;concretefloor001a";
        let data: Arc<[u8]> = Arc::from(*data);
        // let a = DirFile::new(
        //     Arc::from("materials/concrete"),
        //     "concretefloor001a".to_string(),
        // );
        let a = DirFile::new(data.clone(), 0..18, 19..data.len());

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

        let data = b"materials/concrete/concretefloor001a;concretefloor001a";
        let data: Arc<[u8]> = Arc::from(*data);
        // let a = DirFile::new(
        //     Arc::from("materials/concrete/concretefloor001a"),
        //     "concretefloor001a".to_string(),
        // );
        let a = DirFile::new(data.clone(), 0..36, 37..data.len());
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

        let data = b"materials/concrete;computerwall003";
        let data: Arc<[u8]> = Arc::from(*data);
        // let a = DirFile::new(
        //     Arc::from("materials/concrete"),
        //     "computerwall003".to_string(),
        // );
        let a = DirFile::new(data.clone(), 0..18, 19..data.len());
        let b = DirFileBigRefLowercase::new("materials", "CONCRETE/COMPUTERWALL003");
        a_eq(&a, DirFileRef::new("materials/concrete", "computerwall003"));
        a_eq(&a, b);
    }
}
