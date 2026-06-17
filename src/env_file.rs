use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::sys;

/// A parsed `.env`-style file that preserves its original lines so writes keep
/// comments, blank lines, and ordering intact.
pub struct EnvFile {
    pub path: String,
    pub exists: bool,
    lines: Vec<String>,
}

impl EnvFile {
    pub fn load(path: String) -> Self {
        match sys::read_file(&path) {
            Some(bytes) => {
                let content = String::from_utf8_lossy(&bytes);
                // Split keeping logical lines; trailing newline shouldn't create an
                // empty final entry we then re-append.
                let mut lines: Vec<String> =
                    content.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
                if matches!(lines.last(), Some(l) if l.is_empty()) {
                    lines.pop();
                }
                EnvFile { path, exists: true, lines }
            }
            None => EnvFile { path, exists: false, lines: Vec::new() },
        }
    }

    /// Returns the value for a key if the file defines it (last definition wins).
    pub fn get(&self, key: &str) -> Option<String> {
        let mut found = None;
        for line in &self.lines {
            if let Some((k, v)) = parse_line(line) {
                if k == key {
                    found = Some(v);
                }
            }
        }
        found
    }

    /// All keys defined in this file, in first-seen order.
    pub fn keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for line in &self.lines {
            if let Some((k, _)) = parse_line(line) {
                if !out.contains(&k) {
                    out.push(k);
                }
            }
        }
        out
    }

    /// Set (or insert) a key's value in memory.
    pub fn set(&mut self, key: &str, value: &str) {
        let formatted = format_assignment(key, value);
        let mut last_idx = None;
        for (i, line) in self.lines.iter().enumerate() {
            if let Some((k, _)) = parse_line(line) {
                if k == key {
                    last_idx = Some(i);
                }
            }
        }
        match last_idx {
            Some(i) => self.lines[i] = formatted,
            None => self.lines.push(formatted),
        }
    }

    /// Write the file back to disk. Returns false on failure.
    pub fn save(&mut self) -> bool {
        let mut content = self.lines.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        if sys::write_file(&self.path, content.as_bytes()) {
            self.exists = true;
            true
        } else {
            false
        }
    }
}

/// Parse a single line into (key, value) if it is an assignment. Returns None for
/// comments and blanks. Handles an optional `export ` prefix and quoted values.
fn parse_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let eq = trimmed.find('=')?;
    let key = trimmed[..eq].trim().to_string();
    if key.is_empty() || !key.chars().all(is_key_char) {
        return None;
    }
    let raw = trimmed[eq + 1..].trim();
    Some((key, unquote(raw)))
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn unquote(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if raw.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[raw.len() - 1] as char;
        if first == '"' && last == '"' {
            return unescape_double(&raw[1..raw.len() - 1]);
        }
        if first == '\'' && last == '\'' {
            return raw[1..raw.len() - 1].to_string();
        }
    }
    // Unquoted: drop trailing inline comment if separated by whitespace + '#'.
    if let Some(idx) = find_inline_comment(raw) {
        return raw[..idx].trim_end().to_string();
    }
    raw.to_string()
}

fn find_inline_comment(s: &str) -> Option<usize> {
    let mut prev_ws = false;
    for (i, c) in s.char_indices() {
        if c == '#' && (i == 0 || prev_ws) {
            return Some(i);
        }
        prev_ws = c.is_whitespace();
    }
    None
}

fn unescape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn format_assignment(key: &str, value: &str) -> String {
    format!("{}={}", key, format_value(value))
}

/// Render a value for writing: bare when safe, double-quoted+escaped otherwise.
fn format_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let safe = value.chars().all(|c| c.is_ascii_alphanumeric() || "._-/:@+,".contains(c));
    if safe {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp(name: &str) -> String {
        let mut p: PathBuf = std::env::temp_dir();
        p.push(format!("wenv_test_{}_{}", std::process::id(), name));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn parse_basic_quotes_and_comments() {
        let p = tmp("parse.env");
        fs::write(&p, "# comment\nexport A=1\nB=\nC=\"a b#c\"\nD='raw $x'\nE=plain # trailing\n").unwrap();
        let f = EnvFile::load(p.clone());
        assert_eq!(f.get("A").as_deref(), Some("1"));
        assert_eq!(f.get("B").as_deref(), Some(""));
        assert_eq!(f.get("C").as_deref(), Some("a b#c"));
        assert_eq!(f.get("D").as_deref(), Some("raw $x"));
        assert_eq!(f.get("E").as_deref(), Some("plain"));
        assert_eq!(f.keys(), vec!["A", "B", "C", "D", "E"]);
        fs::remove_file(p).ok();
    }

    #[test]
    fn set_preserves_structure_and_quotes_when_needed() {
        let p = tmp("write.env");
        fs::write(&p, "# header\nA=old\n\n# section\nB=keep\n").unwrap();
        let mut f = EnvFile::load(p.clone());
        f.set("A", "new value!"); // needs quoting (space + !)
        f.set("C", "added"); // appended
        assert!(f.save());
        let out = fs::read_to_string(&p).unwrap();
        assert_eq!(out, "# header\nA=\"new value!\"\n\n# section\nB=keep\nC=added\n");
        let f2 = EnvFile::load(p.clone());
        assert_eq!(f2.get("A").as_deref(), Some("new value!"));
        assert_eq!(f2.get("C").as_deref(), Some("added"));
        fs::remove_file(p).ok();
    }

    #[test]
    fn empty_value_written_bare() {
        let p = tmp("empty.env");
        let mut f = EnvFile::load(p.clone());
        f.set("TOKEN", "");
        assert!(f.save());
        assert_eq!(fs::read_to_string(&p).unwrap(), "TOKEN=\n");
        fs::remove_file(p).ok();
    }
}
