//! The prompts of `gd`, `gt` and `gT`: a line in the status line that
//! asks for one thing and writes it to the items it was opened on.

use lsty_core::errs::{self, Nr};
use lsty_core::keys::{Action, Mode, lower};
use lsty_core::list::ItemId;
use lsty_core::ops::{self, Env, Outcome, Target};
use lsty_core::theme::Slot;
use lsty_core::todotxt::Kind as TokenKind;

use crate::actions::Chosen;
use crate::field::Field;
use crate::history::Again;
use crate::messages;
use crate::model::Model;
use crate::text::{Span, fit, pad, put, width};
use crate::track::{Place, Remap};

/// The keys a tag may have that completion offers.
const KNOWN_KEYS: [&str; 6] = ["due:", "t:", "rec:", "file:", "id:", "dep:"];

/// What the prompt label is for a search.
pub(crate) const SEARCH_LABEL: &str = "/";

/// What a prompt asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `set_due`: a fuzzy date; empty clears the date.
    SetDue,
    /// `add_tag`: tags to add.
    AddTag,
    /// `remove_tag`: tags to remove.
    RemoveTag,
}

impl Kind {
    const fn label(self) -> &'static str {
        match self {
            Self::SetDue => "due: ",
            Self::AddTag => "tag: ",
            Self::RemoveTag => "untag: ",
        }
    }

    /// Whether an empty text closes the prompt without a write.
    const fn needs_text(self) -> bool {
        !matches!(self, Self::SetDue)
    }
}

/// The candidates for the word under the cursor, and which one is in
/// the text. At 0 the word is as typed; at `i` it is the `i`-th
/// candidate.
#[derive(Debug)]
struct Completion {
    from: usize,
    to: usize,
    typed: String,
    cands: Vec<String>,
    at: usize,
}

impl Completion {
    /// Moves `by` places round the list and puts that word in `field`.
    fn step(&mut self, field: &mut Field, by: isize) {
        let n = isize::try_from(self.cands.len() + 1).unwrap_or(isize::MAX);
        let at = (isize::try_from(self.at).unwrap_or(0) + by).rem_euclid(n);
        self.at = usize::try_from(at).unwrap_or(0);
        let word = match self.at.checked_sub(1) {
            None => self.typed.as_str(),
            Some(i) => self.cands.get(i).map_or("", String::as_str),
        };
        field.replace(self.from, self.to, word);
        self.to = self.from + word.chars().count();
    }

    fn spans(&self) -> Vec<Span<'_>> {
        let mut out = Vec::new();
        for (i, cand) in self.cands.iter().enumerate() {
            if i > 0 {
                out.push(Span::new("  ", None));
            }
            let slot = if i + 1 == self.at {
                Slot::Selection
            } else {
                Slot::Text
            };
            out.push(Span::new(cand.as_str(), Some(slot)));
        }
        out
    }
}

/// A prompt that is open on items fixed when it opened.
#[derive(Debug)]
pub(crate) struct Prompt {
    kind: Kind,
    /// The count the prompt was opened with.
    count: usize,
    /// The lines of the items, followed across reloads.
    places: Vec<Place>,
    comp: Option<Completion>,
}

impl Prompt {
    /// Ends the list of candidates; the text stays as it is.
    pub(crate) fn drop_completion(&mut self) {
        self.comp = None;
    }
}

impl Model {
    /// What the status line shows before the text of the prompt.
    pub(crate) fn prompt_label(&self) -> &'static str {
        self.prompt
            .as_ref()
            .map_or(SEARCH_LABEL, |prompt| prompt.kind.label())
    }

    /// Opens the prompt of `kind` on `items`, unless one of them is
    /// archived.
    pub(crate) fn open_prompt(
        &mut self,
        kind: Kind,
        items: &[ItemId],
        count: usize,
    ) {
        let chosen = self.chosen(items);
        if chosen.is_empty() {
            return;
        }
        if let Some(text) = crate::actions::archived_refusal(&chosen) {
            self.alert(text);
            return;
        }
        let places = items
            .iter()
            .filter_map(|id| self.snap.item(*id))
            .map(|item| (item.source(), item.nr().get()))
            .collect();
        self.input = Some(Field::new(""));
        self.prompt = Some(Prompt {
            kind,
            count,
            places,
            comp: None,
        });
        self.mode = Mode::Command;
    }

    /// Follows the lines of the prompt's items to a new reading; those
    /// that are gone are left out.
    pub(crate) fn remap_prompt(&mut self, moved: &Remap) {
        if let Some(prompt) = &mut self.prompt {
            prompt.places = prompt
                .places
                .iter()
                .filter_map(|at| moved.track(*at))
                .collect();
        }
    }

    /// The items of the prompt that are still in the files.
    fn prompt_items(&self) -> Vec<ItemId> {
        let Some(prompt) = &self.prompt else {
            return Vec::new();
        };
        prompt
            .places
            .iter()
            .filter_map(|&(source, nr)| self.snap.at(source, Nr::new(nr)?))
            .map(lsty_core::list::Item::item_id)
            .collect()
    }

    /// `execute`: one operation over the items. On an error the prompt
    /// stays open with its text.
    pub(crate) fn execute_prompt(&mut self) {
        let (Some(prompt), Some(field)) = (&self.prompt, &self.input) else {
            return;
        };
        let (kind, count) = (prompt.kind, prompt.count);
        let text = field.text().trim().to_owned();
        let items = self.prompt_items();
        if items.is_empty() {
            self.close_input();
            self.alert(messages::GONE.to_owned());
            return;
        }
        if text.is_empty() && kind.needs_text() {
            self.close_input();
            return;
        }
        let chosen = self.chosen(&items);
        let ran = self.ending(&items, |m| {
            let result = m.attempt(|env| write(env, kind, &chosen, &text));
            if let Err(e) = result {
                return m.settle(Err(e));
            }
            m.close_input();
            m.settle(result)
        });
        if ran {
            self.again = Some(Again::Prompt { kind, count, text });
            self.summarize(chosen.len());
        }
    }

    /// `repeat` of a prompt's action: the same text over the items under
    /// the cursor, without opening the prompt.
    pub(crate) fn repeat_prompt(
        &mut self,
        kind: Kind,
        count: usize,
        text: &str,
    ) {
        let chosen = self.chosen(&self.picked(count));
        if chosen.is_empty() {
            return;
        }
        if let Some(refusal) = crate::actions::archived_refusal(&chosen) {
            self.alert(refusal);
            return;
        }
        let result = self.attempt(|env| write(env, kind, &chosen, text));
        if self.settle(result) {
            self.summarize(chosen.len());
        }
    }

    /// Replaces the messages of several items by a count when they do
    /// not fit the status line.
    fn summarize(&mut self, n: usize) {
        let Some(shown) = self.message.take() else {
            return;
        };
        let used = width(&self.status_base());
        let room = usize::from(self.width).saturating_sub(used + 2);
        let long = Span::new(shown.as_str(), None).cells() > room;
        self.message = Some(if n > 1 && long {
            format!("{n} items changed")
        } else {
            shown
        });
    }

    /// `complete_next` and `complete_prev`: the word under the cursor
    /// takes the next or the previous candidate.
    pub(crate) fn complete(&mut self, action: Action) {
        let by = if action == Action::CompletePrev {
            -1
        } else {
            1
        };
        if self.prompt.as_ref().is_some_and(|p| p.comp.is_none()) {
            let comp = self.start_completion();
            if let Some(prompt) = &mut self.prompt {
                prompt.comp = comp;
            }
        }
        if let (Some(prompt), Some(field)) =
            (&mut self.prompt, &mut self.input)
            && let Some(comp) = &mut prompt.comp
        {
            comp.step(field, by);
        }
    }

    fn start_completion(&self) -> Option<Completion> {
        let kind = self.prompt.as_ref()?.kind;
        let field = self.input.as_ref()?;
        let (from, to) = field.word_span();
        let typed = field.slice(from, to);
        let cands = matching(self.pool(kind, &typed), &typed);
        (!cands.is_empty()).then_some(Completion {
            from,
            to,
            typed,
            cands,
            at: 0,
        })
    }

    /// What can complete `word` in the prompt of `kind`.
    fn pool(&self, kind: Kind, word: &str) -> Vec<String> {
        match kind {
            Kind::SetDue => Vec::new(),
            Kind::AddTag => match word.chars().next() {
                Some('+') => self.names_of(TokenKind::Project),
                Some('@') => self.names_of(TokenKind::Context),
                _ => KNOWN_KEYS.iter().map(|k| (*k).to_owned()).collect(),
            },
            Kind::RemoveTag => self.tokens_of_items(),
        }
    }

    /// The projects or contexts of both files, as written.
    fn names_of(&self, kind: TokenKind) -> Vec<String> {
        self.snap
            .items()
            .iter()
            .flat_map(|item| self.snap.line(item).tokens())
            .filter(|t| t.kind == kind)
            .map(|t| errs::lossy(t.text))
            .collect()
    }

    /// The tokens other than words of the prompt's items, as written.
    fn tokens_of_items(&self) -> Vec<String> {
        self.prompt_items()
            .into_iter()
            .filter_map(|id| self.snap.item(id))
            .flat_map(|item| self.snap.line(item).tokens())
            .filter(|t| t.kind != TokenKind::Word)
            .map(|t| errs::lossy(t.text))
            .collect()
    }

    /// The line above the status line: the candidates of the prompt, or
    /// its error.
    fn above(&self) -> Option<Vec<Span<'_>>> {
        let prompt = self.prompt.as_ref()?;
        if let Some(comp) = &prompt.comp {
            return Some(comp.spans());
        }
        let message = self.message.as_ref()?;
        (self.tone == messages::Tone::Alert)
            .then(|| vec![Span::new(message.as_str(), Some(Slot::Error))])
    }

    /// Draws what the prompt has to say on row `y`, over the list.
    pub(crate) fn draw_above(
        &self,
        buf: &mut ratatui::buffer::Buffer,
        at: (u16, u16),
    ) {
        let Some(spans) = self.above() else {
            return;
        };
        let (cells, y) = at;
        let w = usize::from(cells);
        let mut line = fit(spans, w);
        pad(&mut line, w, None);
        put(buf, (0, y), w, &line, &self.styles);
    }
}

/// The candidates of `pool` that start with `word`, whatever the case,
/// in byte order, each once.
pub(crate) fn matching(pool: Vec<String>, word: &str) -> Vec<String> {
    let word = lower(word);
    let mut out: Vec<String> = pool
        .into_iter()
        .filter(|c| lower(c).starts_with(&word))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The operation of the prompt over the items.
fn write(
    env: &Env<'_>,
    kind: Kind,
    items: &[Chosen],
    text: &str,
) -> errs::Result<Outcome> {
    let targets: Vec<Target> = items.iter().map(Chosen::target).collect();
    let words = || -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    };
    match kind {
        Kind::SetDue => ops::due(env, &targets, text),
        Kind::AddTag => ops::tag(env, &targets, &words()),
        Kind::RemoveTag => ops::untag(env, &targets, &words()),
    }
}
