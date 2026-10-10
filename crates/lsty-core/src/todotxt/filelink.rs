use std::{
    borrow::Cow,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

/// The value of a `file:` tag, as written and percent-decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLink<'a> {
    /// The value as written.
    pub raw: &'a [u8],
    /// The value with every `%XX` decoded.
    pub decoded: Cow<'a, [u8]>,
}

impl<'a> FileLink<'a> {
    /// The link for a `file:` value.
    pub(crate) fn new(raw: &'a [u8]) -> Self {
        Self {
            raw,
            decoded: Self::decode(raw),
        }
    }

    /// The absolute path of the link: a value starting with `/` is cleaned,
    /// `~` and `~/…` lie under `home`, and anything else under `todo_dir`.
    /// It never touches the disk.
    pub fn resolve(&self, todo_dir: &Path, home: &Path) -> PathBuf {
        let v: &[u8] = &self.decoded;
        let path = if v.starts_with(b"/") {
            clean(v)
        } else if v == b"~" || v.starts_with(b"~/") {
            join(home.as_os_str().as_bytes(), v.get(1..).unwrap_or_default())
        } else {
            join(todo_dir.as_os_str().as_bytes(), v)
        };
        PathBuf::from(OsStr::from_bytes(&path))
    }

    /// Decodes every `%XX` with two hexadecimal digits; any other `%` stays.
    pub fn decode(s: &[u8]) -> Cow<'_, [u8]> {
        if !s.contains(&b'%') {
            return Cow::Borrowed(s);
        }
        let mut out = Vec::with_capacity(s.len());
        let mut i = 0;
        while let Some(&b) = s.get(i) {
            if b == b'%'
                && let (Some(hi), Some(lo)) = (
                    s.get(i + 1).and_then(|b| hex(*b)),
                    s.get(i + 2).and_then(|b| hex(*b)),
                )
            {
                out.push(hi << 4 | lo);
                i += 3;
            } else {
                out.push(b);
                i += 1;
            }
        }
        Cow::Owned(out)
    }

    /// Writes `path` as a `file:` value that stays a valid tag: a space
    /// becomes `%20`, `%` becomes `%25` and a colon becomes `%3A`.
    pub fn encode(path: impl AsRef<[u8]>) -> Vec<u8> {
        let path = path.as_ref();
        let mut out = Vec::with_capacity(path.len());
        for &b in path {
            match b {
                b'%' => out.extend_from_slice(b"%25"),
                b' ' => out.extend_from_slice(b"%20"),
                b':' => out.extend_from_slice(b"%3A"),
                _ => out.push(b),
            }
        }
        out
    }
}

fn hex(b: u8) -> Option<u8> {
    char::from(b)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// Joins the non-empty parts with `/` and cleans the result; empty when
/// every part is empty.
fn join(a: &[u8], b: &[u8]) -> Vec<u8> {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Vec::new(),
        (true, false) => clean(b),
        (false, true) => clean(a),
        (false, false) => {
            let mut both = a.to_vec();
            both.push(b'/');
            both.extend_from_slice(b);
            clean(&both)
        }
    }
}

/// Lexical path cleaning: collapses `//`, drops `.` and resolves `..`.
fn clean(path: &[u8]) -> Vec<u8> {
    let rooted = path.starts_with(b"/");
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in path.split(|b| *b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => match parts.last() {
                Some(last) if *last != b".." => {
                    parts.pop();
                }
                _ if rooted => {}
                _ => parts.push(part),
            },
            _ => parts.push(part),
        }
    }
    let mut out = Vec::new();
    if rooted {
        out.push(b'/');
    }
    out.extend_from_slice(&parts.join(&b'/'));
    if out.is_empty() {
        out.push(b'.');
    }
    out
}
