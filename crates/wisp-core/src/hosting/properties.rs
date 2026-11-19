//! server.properties, read and written the way Java's `Properties` does, keeping the file's
//! comments and order so the server's own notes stay where they were.

use std::path::Path;

pub const FILE: &str = "server.properties";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq)]
enum Line {
    /// A comment, a blank line, or anything else kept as it was.
    Other(String),
    /// A setting, with its value unescaped.
    Pair { key: String, value: String },
}

impl Properties {
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    return Line::Other(line.to_string());
                }
                let (key, value) = split_pair(trimmed);
                Line::Pair { key: unescape(key), value: unescape(value) }
            })
            .collect();
        Self { lines }
    }

    /// The server's settings file in `dir`, or nothing when there isn't one yet.
    pub fn read(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .map(|bytes| Self::parse(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|line| match line {
            Line::Pair { key: found, value } if found == key => Some(value.as_str()),
            _ => None,
        })
    }

    /// Changes a setting where it is, or adds it at the end.
    pub fn set(&mut self, key: &str, value: &str) {
        for line in self.lines.iter_mut().rev() {
            if let Line::Pair { key: found, value: current } = line
                && found == key
            {
                *current = value.to_string();
                return;
            }
        }
        self.lines.push(Line::Pair { key: key.to_string(), value: value.to_string() });
    }

    /// Every setting, in file order.
    pub fn pairs(&self) -> Vec<(String, String)> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Pair { key, value } => Some((key.clone(), value.clone())),
                Line::Other(_) => None,
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for line in &self.lines {
            match line {
                Line::Other(raw) => text.push_str(raw),
                Line::Pair { key, value } => {
                    text.push_str(&escape(key, true));
                    text.push('=');
                    text.push_str(&escape(value, false));
                }
            }
            text.push('\n');
        }
        text
    }

    /// Saves the file in `dir`, replacing it in one step.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let temp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&temp, self.to_text())?;
        std::fs::rename(temp, dir.join(FILE))
    }
}

/// Splits "key=value", "key: value" or "key value" at the first unescaped separator.
fn split_pair(line: &str) -> (&str, &str) {
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '=' | ':' => return (&line[..index], line[index + 1..].trim_start()),
            c if c.is_whitespace() => {
                let rest = line[index..].trim_start();
                let rest = rest.strip_prefix(['=', ':']).map_or(rest, str::trim_start);
                return (&line[..index], rest);
            }
            _ => {}
        }
    }
    (line, "")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut pending_high: Option<u16> = None;
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            break;
        };
        match next {
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'f' => out.push('\u{c}'),
            'u' => {
                let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                let Ok(unit) = u16::from_str_radix(&hex, 16) else {
                    continue;
                };
                // Characters past the first 65536 arrive as two halves (UTF-16 surrogates).
                if (0xD800..0xDC00).contains(&unit) {
                    pending_high = Some(unit);
                    continue;
                }
                match pending_high.take() {
                    Some(high) if (0xDC00..0xE000).contains(&unit) => {
                        if let Some(c) = char::decode_utf16([high, unit]).next().and_then(Result::ok) {
                            out.push(c);
                        }
                    }
                    _ => {
                        if let Some(c) = char::from_u32(unit.into()) {
                            out.push(c);
                        }
                    }
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Escapes text for the file: backslashes, line breaks and anything outside ASCII (as \uXXXX, which
/// every Minecraft version reads, whatever it thinks the file's encoding is).
fn escape(text: &str, key: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, c) in text.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{c}' => out.push_str("\\f"),
            '=' | ':' | ' ' if key => {
                out.push('\\');
                out.push(c);
            }
            ' ' if index == 0 => out.push_str("\\ "),
            '#' | '!' if key && index == 0 => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04X}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_like_java_keeping_comments() {
        let text = "#Minecraft server properties\n#Thu Nov 19 2026\nmotd=A \\u00A7cRed\\u00A7r server\nserver-port=25565\nlevel-seed=\nwhite-list = true\n\n";
        let mut properties = Properties::parse(text);
        assert_eq!(properties.get("motd"), Some("A \u{a7}cRed\u{a7}r server"));
        assert_eq!(properties.get("white-list"), Some("true"));
        assert_eq!(properties.get("level-seed"), Some(""));
        assert_eq!(properties.get("missing"), None);
        properties.set("server-port", "25570");
        properties.set("difficulty", "hard");
        let written = properties.to_text();
        assert!(written.starts_with("#Minecraft server properties\n#Thu Nov 19 2026\n"));
        assert!(written.contains("motd=A \\u00A7cRed\\u00A7r server\n"));
        assert!(written.contains("server-port=25570\n"));
        assert!(written.ends_with("difficulty=hard\n"));
        assert_eq!(Properties::parse(&written).pairs(), properties.pairs());
    }

    #[test]
    fn escapes_what_needs_it() {
        let mut properties = Properties::default();
        // Accents, and a character past the first 65536 (written as two halves).
        properties.set("motd", " Ghost\\light\nsecond line café 𐍈");
        let written = properties.to_text();
        assert_eq!(written, "motd=\\ Ghost\\\\light\\nsecond line caf\\u00E9 \\uD800\\uDF48\n");
        assert_eq!(Properties::parse(&written).get("motd"), Some(" Ghost\\light\nsecond line café 𐍈"));
        // Other separators Java allows.
        let other = Properties::parse("gamemode: creative\npvp false\nkey\\=with\\:odd=1");
        assert_eq!(other.get("gamemode"), Some("creative"));
        assert_eq!(other.get("pvp"), Some("false"));
        assert_eq!(other.get("key=with:odd"), Some("1"));
    }
}
