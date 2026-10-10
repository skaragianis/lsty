//! What the popup offers for a word: names, keys, the open items for
//! `dep:` and the files for `file:`. The names and the open items are
//! collected once for each reading of the files, not for each key.

use std::{
    borrow::Cow, cell::OnceCell, fs, os::unix::ffi::OsStrExt, path::Path,
};

use lsty_core::{
    case::lower,
    dates::Date,
    errs::{self, Nr, Source},
    list::{Item, ItemId, Snapshot},
    ops::Seen,
    query::is_upper,
    todotxt::{FileLink, Kind, Line},
};

use crate::{
    complete::{Candidate, Put},
    model::Model,
    prompt::{KNOWN_KEYS, matching},
    reldate::{Today, date_cell},
};

/// The fewest characters of a word that complete as a key.
const MIN_KEY_WORD: usize = 2;
/// What separates the parts of the label of an item.
const GAP: &str = "  ";
/// The keys the popup offers besides those of the tag prompt.
const MORE_KEYS: [&str; 2] = ["started:", "owner:"];

/// An open item of the todo file, as `dep:` offers it.
#[derive(Debug)]
struct Open {
    id: ItemId,
    nr: Nr,
    /// The id the line has.
    given: Option<String>,
    text: String,
    lower: String,
    label: String,
}

/// What the candidates are made of, for one reading and one day.
#[derive(Debug)]
pub(crate) struct Sources {
    serial: u32,
    today: Date,
    projects: Vec<String>,
    contexts: Vec<String>,
    open: Vec<Open>,
    /// The lines of the todo file, copied when a grant first needs
    /// them and shared by every grant of the reading.
    seen: OnceCell<Seen>,
}

impl Sources {
    fn of(snap: &Snapshot, today: Today) -> Self {
        let mut projects = Vec::new();
        let mut contexts = Vec::new();
        let mut open = Vec::new();
        for item in snap.items() {
            let line = snap.line(item);
            projects.extend(tokens_of(line, Kind::Project));
            contexts.extend(tokens_of(line, Kind::Context));
            if item.source() == Source::Todo && item.is_open() {
                open.push(open_item(snap, item, line, today));
            }
        }
        Self {
            serial: snap.serial(),
            today: today.date,
            projects: names(projects),
            contexts: names(contexts),
            open,
            seen: OnceCell::new(),
        }
    }
}

/// The tokens of `kind` of the line, as written.
fn tokens_of(line: &Line, kind: Kind) -> impl Iterator<Item = String> {
    line.tokens()
        .filter(move |t| t.kind == kind)
        .map(|t| errs::lossy(t.text))
}

/// The names in byte order, each once.
fn names(mut all: Vec<String>) -> Vec<String> {
    all.sort();
    all.dedup();
    all
}

fn open_item(snap: &Snapshot, item: &Item, line: &Line, today: Today) -> Open {
    let text = errs::lossy(&line.text());
    let tags: Vec<String> = line
        .tokens()
        .filter(|t| matches!(t.kind, Kind::Project | Kind::Context))
        .map(|t| errs::lossy(t.text))
        .collect();
    let due = date_cell(item, line, today).text;
    let due = due.strip_prefix("due ").unwrap_or(&due);
    let parts = [item.nr().to_string(), text.clone(), tags.join(" ")];
    let label = parts
        .iter()
        .map(String::as_str)
        .chain([due])
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(GAP);
    Open {
        id: item.item_id(),
        nr: item.nr(),
        given: snap.id_of(item).map(errs::lossy),
        lower: lower(&text),
        text,
        label,
    }
}

impl Model {
    /// Collects the names and the open items again when the files or the
    /// day changed.
    fn refresh_sources(&mut self) {
        let today = self.context().today;
        let fresh = self.sources.as_ref().is_some_and(|s| {
            s.serial == self.snap.serial() && s.today == today.date
        });
        if !fresh {
            self.sources = Some(Sources::of(&self.snap, today));
        }
    }

    /// The lines of the todo file of the reading shown: one copy for
    /// all the grants made on it.
    pub(crate) fn seen_lines(&mut self) -> Seen {
        self.refresh_sources();
        let snap = &self.snap;
        self.sources.as_ref().map_or_else(
            || Seen::of(snap),
            |s| s.seen.get_or_init(|| Seen::of(snap)).clone(),
        )
    }

    /// The candidates for the word that ends at the cursor.
    pub(crate) fn candidates_for(&mut self, word: &str) -> Vec<Candidate> {
        if let Some(typed) = word.strip_prefix("dep:") {
            return self.dep_candidates(typed);
        }
        if let Some(typed) = word.strip_prefix("file:") {
            return self.file_candidates(typed);
        }
        if word.starts_with(['+', '@']) {
            return self.name_candidates(word);
        }
        if word.chars().count() >= MIN_KEY_WORD && !word.contains(':') {
            return key_candidates(word);
        }
        Vec::new()
    }

    /// The projects or the contexts that match `word`, sigil and all:
    /// those that start with it, then those with a part that does.
    fn name_candidates(&mut self, word: &str) -> Vec<Candidate> {
        matching(self.name_pool(word), word)
            .into_iter()
            .map(Candidate::plain)
            .collect()
    }

    /// The projects (for a word starting with `+`) or the contexts of
    /// both files, as written, each once.
    pub(crate) fn name_pool(&mut self, word: &str) -> Vec<String> {
        self.refresh_sources();
        self.sources.as_ref().map_or_else(Vec::new, |s| {
            if word.starts_with('+') {
                &s.projects
            } else {
                &s.contexts
            }
            .clone()
        })
    }

    /// The open items whose text holds `typed`, as far as they can be
    /// waited for by the item being edited.
    fn dep_candidates(&mut self, typed: &str) -> Vec<Candidate> {
        self.refresh_sources();
        let Some(sources) = &self.sources else {
            return Vec::new();
        };
        let Some(draft) = &self.edit else {
            return Vec::new();
        };
        let folded = !typed.chars().any(is_upper);
        let waits = self.waits_for();
        let me = self.edited_item();
        let waiting =
            me.map(|me| self.snap.waiting_on(me)).unwrap_or_default();
        sources
            .open
            .iter()
            .filter(|o| {
                let text = if folded { &o.lower } else { &o.text };
                text.contains(typed)
            })
            .filter_map(|o| {
                if waiting.get(o.id.index()) == Some(&true) {
                    return None;
                }
                let id = o.given.clone().or_else(|| {
                    let grant =
                        draft.comp.grants.iter().find(|g| g.nr() == o.nr);
                    grant.map(|g| g.id().to_string())
                });
                if id
                    .as_ref()
                    .is_some_and(|id| waits.iter().any(|w| w == id.as_bytes()))
                {
                    return None;
                }
                let put = match id {
                    Some(id) => Put::Text(format!("dep:{id}")),
                    None => Put::NeedsId(o.id),
                };
                Some(Candidate {
                    label: o.label.clone(),
                    put,
                })
            })
            .collect()
    }

    /// The item of the draft; none for a new item or one that is gone.
    fn edited_item(&self) -> Option<ItemId> {
        let draft = self.edit.as_ref()?;
        if !draft.replacing() {
            return None;
        }
        let (source, nr) = draft.place()?;
        let item = self.snap.at(source, nr)?;
        Some(item.item_id())
    }

    /// The ids the draft already waits for, leaving out the word being
    /// completed.
    fn waits_for(&self) -> Vec<Vec<u8>> {
        let Some(draft) = &self.edit else {
            return Vec::new();
        };
        let mut text: Vec<char> = draft.field.text.clone();
        if let Some(p) = &draft.comp.popup {
            text.drain(p.from.min(text.len())..p.to.min(text.len()));
        }
        let text: String = text.into_iter().collect();
        Line::parse(text)
            .tags()
            .deps
            .iter()
            .map(|d| d.to_vec())
            .collect()
    }

    /// The entries of the folder that `typed` names, whose names start
    /// with the last part of `typed`.
    fn file_candidates(&self, typed: &str) -> Vec<Candidate> {
        let decoded = FileLink::decode(typed.as_bytes());
        let cut = decoded
            .iter()
            .rposition(|b| *b == b'/')
            .map_or(0, |i| i + 1);
        let (folder, last) = decoded.split_at(cut);
        let link = FileLink {
            raw: &[],
            decoded: Cow::Borrowed(folder),
        };
        let todo = &self.snap.found().todo;
        let dir = todo.parent().unwrap_or_else(|| Path::new(""));
        let home = self.deps.home.as_deref().unwrap_or_else(|| Path::new(""));
        let dir = link.resolve(dir, home);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<(Vec<u8>, bool)> = entries
            .filter_map(Result::ok)
            .filter(|e| offered_name(e.file_name().as_bytes(), last))
            .map(|e| (e.file_name().as_bytes().to_vec(), is_folder(&e)))
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|(name, folder_mark)| {
                file_candidate(folder, name, folder_mark)
            })
            .collect()
    }
}

/// The candidate for an entry named `name` of the folder `folder`.
fn file_candidate(
    folder: &[u8],
    mut name: Vec<u8>,
    is_dir: bool,
) -> Candidate {
    if is_dir {
        name.push(b'/');
    }
    let path = [folder, &name].concat();
    let encoded = FileLink::encode(&path);
    Candidate {
        label: errs::lossy(&name),
        put: Put::Text(format!("file:{}", errs::lossy(&encoded))),
    }
}

/// Whether the name starts with the typed part, and is not hidden unless
/// the typed part starts with a dot too.
fn offered_name(name: &[u8], last: &[u8]) -> bool {
    name.starts_with(last)
        && (!name.starts_with(b".") || last.starts_with(b"."))
}

/// Whether the entry is a folder, or a link to one.
fn is_folder(entry: &fs::DirEntry) -> bool {
    let Ok(kind) = entry.file_type() else {
        return false;
    };
    if kind.is_symlink() {
        return fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
    }
    kind.is_dir()
}

/// The keys that start with `word`, whatever the case.
fn key_candidates(word: &str) -> Vec<Candidate> {
    let mut keys: Vec<&str> =
        KNOWN_KEYS.iter().chain(MORE_KEYS.iter()).copied().collect();
    keys.sort_unstable();
    let word = lower(word);
    keys.into_iter()
        .filter(|k| lower(k).starts_with(&word))
        .map(|k| Candidate::plain(k.to_owned()))
        .collect()
}
