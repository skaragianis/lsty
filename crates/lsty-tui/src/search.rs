//! The text line of the status line: the search, whose list follows the
//! text as it changes, and the prompts that share its keys.

use lsty_core::{
    errs::Error,
    keys::{Action, Fed, Outcome},
    query::{Query, state_words},
    todotxt::Kind,
};

use crate::{
    connective::{Conn, Term},
    field::Field,
    line::{Line, LineKind},
    model::Model,
    msg::Cmd,
    panel::Panel,
    sidebar::{CONTEXTS, PROJECTS},
    text::width_of,
};

/// What a search does to the view.
#[derive(Default)]
pub(crate) struct Split {
    /// The projects and the contexts the search puts in place of the
    /// selection of their panel; `None` leaves the panel as it is.
    moved: [Option<Vec<Term>>; 2],
    /// The terms that stay in the search, as written, with their queries.
    pub(crate) stays: Vec<(String, Query)>,
}

impl Split {
    /// The projects or contexts that filter the list for panel `i` of
    /// `panels`: those the search moved there, or those selected.
    pub(crate) fn active<'a>(
        &'a self,
        panels: &'a [Panel],
        i: usize,
    ) -> &'a [Term] {
        let moved = self.moved.get(i).and_then(Option::as_deref);
        moved.unwrap_or(&panels[i].active)
    }

    /// The terms that stay, as the text of a search.
    fn text(self) -> String {
        let terms: Vec<String> =
            self.stays.into_iter().map(|(t, _)| t).collect();
        terms.join(" ")
    }
}

impl Model {
    /// `search`: opens the prompt with the kept search filled in.
    pub(crate) fn open_search(&mut self) {
        let kind = LineKind::Search(self.search.clone());
        self.line = Some(Line::new(kind, &self.search));
        self.session.prefill(!self.search.is_empty());
    }

    /// Drops the kept search and rebuilds the list. With none kept it
    /// does nothing.
    pub(crate) fn clear_search(&mut self) {
        if !self.search.is_empty() {
            self.search.clear();
            self.refresh();
        }
    }

    /// Closes the text line, whichever it is.
    pub(crate) fn close_input(&mut self) {
        if self.choosing.is_none() {
            self.end_preview();
        }
        self.line = None;
        self.session.typed();
        self.reset_keys();
    }

    /// Drops term `at` of the kept search, which chip clicks come to.
    pub(crate) fn clear_search_term(&mut self, at: usize) {
        let mut split = self.searched();
        if at < split.stays.len() {
            split.stays.remove(at);
            self.search = split.text();
            self.refresh();
        }
    }

    /// What the search does now. While it is typed, the last text that
    /// parsed moves its projects and contexts into their panels; the
    /// kept search only stays.
    pub(crate) fn searched(&self) -> Split {
        let split = match &self.line {
            Some(Line {
                kind: LineKind::Search(parsed),
                ..
            }) => self.split(parsed, true),
            _ => self.split(&self.search, false),
        };
        split.unwrap_or_default()
    }

    /// The terms of `text`; when `moving`, those that [`Model::take`]
    /// moves into a panel do not stay.
    fn split(&self, text: &str, moving: bool) -> Result<Split, Error> {
        let mut split = Split::default();
        for (written, query) in Query::parse_terms(text)? {
            if !(moving && self.take(&query, &mut split.moved)) {
                split.stays.push((written.to_owned(), query));
            }
        }
        Ok(split)
    }

    /// Moves a term of only projects, or only contexts, into `moved`, with
    /// the connectives it was written with, when each of them has an
    /// entry in its panel and is not in `moved` yet. Whether it did.
    fn take(&self, query: &Query, moved: &mut [Option<Vec<Term>>; 2]) -> bool {
        let kinds = [(PROJECTS, Kind::Project), (CONTEXTS, Kind::Context)];
        let found = kinds.into_iter().find_map(|(i, kind)| {
            query
                .names_of(kind)
                .map(|(negated, names)| (i, negated, names))
        });
        let Some((i, negated, names)) = found else {
            return false;
        };
        let taken = moved[i].as_deref().unwrap_or_default();
        let fresh = names.iter().enumerate().all(|(j, &name)| {
            self.panels[i].has(name)
                && !names[..j].contains(&name)
                && !taken.iter().any(|t| t.name == name)
        });
        if !fresh {
            return false;
        }
        let terms = moved[i].get_or_insert_default();
        for (j, name) in names.into_iter().enumerate() {
            let conn = match (j, negated) {
                (0, false) => Conn::And,
                (0, true) => Conn::Not,
                _ => Conn::Or,
            };
            let name = name.to_vec();
            terms.push(Term { name, conn });
        }
        true
    }

    /// `Enter` on the search: its projects and contexts move into their
    /// panels and the other terms are kept. Text that does not parse
    /// leaves the prompt open.
    fn keep_search(&mut self) {
        let typed = self
            .line
            .as_ref()
            .map(|l| l.field.text())
            .unwrap_or_default();
        let Ok(mut split) = self.split(&typed, true) else {
            return;
        };
        self.remember_search(&typed);
        let moved = std::mem::take(&mut split.moved);
        for (panel, moved) in self.panels.iter_mut().zip(moved) {
            if let Some(moved) = moved {
                panel.active = moved;
            }
        }
        self.search = split.text();
        self.close_input();
        self.rebuild_panels();
        self.scroll_panels();
    }

    /// Takes the keys that the command table made of a stroke while the
    /// text line is open, and narrows the list to the new text when it
    /// is the search.
    pub(crate) fn prompt_keys(&mut self, fed: Fed) -> Vec<Cmd> {
        let searching = self
            .line
            .as_ref()
            .is_some_and(|l| matches!(l.kind, LineKind::Search(_)));
        let action = self.prompt_action_of(fed.outcome);
        let completing = matches!(
            action,
            Some(Action::CompleteNext | Action::CompletePrev)
        );
        if !completing && let Some(line) = &mut self.line {
            line.comp = None;
            self.preview_completed();
        }
        if !fed.text.is_empty() {
            self.session.typed();
        }
        if let Some(line) = &mut self.line {
            for stroke in fed.text {
                line.field.typed(stroke);
            }
        }
        let mut cmds = Vec::new();
        if let Some(action) = action {
            cmds = self.prompt_action(action);
        }
        if let Some(Line {
            kind: LineKind::Search(parsed),
            field,
            ..
        }) = &mut self.line
        {
            let text = field.text();
            if Query::parse(&text).is_ok() {
                *parsed = text;
            }
        }
        if searching {
            self.reshow();
        }
        cmds
    }

    /// The action a key has on the text line; while candidates are
    /// shown, the arrows move through them.
    fn prompt_action_of(&self, outcome: Outcome) -> Option<Action> {
        let Outcome::Matched { action, .. } = outcome else {
            return None;
        };
        let Some(comp) = self.line.as_ref().and_then(|l| l.comp.as_ref())
        else {
            return Some(action);
        };
        let sideways =
            matches!(action, Action::CursorLeft | Action::CursorRight);
        Some(if sideways && !comp.stepped() {
            action
        } else {
            menu(action)
        })
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the prompt takes only the editing actions"
    )]
    fn prompt_action(&mut self, action: Action) -> Vec<Cmd> {
        if !matches!(action, Action::HistoryPrev | Action::HistoryNext) {
            self.session.typed();
        }
        match action {
            Action::HistoryPrev | Action::HistoryNext => {
                self.recall_step(action);
            }
            Action::Execute => match self.line.as_ref().map(|l| &l.kind) {
                Some(LineKind::Command) => return self.execute_line(),
                Some(LineKind::Tasks(_)) => self.execute_prompt(),
                _ => self.keep_search(),
            },
            Action::Cancel => self.close_input(),
            Action::CompleteNext | Action::CompletePrev => {
                self.complete(action);
            }
            _ => {
                if let Some(line) = &mut self.line {
                    line.field.act(action);
                }
            }
        }
        Vec::new()
    }

    /// What completes `word` in the search: the projects or the
    /// contexts for a word with a sigil, else the states.
    pub(crate) fn search_pool(&mut self, word: &str) -> Vec<String> {
        if word.starts_with(['+', '@']) {
            return self.name_pool(word);
        }
        state_words().map(|w| format!("is:{w}")).collect()
    }

    /// Where the text cursor is on the screen, when the field of
    /// INSERT mode or the prompt is open.
    pub(crate) fn cursor_at(&self) -> Option<(u16, u16)> {
        if self.edit.is_some() {
            return self.field_cursor();
        }
        if self.typing_help() {
            return self.help_cursor();
        }
        if self.typing_theme() {
            return self.theme_cursor();
        }
        let line = self.line.as_ref()?;
        let mode = width_of(self.mode().as_str()) + 2;
        let x = mode + 1 + width_of(line.kind.label()) + line.field.cells();
        let y = self.height.checked_sub(1)?;
        Some((u16::try_from(x).ok()?.min(self.width.saturating_sub(1)), y))
    }
}

/// What a key does while candidates are shown: the arrows move through
/// them (`Left` and `Right` only once a candidate is in the text).
#[expect(clippy::wildcard_enum_match_arm, reason = "only the arrows change")]
const fn menu(action: Action) -> Action {
    match action {
        Action::CursorRight | Action::HistoryNext => Action::CompleteNext,
        Action::CursorLeft | Action::HistoryPrev => Action::CompletePrev,
        other => other,
    }
}

/// Where the alternative under the cursor of the search starts and
/// ends: within the word, after a leading `-` and the last `|` before
/// the cursor, up to the next `|`.
pub(crate) fn alternative(field: &Field) -> (usize, usize) {
    let (from, to) = field.word_span();
    let word = field.text.get(from..to).unwrap_or_default();
    let at = field.cursor - from;
    let (before, after) = word.split_at(at);
    let start = before.iter().rposition(|&c| c == '|').map_or_else(
        || usize::from(word.first() == Some(&'-')).min(at),
        |i| i + 1,
    );
    let end = after
        .iter()
        .position(|&c| c == '|')
        .map_or(word.len(), |i| at + i);
    (from + start, from + end)
}
