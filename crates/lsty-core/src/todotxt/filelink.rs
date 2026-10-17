use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt as _,
    path::{Component, Path, PathBuf},
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
        let (base, rest) = if v.starts_with(b"/") {
            (Path::new("/"), v)
        } else if v == b"~" || v.starts_with(b"~/") {
            (home, v.get(1..).unwrap_or_default())
        } else {
            (todo_dir, v)
        };
        let mut path = base.as_os_str().to_owned();
        path.push("/");
        path.push(OsStr::from_bytes(rest));
        clean(Path::new(&path))
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
                out.push(hi * 16 + lo);
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

/// `path` with `.` and `..` removed lexically. A `..` with nothing before
/// it is dropped, so a relative `../a` becomes `a`: callers pass absolute
/// paths.
pub(crate) fn clean(path: &Path) -> PathBuf {
    let mut parts: Vec<OsString> = Vec::new();
    let mut root = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                root.push(c.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(n) => parts.push(n.to_os_string()),
        }
    }
    root.extend(parts);
    root
}
