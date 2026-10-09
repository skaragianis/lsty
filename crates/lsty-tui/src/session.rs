//! What the screen remembers between launches: the history of the `:`
//! line and of the search, walked with the history keys, and the folds
//! of the trees.

use lsty_core::keys::Action;

use crate::line::LineKind;
use crate::messages::Tone;
use crate::model::Model;
use crate::sidebar::{CONTEXTS, PROJECTS};
use crate::store::{Folds, Saved, Store};

/// The most lines a history keeps.
const LIMIT: usize = 200;

/// A history, newest last.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lines(Vec<String>);

impl Lines {
    /// The history after `line` was kept: an older equal line moves to
    /// the end, and only the newest entries stay.
    pub(crate) fn with(&mut self, line: &str) {
        self.0.retain(|old| old != line);
        self.0.push(line.to_owned());
        let extra = self.0.len().saturating_sub(LIMIT);
        self.0.drain(..extra);
    }

    /// `saved` with repeats dropped and only the newest kept.
    fn normalised(saved: &[String]) -> Self {
        let mut kept = Self::default();
        for line in saved {
            kept.with(line);
        }
        kept
    }
}

/// A walk through the entries that start with what was typed.
#[derive(Debug)]
pub(crate) struct Recalled {
    typed: String,
    entries: Vec<String>,
    at: usize,
}

impl Recalled {
    fn text(&self) -> &str {
        self.entries
            .get(self.at)
            .map_or(&self.typed, String::as_str)
    }
}

/// The remembered state of a session, and the walk in progress.
#[derive(Debug, Default)]
pub(crate) struct Session {
    store: Store,
    /// The lines run on the `:` line.
    command: Lines,
    /// The searches that were kept.
    search: Lines,
    recall: Option<Recalled>,
    /// The text of the search prompt is the kept search, not typed.
    prefilled: bool,
    /// A failed save was already shown.
    failed: bool,
}

impl Session {
    /// The session over `store`, with the saved histories read.
    pub(crate) fn new(store: Store) -> Self {
        let saved = store.history();
        Self {
            command: Lines::normalised(&saved.command),
            search: Lines::normalised(&saved.search),
            store,
            ..Self::default()
        }
    }

    /// Ends the walk through the history, as when text is typed.
    pub(crate) fn typed(&mut self) {
        self.recall = None;
        self.prefilled = false;
    }

    /// The search prompt opened with the kept search in it.
    pub(crate) fn prefill(&mut self, filled: bool) {
        self.prefilled = filled;
    }
}

impl Model {
    /// Folds the nodes that were folded when the files were last used.
    pub(crate) fn start_folds(&mut self) {
        let Some(key) = self.fold_key() else {
            return;
        };
        let saved = self.session.store.folds(&key);
        self.panels[PROJECTS].set_folded(&saved.projects);
        self.panels[CONTEXTS].set_folded(&saved.contexts);
    }

    /// The todo file as the folds are filed under it; none when its name
    /// is not text.
    fn fold_key(&self) -> Option<String> {
        let todo = &self.snap.found().todo;
        let resolved =
            std::fs::canonicalize(todo).unwrap_or_else(|_| todo.clone());
        resolved.into_os_string().into_string().ok()
    }

    /// Writes the folded names that still exist.
    pub(crate) fn keep_folds(&mut self) {
        let Some(key) = self.fold_key() else {
            return;
        };
        let folds = Folds {
            projects: self.panels[PROJECTS].folded_names(),
            contexts: self.panels[CONTEXTS].folded_names(),
        };
        let result = self.session.store.save_folds(&key, folds);
        self.persisted(result);
    }

    /// Shows why a save failed, once, unless a message is up.
    fn persisted(&mut self, result: Result<(), String>) {
        let Err(why) = result else {
            return;
        };
        if self.session.failed || self.message.is_some() {
            return;
        }
        self.session.failed = true;
        self.tone = Tone::Alert;
        self.message = Some(format!("can't save session state: {why}"));
    }

    /// Keeps a line run on the `:` line.
    pub(crate) fn remember_command(&mut self, line: &str) {
        self.remember(line, |s| &mut s.command);
    }

    /// Keeps a search.
    pub(crate) fn remember_search(&mut self, line: &str) {
        self.remember(line, |s| &mut s.search);
    }

    fn remember(&mut self, line: &str, pick: fn(&mut Session) -> &mut Lines) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        pick(&mut self.session).with(line);
        let saved = Saved {
            command: self.session.command.0.clone(),
            search: self.session.search.0.clone(),
        };
        let result = self.session.store.save_history(&saved);
        self.persisted(result);
    }

    /// `history_prev` and `history_next`: puts an older or newer line
    /// of the open prompt's history in the text.
    pub(crate) fn recall_step(&mut self, action: Action) {
        let Some(line) = &self.line else {
            return;
        };
        let typed = line.field.text();
        let history = match line.kind {
            LineKind::Search => &self.session.search,
            LineKind::Command => &self.session.command,
            LineKind::Items(_) => return,
        };
        let prefix = if self.session.prefilled { "" } else { &typed };
        let walk = self.session.recall.get_or_insert_with(|| {
            let entries: Vec<String> = history
                .0
                .iter()
                .filter(|e| e.starts_with(prefix))
                .cloned()
                .collect();
            Recalled {
                typed: typed.clone(),
                at: entries.len(),
                entries,
            }
        });
        if action == Action::HistoryPrev {
            let Some(older) = walk.at.checked_sub(1) else {
                return;
            };
            walk.at = older;
        } else if walk.at < walk.entries.len() {
            walk.at += 1;
        } else {
            return;
        }
        let text = walk.text().to_owned();
        if let Some(line) = &mut self.line {
            line.field = crate::field::Field::new(&text);
        }
    }
}
