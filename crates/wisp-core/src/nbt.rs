//! Just enough NBT, Minecraft's binary data format, to read level.dat and player stats files and
//! to read and write servers.dat.
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

/// A root tag as uncompressed NBT bytes (how servers.dat is stored).
pub fn to_bytes(root: &Tag) -> Vec<u8> {
    let mut out = vec![kind_of(root)];
    write_string(&mut out, "");
    write_payload(&mut out, root);
    out
}

fn kind_of(tag: &Tag) -> u8 {
    match tag {
        Tag::Byte(_) => 1,
        Tag::Short(_) => 2,
        Tag::Int(_) => 3,
        Tag::Long(_) => 4,
        Tag::Float(_) => 5,
        Tag::Double(_) => 6,
        Tag::ByteArray(_) => 7,
        Tag::String(_) => 8,
        Tag::List(_) => 9,
        Tag::Compound(_) => 10,
        Tag::IntArray(_) => 11,
        Tag::LongArray(_) => 12,
    }
}

fn write_length(out: &mut Vec<u8>, length: usize) {
    out.extend((length.min(i32::MAX as usize) as i32).to_be_bytes());
}

fn write_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(value) => out.push(*value as u8),
        Tag::Short(value) => out.extend(value.to_be_bytes()),
        Tag::Int(value) => out.extend(value.to_be_bytes()),
        Tag::Long(value) => out.extend(value.to_be_bytes()),
        Tag::Float(value) => out.extend(value.to_be_bytes()),
        Tag::Double(value) => out.extend(value.to_be_bytes()),
        Tag::ByteArray(items) => {
            write_length(out, items.len());
            out.extend(items.iter().map(|&byte| byte as u8));
        }
        Tag::String(text) => write_string(out, text),
        Tag::List(items) => {
            out.push(items.first().map_or(0, kind_of));
            write_length(out, items.len());
            for item in items {
                write_payload(out, item);
            }
        }
        Tag::Compound(children) => {
            // Sorted, so the same data always makes the same bytes.
            let mut names: Vec<&String> = children.keys().collect();
            names.sort();
            for name in names {
                let child = &children[name];
                out.push(kind_of(child));
                write_string(out, name);
                write_payload(out, child);
            }
            out.push(0);
        }
        Tag::IntArray(items) => {
            write_length(out, items.len());
            for item in items {
                out.extend(item.to_be_bytes());
            }
        }
        Tag::LongArray(items) => {
            write_length(out, items.len());
            for item in items {
                out.extend(item.to_be_bytes());
            }
        }
    }
}

/// Java's "modified UTF-8": NUL takes two bytes, and characters past U+FFFF are written as their
/// two UTF-16 halves, three bytes each. Cut at a character if it passes the 65,535-byte limit.
fn write_string(out: &mut Vec<u8>, text: &str) {
    let mut bytes = Vec::with_capacity(text.len());
    for character in text.chars() {
        let before = bytes.len();
        let mut units = [0u16; 2];
        for &unit in character.encode_utf16(&mut units).iter() {
            match unit {
                0x0001..=0x007f => bytes.push(unit as u8),
                0x0000 | 0x0080..=0x07ff => {
                    bytes.push(0xc0 | (unit >> 6) as u8);
                    bytes.push(0x80 | (unit & 0x3f) as u8);
                }
                _ => {
                    bytes.push(0xe0 | (unit >> 12) as u8);
                    bytes.push(0x80 | ((unit >> 6) & 0x3f) as u8);
                    bytes.push(0x80 | (unit & 0x3f) as u8);
                }
            }
        }
        if bytes.len() > u16::MAX as usize {
            bytes.truncate(before);
            break;
        }
    }
    out.extend((bytes.len() as u16).to_be_bytes());
    out.extend(bytes);
}

/// Reads modified UTF-8 (plain UTF-8 too); anything broken becomes U+FFFD.
fn read_modified_utf8(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let mut units = Vec::with_capacity(bytes.len());
    let mut at = 0;
    let continuation = |index: usize| bytes.get(index).map(|&byte| u32::from(byte & 0x3f));
    while at < bytes.len() {
        let lead = bytes[at];
        if lead < 0x80 {
            units.push(u16::from(lead));
            at += 1;
        } else if lead & 0xe0 == 0xc0 && let Some(second) = continuation(at + 1) {
            units.push(((u32::from(lead & 0x1f) << 6) | second) as u16);
            at += 2;
        } else if lead & 0xf0 == 0xe0
            && let (Some(second), Some(third)) = (continuation(at + 1), continuation(at + 2))
        {
            units.push(((u32::from(lead & 0x0f) << 12) | (second << 6) | third) as u16);
            at += 3;
        } else if lead & 0xf8 == 0xf0
            && let (Some(second), Some(third), Some(fourth)) = (continuation(at + 1), continuation(at + 2), continuation(at + 3))
        {
            let code = (u32::from(lead & 0x07) << 18) | (second << 12) | (third << 6) | fourth;
            let mut pair = [0u16; 2];
            units.extend_from_slice(char::from_u32(code).unwrap_or('\u{fffd}').encode_utf16(&mut pair));
            at += 4;
        } else {
            units.push(0xfffd);
            at += 1;
        }
    }
    String::from_utf16_lossy(&units)
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
        Ok(read_modified_utf8(self.take(length)?))
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
    fn writes_what_it_reads_including_odd_text() {
        let mut server = HashMap::new();
        server.insert("name".to_string(), Tag::String("Caf\u{e9} \u{1d11e} \u{0} end".into()));
        server.insert("ip".to_string(), Tag::String("play.example.net:25570".into()));
        server.insert("acceptTextures".to_string(), Tag::Byte(1));
        let mut root = HashMap::new();
        root.insert("servers".to_string(), Tag::List(vec![Tag::Compound(server)]));
        root.insert("empty".to_string(), Tag::List(Vec::new()));
        root.insert("numbers".to_string(), Tag::IntArray(vec![1, -2, 3]));
        let root = Tag::Compound(root);
        let bytes = to_bytes(&root);
        assert_eq!(parse(&bytes).unwrap(), root);
        // NUL is written as two bytes, never a raw zero inside a string.
        assert!(bytes.windows(2).any(|pair| pair == [0xc0, 0x80]));
    }

    #[test]
    fn refuses_damaged_files() {
        assert!(parse(&[10, 0, 0, 8, 0, 4, b'N']).is_err());
        // A list claiming two billion items in a few bytes.
        assert!(parse(&[10, 0, 0, 9, 0, 1, b'L', 3, 0x7f, 0xff, 0xff, 0xff]).is_err());
        assert!(parse(&[10, 0, 0, 99, 0, 0]).is_err());
    }
}
