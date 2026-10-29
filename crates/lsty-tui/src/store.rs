//! The files that keep what the screen remembers between launches: the
//! history of the `:` line and of the search, and the folded nodes of
//! the Projects and Contexts trees of each todo file.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, DirBuilder},
    io,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

use lsty_core::{
    errs::{io_text, lossy_path},
    fsio::{self, OsHooks},
};
use rustix::io::Errno;
use toml::de::{DeTable, DeValue};

const HISTORY_FILE: &str = "history.toml";
const FOLDS_FILE: &str = "folds.toml";
/// The mode of the state folder when it is created.
const FOLDER_MODE: u32 = 0o700;

/// The lines kept for the `:` line and for the search, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Saved {
    pub(crate) command: Vec<String>,
    pub(crate) search: Vec<String>,
}

/// The names of the folded nodes of one todo file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Folds {
    pub(crate) projects: Vec<String>,
    pub(crate) contexts: Vec<String>,
}

impl Folds {
    fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.contexts.is_empty()
    }
}

/// A folder of state files. Without a folder nothing is remembered.
#[derive(Clone, Debug, Default)]
pub(crate) struct Store {
    dir: Option<PathBuf>,
}

impl Store {
    /// The store that keeps its files in `dir`.
    pub(crate) const fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    /// The saved history; empty when the file is missing, an error
    /// naming the file when it cannot be read or does not parse.
    pub(crate) fn history(&self) -> Result<Saved, String> {
        self.read(HISTORY_FILE, parse_history)
    }

    /// Writes both histories.
    pub(crate) fn save_history(&self, saved: &Saved) -> Result<(), String> {
        let text = format!(
            "command = {}\nsearch = {}\n",
            array(&saved.command),
            array(&saved.search)
        );
        self.write(HISTORY_FILE, text.as_bytes())
    }

    fn all_folds(&self) -> Result<BTreeMap<String, Folds>, String> {
        self.read(FOLDS_FILE, parse_folds)
    }

    /// The folded names saved for the todo file `todo`; none when the
    /// file cannot be read.
    pub(crate) fn folds(&self, todo: &str) -> Folds {
        let mut all = self.all_folds().unwrap_or_default();
        all.remove(todo).unwrap_or_default()
    }

    /// Saves the folded names of `todo`, and leaves those of the other
    /// todo files as they are, and writes nothing when the file cannot
    /// be read. No folded names remove the entry.
    pub(crate) fn save_folds(
        &self,
        todo: &str,
        folds: Folds,
    ) -> Result<(), String> {
        let mut all = self.all_folds()?;
        if folds.is_empty() {
            all.remove(todo);
        } else {
            all.insert(todo.to_owned(), folds);
        }
        self.write(FOLDS_FILE, folds_text(&all).as_bytes())
    }

    /// The file `name` read by `parse`; the default when there is no
    /// folder or no file.
    fn read<T: Default>(
        &self,
        name: &str,
        parse: fn(&DeTable<'_>) -> Option<T>,
    ) -> Result<T, String> {
        let Some(dir) = &self.dir else {
            return Ok(T::default());
        };
        let path = dir.join(name);
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(reason(&path, &e)),
        };
        let shown = lossy_path(&path);
        let text = String::from_utf8(data)
            .map_err(|_| format!("{shown}: not UTF-8"))?;
        let table = DeTable::parse(&text)
            .map_err(|_| format!("{shown}: not valid TOML"))?;
        parse(table.get_ref())
            .ok_or_else(|| format!("{shown}: an entry has the wrong type"))
    }

    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        save(&dir.join(name), data)
    }
}

/// Writes `data` to `path` through a temporary file, after making the
/// folders above it that are missing. A failure names where it
/// happened.
pub(crate) fn save(path: &Path, data: &[u8]) -> Result<(), String> {
    let target = linked(path).map_err(|e| reason(path, &e))?;
    let path = target.as_path();
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        make_folders(dir).map_err(|(at, e)| reason(&at, &e))?;
    }
    fsio::write(path, data, &OsHooks).map_err(|e| {
        let cause = std::error::Error::source(&e)
            .and_then(|s| s.downcast_ref::<io::Error>());
        cause.map_or_else(
            || e.message().to_owned(),
            |cause| reason(path, cause),
        )
    })
}

/// How many symlinks [`linked`] follows before it gives up.
const MAX_LINKS: usize = 40;

/// Where a write to `path` lands: the end of its chain of symlinks, so
/// a link (a config file kept with other dotfiles) stays a link. A
/// dangling link gets its target made. A chain longer than
/// [`MAX_LINKS`] is `too many levels of symbolic links`.
fn linked(path: &Path) -> io::Result<PathBuf> {
    let mut at = path.to_path_buf();
    for _ in 0..=MAX_LINKS {
        let Ok(next) = fs::read_link(&at) else {
            return Ok(at);
        };
        at = at
            .parent()
            .map_or_else(|| next.clone(), |dir| dir.join(&next));
    }
    Err(io::Error::from(Errno::LOOP))
}

/// Makes `path` and the folders above it that are missing, each with the
/// folder mode. A failure names the folder it happened at; a file in
/// the way is `not a directory`.
fn make_folders(path: &Path) -> Result<(), (PathBuf, io::Error)> {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => return Ok(()),
        Ok(_) => {
            let e = io::Error::from(Errno::NOTDIR);
            return Err((path.to_path_buf(), e));
        }
        Err(_) => {}
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        make_folders(parent)?;
    }
    match DirBuilder::new().mode(FOLDER_MODE).create(path) {
        Ok(()) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(e) => Err((path.to_path_buf(), e)),
    }
}

pub(crate) fn reason(path: &Path, err: &io::Error) -> String {
    format!("{}: {}", lossy_path(path), io_text(err))
}

/// The list at `key`, empty when the key is missing, `None` when it is
/// no list of strings.
fn list_at(table: &DeTable<'_>, key: &str) -> Option<Vec<String>> {
    table.get(key).map_or(Some(Vec::new()), |value| {
        lsty_core::config::toml_strings(value.get_ref())
    })
}

fn parse_history(table: &DeTable<'_>) -> Option<Saved> {
    Some(Saved {
        command: list_at(table, "command")?,
        search: list_at(table, "search")?,
    })
}

fn parse_folds(table: &DeTable<'_>) -> Option<BTreeMap<String, Folds>> {
    let mut all = BTreeMap::new();
    for (key, value) in table {
        let DeValue::Table(one) = value.get_ref() else {
            return None;
        };
        let folds = Folds {
            projects: list_at(one, "projects")?,
            contexts: list_at(one, "contexts")?,
        };
        all.insert(key.get_ref().to_string(), folds);
    }
    Some(all)
}

fn folds_text(all: &BTreeMap<String, Folds>) -> String {
    let tables: Vec<String> = all
        .iter()
        .map(|(todo, folds)| {
            format!(
                "[{}]\n  projects = {}\n  contexts = {}\n",
                key(todo),
                array(&folds.projects),
                array(&folds.contexts)
            )
        })
        .collect();
    tables.join("\n")
}

fn array(values: &[String]) -> String {
    let quoted: Vec<String> = values.iter().map(|s| quoted(s)).collect();
    format!("[{}]", quoted.join(", "))
}

/// A table key: bare when it can be, else quoted.
fn key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { name.to_owned() } else { quoted(name) }
}

/// `text` as a TOML basic string.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' || c == '\u{7f}' => {
                write!(out, "\\u{:04x}", u32::from(c)).ok();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
