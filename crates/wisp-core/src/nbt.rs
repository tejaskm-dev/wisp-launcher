//! Just enough NBT, Minecraft's binary data format, to read level.dat and player stats files.
//!
//! Files may be gzip- or zlib-compressed or plain. Reading is defensive: lengths are checked
//! against the bytes left and nesting is capped, so a damaged file fails instead of running away.

use std::collections::HashMap;
use std::io::Read;

/// The most a compressed file may unpack to. level.dat is a few kilobytes; this is generous.
const MAX_UNPACKED: u64 = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(HashMap<String, Tag>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Debug, thiserror::Error)]
pub enum NbtError {
    #[error("couldn't unpack the file: {0}")]
    Unpack(#[from] std::io::Error),
    #[error("the file ends too early")]
    Truncated,
    #[error("unknown tag type {0}")]
    UnknownTag(u8),
    #[error("nested too deeply")]
    TooDeep,
}

impl Tag {
    /// A child of a compound tag.
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(children) => children.get(key),
            _ => None,
        }
    }

    /// A descendant, following compound keys.
    pub fn at(&self, path: &[&str]) -> Option<&Tag> {
        path.iter().try_fold(self, |tag, key| tag.get(key))
    }

    /// Any whole number, widened.
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Tag::Byte(value) => Some(value.into()),
            Tag::Short(value) => Some(value.into()),
            Tag::Int(value) => Some(value.into()),
            Tag::Long(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        self.as_i64().map(|value| value != 0)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(text) => Some(text),
            _ => None,
        }
    }
}

/// Parses a file's bytes, unpacking gzip or zlib first if they're packed.
pub fn parse(bytes: &[u8]) -> Result<Tag, NbtError> {
    let unpacked;
    let raw = match bytes {
        [0x1f, 0x8b, ..] => {
            let mut out = Vec::new();
            flate2::read::MultiGzDecoder::new(bytes)
                .take(MAX_UNPACKED)
                .read_to_end(&mut out)?;
            unpacked = out;
            &unpacked[..]
        }
        [0x78, ..] => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(bytes)
                .take(MAX_UNPACKED)
                .read_to_end(&mut out)?;
            unpacked = out;
            &unpacked[..]
        }
        _ => bytes,
    };
    let mut reader = Reader { bytes: raw, at: 0 };
    let kind = reader.u8()?;
    if kind == 0 {
        return Ok(Tag::Compound(HashMap::new()));
    }
    reader.string()?; // The root's name, always empty in practice.
    reader.payload(kind, 0)
}

/// Reads and parses a file.
pub fn read_file(path: &std::path::Path) -> Result<Tag, NbtError> {
    parse(&std::fs::read(path)?)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], NbtError> {
        let end = self.at.checked_add(count).ok_or(NbtError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(NbtError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], NbtError> {
        Ok(self.take(N)?.try_into().expect("took exactly N bytes"))
    }

    fn u8(&mut self) -> Result<u8, NbtError> {
        Ok(self.array::<1>()?[0])
    }

    /// A length prefix, refused when it promises more items than bytes remain.
    fn length(&mut self, item_size: usize) -> Result<usize, NbtError> {
        let length = i32::from_be_bytes(self.array()?).max(0) as usize;
        let left = self.bytes.len() - self.at;
        if length.saturating_mul(item_size.max(1)) > left {
            return Err(NbtError::Truncated);
        }
        Ok(length)
    }

    /// Java's "modified UTF-8"; ordinary text reads the same as UTF-8.
    fn string(&mut self) -> Result<String, NbtError> {
        let length = u16::from_be_bytes(self.array()?) as usize;
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    fn payload(&mut self, kind: u8, depth: usize) -> Result<Tag, NbtError> {
        if depth > MAX_DEPTH {
            return Err(NbtError::TooDeep);
        }
        Ok(match kind {
            1 => Tag::Byte(i8::from_be_bytes(self.array()?)),
            2 => Tag::Short(i16::from_be_bytes(self.array()?)),
            3 => Tag::Int(i32::from_be_bytes(self.array()?)),
            4 => Tag::Long(i64::from_be_bytes(self.array()?)),
            5 => Tag::Float(f32::from_be_bytes(self.array()?)),
            6 => Tag::Double(f64::from_be_bytes(self.array()?)),
            7 => {
                let length = self.length(1)?;
                Tag::ByteArray(self.take(length)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let item = self.u8()?;
                let length = self.length(if item == 0 { 0 } else { 1 })?;
                let mut items = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    items.push(self.payload(item, depth + 1)?);
                }
                Tag::List(items)
            }
            10 => {
                let mut children = HashMap::new();
                loop {
                    let child = self.u8()?;
                    if child == 0 {
                        break;
                    }
                    let name = self.string()?;
                    children.insert(name, self.payload(child, depth + 1)?);
                }
                Tag::Compound(children)
            }
            11 => {
                let length = self.length(4)?;
                let mut items = Vec::with_capacity(length);
                for _ in 0..length {
                    items.push(i32::from_be_bytes(self.array()?));
                }
                Tag::IntArray(items)
            }
            12 => {
                let length = self.length(8)?;
                let mut items = Vec::with_capacity(length);
                for _ in 0..length {
                    items.push(i64::from_be_bytes(self.array()?));
                }
                Tag::LongArray(items)
            }
            other => return Err(NbtError::UnknownTag(other)),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Builds NBT bytes for tests: a root compound with the given (already encoded) children.
    pub(crate) struct Builder(Vec<u8>);

    impl Builder {
        pub(crate) fn root() -> Self {
            Self(vec![10, 0, 0])
        }
        fn name(&mut self, kind: u8, name: &str) {
            self.0.push(kind);
            self.0.extend((name.len() as u16).to_be_bytes());
            self.0.extend(name.as_bytes());
        }
        pub(crate) fn string(mut self, name: &str, value: &str) -> Self {
            self.name(8, name);
            self.0.extend((value.len() as u16).to_be_bytes());
            self.0.extend(value.as_bytes());
            self
        }
        pub(crate) fn long(mut self, name: &str, value: i64) -> Self {
            self.name(4, name);
            self.0.extend(value.to_be_bytes());
            self
        }
        pub(crate) fn int(mut self, name: &str, value: i32) -> Self {
            self.name(3, name);
            self.0.extend(value.to_be_bytes());
            self
        }
        pub(crate) fn byte(mut self, name: &str, value: i8) -> Self {
            self.name(1, name);
            self.0.push(value as u8);
            self
        }
        pub(crate) fn open(mut self, name: &str) -> Self {
            self.name(10, name);
            self
        }
        pub(crate) fn close(mut self) -> Self {
            self.0.push(0);
            self
        }
        pub(crate) fn gzipped(self) -> Vec<u8> {
            let mut bytes = self.close().0;
            bytes.push(0);
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(&bytes).unwrap();
            encoder.finish().unwrap()
        }
    }

    #[test]
    fn reads_gzipped_compounds_and_numbers() {
        let bytes = Builder::root()
            .open("Data")
            .string("LevelName", "Skyblock")
            .long("LastPlayed", 1_794_760_228_899)
            .int("GameType", 1)
            .open("Version")
            .string("Name", "26.3")
            .close()
            .close()
            .gzipped();
        let root = parse(&bytes).unwrap();
        assert_eq!(root.at(&["Data", "LevelName"]).and_then(Tag::as_str), Some("Skyblock"));
        assert_eq!(root.at(&["Data", "LastPlayed"]).and_then(Tag::as_i64), Some(1_794_760_228_899));
        assert_eq!(root.at(&["Data", "GameType"]).and_then(Tag::as_i64), Some(1));
        assert_eq!(root.at(&["Data", "Version", "Name"]).and_then(Tag::as_str), Some("26.3"));
    }

    #[test]
    fn refuses_damaged_files() {
        assert!(parse(&[10, 0, 0, 8, 0, 4, b'N']).is_err());
        // A list claiming two billion items in a few bytes.
        assert!(parse(&[10, 0, 0, 9, 0, 1, b'L', 3, 0x7f, 0xff, 0xff, 0xff]).is_err());
        assert!(parse(&[10, 0, 0, 99, 0, 0]).is_err());
    }
}
