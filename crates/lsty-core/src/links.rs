//! An item's `file:` links as paths on disk, and how one is opened: text
//! in the editor, anything else with the system's opener, and never a
//! program.

use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata},
    io::{self, Read as _},
    os::unix::{ffi::OsStrExt as _, fs::PermissionsExt as _},
    path::{Path, PathBuf},
};

use crate::{
    case::lower,
    errs::{lossy, lossy_path},
    list::{Item, Snapshot},
};

/// How a path is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// It would run a program. The text says so.
    Refused(String),
    /// It is not there. The text says so.
    Missing(String),
    /// A text file, which the editor opens.
    Text,
    /// Anything else, which the system's opener opens.
    Other,
}

/// How many bytes of a file say whether it is text.
const SNIFF_SIZE: usize = 8192;
/// [`SNIFF_SIZE`] as the limit of a reader.
const SNIFF_LIMIT: u64 = SNIFF_SIZE as u64;

/// The permission bits that let anyone run a file.
const EXEC_BITS: u32 = 0o111;

/// The extensions of files that launch a program, in lower case.
const LAUNCHER_EXTS: [&str; 15] = [
    ".desktop",
    ".app",
    ".command",
    ".appimage",
    ".jar",
    ".exe",
    ".bat",
    ".cmd",
    ".com",
    ".msi",
    ".scr",
    ".lnk",
    ".run",
    ".workflow",
    ".action",
];

/// What a program that was started says when it fails, in the words of
/// the error message.
pub type HookResult = Result<(), String>;

/// Says how `path` is opened.
pub fn classify(path: &Path) -> Kind {
    let missing =
        || Kind::Missing(format!("{} does not exist", lossy_path(path)));
    let target = match fs::canonicalize(path) {
        Ok(target) => target,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return missing(),
        Err(_) => path.to_path_buf(),
    };
    let info = match fs::metadata(&target) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return missing(),
        Err(_) => None,
    };
    let regular = info.as_ref().is_some_and(Metadata::is_file);
    let launches = has_launcher_ext(path) || has_launcher_ext(&target);
    if launches || (regular && is_executable(info.as_ref())) {
        return Kind::Refused(format!(
            "won't open executable: {}",
            lossy_path(path)
        ));
    }
    if regular && is_text(&target) {
        return Kind::Text;
    }
    Kind::Other
}

/// The extension of the last name of `path`, with its dot; the name
/// `.desktop` has the extension `.desktop`.
fn extension(path: &Path) -> &[u8] {
    let bytes = path.as_os_str().as_bytes();
    for (i, b) in bytes.iter().enumerate().rev() {
        match b {
            b'/' => return &[],
            b'.' => return &bytes[i..],
            _ => {}
        }
    }
    &[]
}

fn has_launcher_ext(path: &Path) -> bool {
    let ext = lower(&lossy(extension(path)));
    LAUNCHER_EXTS.contains(&ext.as_str())
}

fn is_executable(info: Option<&Metadata>) -> bool {
    info.is_some_and(|i| i.permissions().mode() & EXEC_BITS != 0)
}

/// Whether the start of the file is valid UTF-8 without a NUL byte. A
/// rune cut by the end of the sample is fine.
fn is_text(path: &Path) -> bool {
    let Ok(head) = read_head(path) else {
        return false;
    };
    let cut_off = head.len() == SNIFF_SIZE;
    let valid = match std::str::from_utf8(&head) {
        Ok(_) => true,
        Err(e) => cut_off && e.error_len().is_none(),
    };
    valid && !head.contains(&0)
}

fn read_head(path: &Path) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(SNIFF_SIZE);
    File::open(path)?.take(SNIFF_LIMIT).read_to_end(&mut head)?;
    Ok(head)
}

/// The program and arguments that edit `path`: `EDITOR` split at ASCII
/// white space, bytes untouched, or `vi` when it is unset or empty.
pub fn editor_command(editor: Option<&OsStr>, path: &Path) -> Vec<OsString> {
    let mut words = editor.map(split_words).unwrap_or_default();
    if words.is_empty() {
        words.push("vi".into());
    }
    words.push(path.as_os_str().to_owned());
    words
}

/// Splits a setting such as `EDITOR` or `PAGER` at runs of white space
/// (Unicode white space where the bytes are UTF-8), keeping every byte of
/// each word, invalid bytes included.
pub fn split_words(setting: &OsStr) -> Vec<OsString> {
    let mut words = Words::default();
    for chunk in setting.as_bytes().utf8_chunks() {
        chunk.valid().chars().for_each(|c| words.push_char(c));
        words.current.extend_from_slice(chunk.invalid());
    }
    words.end_word();
    words
        .done
        .iter()
        .map(|w| OsStr::from_bytes(w).to_owned())
        .collect()
}

/// The words found so far and the one being read.
#[derive(Default)]
struct Words {
    done: Vec<Vec<u8>>,
    current: Vec<u8>,
}

impl Words {
    fn push_char(&mut self, c: char) {
        if c.is_whitespace() {
            self.end_word();
            return;
        }
        let mut buf = [0; 4];
        self.current
            .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }

    fn end_word(&mut self) {
        if !self.current.is_empty() {
            self.done.push(std::mem::take(&mut self.current));
        }
    }
}

/// One `file:` link of an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedFile {
    /// The link as written.
    pub raw: Vec<u8>,
    /// The path it names: relative to the folder of the todo file, or
    /// under the home folder for `~/`.
    pub path: PathBuf,
}

/// The files `item` links to, in the order of its line. `home` is the
/// folder `~/` stands for.
pub fn linked_files(
    snap: &Snapshot,
    item: &Item,
    home: Option<&Path>,
) -> Vec<LinkedFile> {
    let todo = &snap.found().todo;
    let dir = todo.parent().unwrap_or_else(|| Path::new(""));
    let home = home.unwrap_or_else(|| Path::new(""));
    snap.tags(item)
        .files
        .iter()
        .map(|link| LinkedFile {
            raw: link.raw.to_vec(),
            path: link.resolve(dir, home),
        })
        .collect()
}

#[cfg(test)]
mod tests;
