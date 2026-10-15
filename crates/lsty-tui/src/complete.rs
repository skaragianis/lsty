//! The completion popup of INSERT mode: it follows the word at the
//! text cursor, offers what can follow, and has its own keys.

use lsty_core::keys::{Action, Fed, Mode, Outcome as Pressed, Stroke};

use crate::{field::is_sep, model::Model};

/// How many candidates the popup shows at once.
pub(crate) const POPUP_ROWS: usize = 8;

/// What accepting a candidate puts in place of the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Put {
    /// This text.
    Text(String),
    /// `dep:` and the id of the task at this line, which gets one now.
    NeedsId(lsty_core::list::TaskId),
}

/// A line of the popup and what accepting it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) label: String,
    pub(crate) put: Put,
}

impl Candidate {
    /// A candidate that puts its own label.
    pub(crate) fn plain(text: String) -> Self {
        Self {
            put: Put::Text(text.clone()),
            label: text,
        }
    }
}

/// The candidates for the word from `from` to `to`, and which one is
/// selected.
#[derive(Debug)]
pub(crate) struct Popup {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) cands: Vec<Candidate>,
    pub(crate) at: usize,
    pub(crate) top: usize,
}

/// `at` moved `by` places (one at most) round a ring of `n`.
pub(crate) fn ring(at: usize, by: isize, n: usize) -> usize {
    (at + n).saturating_add_signed(by) % n.max(1)
}

impl Popup {
    /// Selects the candidate `by` places on, round the list.
    fn step(&mut self, by: isize) {
        self.at = ring(self.at, by, self.cands.len());
        self.top = self.top.max((self.at + 1).saturating_sub(POPUP_ROWS));
        self.top = self.top.min(self.at);
    }

    /// The first row that is shown when `rows` fit, and the candidates
    /// from there: the selection stays in view.
    pub(crate) fn shown(&self, rows: usize) -> (usize, &[Candidate]) {
        let top = self.top.max((self.at + 1).saturating_sub(rows));
        let top = top.min(self.at);
        let end = (top + rows).min(self.cands.len());
        (top, self.cands.get(top..end).unwrap_or_default())
    }
}

/// Whether two candidates insert the same text under the same label.
/// The task of a `dep:` candidate is not compared: it belongs to one
/// reading.
fn same_choice(a: &Candidate, b: &Candidate) -> bool {
    let put = match (&a.put, &b.put) {
        (Put::Text(x), Put::Text(y)) => x == y,
        (Put::NeedsId(_), Put::NeedsId(_)) => true,
        (Put::Text(_), Put::NeedsId(_)) | (Put::NeedsId(_), Put::Text(_)) => {
            false
        }
    };
    put && a.label == b.label
}

/// What a draft keeps of the popup.
#[derive(Debug, Default)]
pub(crate) struct Completion {
    /// The popup, while one is open.
    pub(crate) popup: Option<Popup>,
    /// Where the word being completed starts: set when its sigil is
    /// typed, kept while the text of that word is what changes.
    pub(crate) under: Option<usize>,
    /// The word that was closed with `close`: no popup until it changes.
    pub(crate) shut: String,
    /// The ids to give to the tasks the text waits for.
    pub(crate) grants: Vec<lsty_core::ops::Grant>,
}

/// Whether the action belongs to the popup's keys.
const fn is_popup_action(action: Action) -> bool {
    matches!(
        action,
        Action::Accept | Action::Close | Action::Next | Action::Prev
    )
}

impl Model {
    /// The open completion popup, if any.
    pub(crate) fn popup(&self) -> Option<&Popup> {
        self.edit.as_ref()?.comp.popup.as_ref()
    }

    /// The open completion popup, to change.
    fn popup_mut(&mut self) -> Option<&mut Popup> {
        self.edit.as_mut()?.comp.popup.as_mut()
    }

    /// The keys of the popup layered over those of INSERT mode while the
    /// popup is open, those of INSERT mode otherwise. Keys pending in
    /// the one that is not used move over to the one that is.
    fn edit_fed(&mut self, stroke: Stroke) -> Fed {
        let open = self.popup().is_some();
        let (using, other) = if open {
            (Mode::Completion, Mode::Insert)
        } else {
            (Mode::Insert, Mode::Completion)
        };
        let nothing = Fed {
            outcome: Pressed::NoMatch,
            text: Vec::new(),
        };
        let Some(mut spare) = self.matchers.remove(&other) else {
            return nothing;
        };
        let fed = self.matchers.get_mut(&using).map(|matcher| {
            let mut carried = matcher.take(&mut spare);
            let mut fed = matcher.feed(stroke);
            carried.append(&mut fed.text);
            fed.text = carried;
            fed
        });
        self.matchers.insert(other, spare);
        fed.unwrap_or(nothing)
    }

    /// Whether INSERT mode commits on `stroke` alone.
    fn commits(&self, stroke: Stroke) -> bool {
        let fed = self.deps.keys.matcher(Mode::Insert).feed(stroke);
        matches!(
            fed.outcome,
            Pressed::Matched {
                action: Action::Commit,
                ..
            }
        )
    }

    /// Takes a key press while the field is open.
    pub(crate) fn edit_press(&mut self, stroke: Stroke) {
        let fed = self.edit_fed(stroke);
        let own = matches!(
            fed.outcome,
            Pressed::Matched { action, .. } if is_popup_action(action)
        );
        let typed = !fed.text.is_empty();
        let len = self.edit.as_ref().map(|d| d.field.text.len());
        self.type_text(fed.text);
        if let Pressed::Matched { action, .. } = fed.outcome {
            // An accept that would change nothing, by a key that commits
            // in INSERT mode, closes the popup and gives it the key.
            if action == Action::Accept
                && self.accepts_nothing()
                && self.commits(stroke)
            {
                self.close_popup();
                return self.edit_press(stroke);
            }
            self.edit_action(action);
        }
        if self.edit.is_some() {
            if !own {
                let changed =
                    len != self.edit.as_ref().map(|d| d.field.text.len());
                self.follow_word(changed, typed);
            }
            self.scroll();
        }
    }

    /// Types strokes in the field; the word of the popup grows with
    /// them.
    fn type_text(&mut self, text: Vec<Stroke>) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        for stroke in text {
            draft.field.typed(stroke);
        }
        let cursor = draft.field.cursor;
        if let Some(popup) = &mut draft.comp.popup {
            popup.to = cursor;
        }
    }

    /// Opens the popup for the word at the cursor, or closes it. Inside
    /// a word, with text after the cursor, it opens only for the sigil
    /// that `typed` text has just put there, and goes on, popup or not,
    /// while the key `changed` the text of the word it opened for.
    pub(crate) fn follow_word(&mut self, changed: bool, typed: bool) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        draft.comp.popup = None;
        let under = draft.comp.under.take();
        let Some((from, word)) = draft.field.word_before() else {
            draft.comp.shut.clear();
            return;
        };
        let cursor = draft.field.cursor;
        let inside = draft.field.text.get(cursor).is_some_and(|&c| !is_sep(c));
        let sigil =
            word.len() == 1 && "+@".contains(&word) || word.ends_with(':');
        if inside && !(changed && under == Some(from) || typed && sigil) {
            return;
        }
        draft.comp.under = Some(from);
        if word == draft.comp.shut {
            return;
        }
        draft.comp.shut.clear();
        let to = draft.field.cursor;
        // The word counts as the popup's from here on, so what the
        // candidates look at leaves it out.
        draft.comp.popup = Some(Popup {
            from,
            to,
            cands: Vec::new(),
            at: 0,
            top: 0,
        });
        let cands = self.candidates_for(&word);
        let Some(draft) = &mut self.edit else {
            return;
        };
        match &mut draft.comp.popup {
            Some(popup) if !cands.is_empty() => popup.cands = cands,
            _ => draft.comp.popup = None,
        }
    }

    /// Makes the candidates again after a new reading of the files, and
    /// keeps the selection on the same candidate when it is still
    /// listed, else on the first.
    pub(crate) fn refresh_popup(&mut self) {
        let Some(old) = self.popup().and_then(|p| p.cands.get(p.at).cloned())
        else {
            return;
        };
        self.follow_word(true, true);
        if let Some(popup) = self.popup_mut() {
            popup.at = popup
                .cands
                .iter()
                .position(|c| same_choice(c, &old))
                .unwrap_or(0);
            popup.top = 0;
            popup.step(0);
        }
    }

    /// `close` shuts the popup on its word.
    pub(crate) fn close_popup(&mut self) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        if let Some(popup) = draft.comp.popup.take() {
            draft.comp.shut = draft.field.slice(popup.from, popup.to);
        }
        draft.comp.under = None;
    }

    /// `next` and `prev` move the selection.
    pub(crate) fn step_popup(&mut self, by: isize) {
        if let Some(popup) = self.popup_mut() {
            popup.step(by);
        }
    }

    /// Whether accepting would leave the text as it is: the selected
    /// candidate is the word already typed, with no text run into it.
    fn accepts_nothing(&self) -> bool {
        let open = self.edit.as_ref().zip(self.popup());
        open.is_some_and(|(draft, popup)| {
            let word = draft.field.slice(popup.from, popup.to);
            let chosen = popup.cands.get(popup.at);
            draft.field.text.get(popup.to).is_none_or(|&c| is_sep(c))
                && chosen.is_some_and(|c| c.put == Put::Text(word))
        })
    }

    /// Puts the selected candidate in place of the word. After a folder
    /// the popup opens again for its content.
    pub(crate) fn take_candidate(&mut self) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        let Some(popup) = draft.comp.popup.take() else {
            return;
        };
        let Some(cand) = popup.cands.get(popup.at) else {
            return;
        };
        let text = match &cand.put {
            Put::Text(text) => text.clone(),
            Put::NeedsId(task) => match self.give_id(*task) {
                Ok(text) => text,
                Err(e) => {
                    self.fail(&e);
                    return;
                }
            },
        };
        let Some(draft) = &mut self.edit else {
            return;
        };
        // Text that follows the word is not run into it.
        let folder = text.ends_with('/');
        let glued =
            draft.field.text.get(popup.to).is_some_and(|&c| !is_sep(c));
        let mut put = text.clone();
        if glued && !folder {
            put.push(' ');
        }
        draft.field.replace(popup.from, popup.to, &put);
        draft.comp.shut.clone_from(&text);
        if folder {
            draft.comp.shut.clear();
            draft.comp.under = Some(popup.from);
            self.follow_word(true, true);
        }
    }
}
