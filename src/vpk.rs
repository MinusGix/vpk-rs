use crate::entry::*;
use crate::structs::*;
use crate::Error;

use binread::BinReaderExt;
use std::collections::HashMap;

use std::io::Cursor;
use std::io::{Seek, SeekFrom};
use std::mem;
use std::path::Path;

const VPK_SIGNATURE: u32 = 0x55aa1234;
const VPK_SELF_HASHES_LENGTH: u32 = 48;

#[derive(Debug)]
pub struct VPK {
    pub header_length: u32,
    pub header: VPKHeader,
    pub header_v2: Option<VPKHeaderV2>,
    pub header_v2_checksum: Option<VPKHeaderV2Checksum>,
    pub tree: HashMap<String, VPKEntry>,

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
            tree: HashMap::new(),
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

        // Read index tree
        loop {
            let ext = read_cstring(&mut reader)?;
            if ext.is_empty() {
                break;
            }

            loop {
                let mut path = read_cstring(&mut reader)?;
                if path.is_empty() {
                    break;
                }
                if path != " " {
                    path += "/";
                } else {
                    path = String::new();
                }

                loop {
                    let name = read_cstring(&mut reader)?;
                    if name.is_empty() {
                        break;
                    }

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

                    vpk.tree
                        .insert(format!("{}{}.{}", path, name, ext), vpk_entry);
                }
            }
        }

        vpk.data = file;

        Ok(vpk)
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = (&str, VPKEntryHandle<'_>)> {
        self.tree.iter().map(move |(key, v)| {
            (
                key.as_str(),
                VPKEntryHandle {
                    vpk: self,
                    entry: v,
                },
            )
        })
    }
}

fn read_cstring(reader: &mut Cursor<&[u8]>) -> Result<String, Error> {
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

    let string = String::from_utf8_lossy(&data[start..end]).to_string();

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
}
