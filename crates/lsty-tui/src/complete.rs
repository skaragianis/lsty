//! The completion popup of INSERT mode: it follows the word at the
//! text cursor, offers what can follow, and has its own keys.

use lsty_core::keys::{Action, Fed, Mode, Outcome as Pressed, Stroke};

use crate::model::Model;

/// How many candidates the popup shows at once.
pub(crate) const POPUP_ROWS: usize = 8;

/// What accepting a candidate puts in place of the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Put {
    /// This text.
    Text(String),
    /// `dep:` and the id of the item at this line, which gets one now.
    NeedsId(lsty_core::list::ItemId),
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

impl Popup {
    /// Selects the candidate `by` places on, round the list.
    fn step(&mut self, by: isize) {
        let n = isize::try_from(self.cands.len()).unwrap_or(isize::MAX);
        let at = (isize::try_from(self.at).unwrap_or(0) + by).rem_euclid(n);
        self.at = usize::try_from(at).unwrap_or(0);
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
/// The item of a `dep:` candidate is not compared: it belongs to one
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
    /// The word that was closed with `close`: no popup until it changes.
    pub(crate) shut: String,
    /// The ids to give to the items the text waits for.
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
    /// The keys of the popup layered over those of INSERT mode while the
    /// popup is open, those of INSERT mode otherwise. Keys pending in
    /// the one that is not used move over to the one that is.
    fn edit_fed(&mut self, stroke: Stroke) -> Fed {
        let open = self.edit.as_ref().is_some_and(|d| d.comp.popup.is_some());
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

    /// Takes a key press while the field is open.
    pub(crate) fn edit_press(&mut self, stroke: Stroke) {
        let fed = self.edit_fed(stroke);
        let own = matches!(
            fed.outcome,
            Pressed::Matched { action, .. } if is_popup_action(action)
        );
        self.type_text(fed.text);
        if let Pressed::Matched { action, .. } = fed.outcome {
            self.edit_action(action);
        }
        if self.edit.is_some() {
            if !own {
                self.follow_word();
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

    /// Opens the popup for the word at the cursor, or closes it.
    pub(crate) fn follow_word(&mut self) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        draft.comp.popup = None;
        let Some((from, word)) = draft.field.word_before() else {
            draft.comp.shut.clear();
            return;
        };
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
        let Some(old) = self
            .edit
            .as_ref()
            .and_then(|d| d.comp.popup.as_ref())
            .and_then(|p| p.cands.get(p.at).cloned())
        else {
            return;
        };
        self.follow_word();
        if let Some(popup) =
            self.edit.as_mut().and_then(|d| d.comp.popup.as_mut())
        {
            popup.at = popup
                .cands
                .iter()
                .position(|c| same_choice(c, &old))
                .unwrap_or(0);
            popup.top = 0;
            popup.step(0);
        }
    }

    /// `accept`, `close`, `next` and `prev`.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the popup's actions get here"
    )]
    pub(crate) fn popup_action(&mut self, action: Action) {
        match action {
            Action::Accept => self.take_candidate(),
            Action::Close | Action::Next | Action::Prev => {
                self.move_in_popup(action);
            }
            _ => {}
        }
    }

    /// `close` shuts the popup on its word; `next` and `prev` move the
    /// selection.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the moves of the popup get here"
    )]
    fn move_in_popup(&mut self, action: Action) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        let Some(popup) = &mut draft.comp.popup else {
            return;
        };
        match action {
            Action::Close => {
                draft.comp.shut = draft.field.slice(popup.from, popup.to);
                draft.comp.popup = None;
            }
            Action::Next => popup.step(1),
            Action::Prev => popup.step(-1),
            _ => {}
        }
    }

    /// Puts the selected candidate in place of the word. After a folder
    /// the popup opens again for its content.
    fn take_candidate(&mut self) {
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
            Put::NeedsId(item) => match self.give_id(*item) {
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
        draft.field.replace(popup.from, popup.to, &text);
        draft.comp.shut.clone_from(&text);
        if text.ends_with('/') {
            draft.comp.shut.clear();
            self.follow_word();
        }
    }
}
