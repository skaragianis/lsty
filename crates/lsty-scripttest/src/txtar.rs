//! The txtar archive format: a comment, then `-- name --` sections.

use std::io::Write as _;

/// A parsed archive.
#[derive(Debug, PartialEq, Eq)]
pub struct Archive {
    /// Everything before the first section: the script.
    pub comment: String,
    /// The sections, in order: name and contents.
    pub files: Vec<(String, String)>,
}

impl Archive {
    /// Parses `text`.
    pub fn parse(text: &str) -> Self {
        let bytes = ByteArchive::parse(text.as_bytes());
        // Text is cut at newlines and markers, so every part is text.
        let files = bytes
            .files
            .into_iter()
            .map(|(name, data)| (name, text_of(data)))
            .collect();
        Self {
            comment: bytes.comment,
            files,
        }
    }

    /// The archive as text.
    pub fn format(self) -> String {
        let files = self
            .files
            .into_iter()
            .map(|(name, data)| (name, data.into_bytes()))
            .collect();
        text_of(
            ByteArchive {
                comment: self.comment,
                files,
            }
            .format(),
        )
    }
}

fn text_of(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// An archive whose sections are bytes, for transcripts that hold data
/// that is not text.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ByteArchive {
    /// Everything before the first section: the script.
    pub comment: String,
    /// The sections, in order: name and contents.
    pub files: Vec<(String, Vec<u8>)>,
}

impl ByteArchive {
    /// Parses `bytes`. The comment is text; a marker line is text too.
    pub fn parse(bytes: &[u8]) -> Self {
        let mut archive = Self::default();
        let mut current: Option<(String, Vec<u8>)> = None;
        let mut comment = Vec::new();

        for line in bytes.split_inclusive(|&b| b == b'\n') {
            let name = std::str::from_utf8(line).ok().and_then(marker);
            if let Some(name) = name {
                archive.push(current.take());
                current = Some((name.to_owned(), Vec::new()));
                continue;
            }
            match &mut current {
                Some((_, data)) => data.extend_from_slice(line),
                None => comment.extend_from_slice(line),
            }
        }
        archive.push(current);
        archive.comment = String::from_utf8_lossy(&comment).into_owned();
        archive
    }

    fn push(&mut self, file: Option<(String, Vec<u8>)>) {
        if let Some((name, mut data)) = file {
            if !data.is_empty() && !data.ends_with(b"\n") {
                data.push(b'\n');
            }
            self.files.push((name, data));
        }
    }

    /// The archive as bytes.
    pub fn format(&self) -> Vec<u8> {
        let mut out = self.comment.clone().into_bytes();
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        for (name, data) in &self.files {
            writeln!(out, "-- {name} --").ok();
            out.extend_from_slice(data);
            if !data.is_empty() && !data.ends_with(b"\n") {
                out.push(b'\n');
            }
        }
        out
    }
}

/// The name of a `-- name --` line.
fn marker(line: &str) -> Option<&str> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let name = line.strip_prefix("-- ")?.strip_suffix(" --")?.trim();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format() {
        let text = "exec lsty 0\n\n-- a.txt --\none\n-- b/c --\n-- d --\nlast";
        let archive = Archive::parse(text);
        assert_eq!(archive.comment, "exec lsty 0\n\n");
        assert_eq!(
            archive.files,
            [
                ("a.txt".to_owned(), "one\n".to_owned()),
                ("b/c".to_owned(), String::new()),
                ("d".to_owned(), "last\n".to_owned()),
            ]
        );
        assert_eq!(archive.format(), format!("{text}\n"));
    }

    #[test]
    fn bytes_that_are_not_text_survive() {
        let mut text = b"frame f\n-- todo.txt --\ncaf".to_vec();
        text.extend_from_slice(&[0xe9, 0xff, b'\n']);
        let archive = ByteArchive::parse(&text);
        assert_eq!(archive.comment, "frame f\n");
        assert_eq!(archive.files[0].1, [b'c', b'a', b'f', 0xe9, 0xff, b'\n']);
        assert_eq!(archive.format(), text);
    }

    #[test]
    fn marker_needs_both_dashes() {
        assert_eq!(marker("-- x --\n"), Some("x"));
        assert_eq!(marker("-- x\n"), None);
        assert_eq!(marker("--  --\n"), None);
    }
}
