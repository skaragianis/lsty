//! The prompts of `gd`, `gt` and `gT`: a line in the status line that
//! asks for one thing and writes it to the items it was opened on.

use lsty_core::case::lower;
use lsty_core::errs;
use lsty_core::keys::Action;
use lsty_core::list::ItemId;
use lsty_core::ops::{self, Env, Outcome, Target};
use lsty_core::theme::Slot;
use lsty_core::todotxt::Kind as TokenKind;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::actions::Chosen;
use crate::field::Field;
use crate::history::Again;
use crate::line::Line;
use crate::messages;
use crate::model::Model;
use crate::text::{Span, fit, pad, put, width};
use crate::track::{Place, Remap};

/// The keys a tag may have that completion offers.
pub(crate) const KNOWN_KEYS: [&str; 6] =
    ["due:", "t:", "rec:", "file:", "id:", "dep:"];

/// What the prompt label is for the `:` line.
const COMMAND_LABEL: &str = ":";

/// What stands between two candidates.
const GAP: &str = "  ";

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
pub(crate) struct Completion {
    from: usize,
    to: usize,
    typed: String,
    cands: Vec<String>,
    at: usize,
}

impl Completion {
    /// Moves `by` places round the list and puts that word in `field`.
    pub(crate) fn step(&mut self, field: &mut Field, by: isize) {
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

    /// The candidate in the text, if one is.
    pub(crate) fn chosen(&self) -> Option<&str> {
        let i = self.at.checked_sub(1)?;
        self.cands.get(i).map(String::as_str)
    }

    /// The candidates that fit in `w` cells, from one that leaves the
    /// candidate in the text in sight.
    pub(crate) fn spans(&self, w: usize) -> Vec<Span<'_>> {
        let first = self.first_shown(w);
        let mut out = Vec::new();
        for (i, cand) in self.cands.iter().enumerate().skip(first) {
            if i > first {
                out.push(Span::new(GAP, None));
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

    /// The first candidate shown, so that the one in the text ends
    /// within `w` cells.
    fn first_shown(&self, w: usize) -> usize {
        let Some(at) = self.at.checked_sub(1) else {
            return 0;
        };
        let cells = |s: &str| Span::new(s, None).cells();
        let gap = cells(GAP);
        let mut end: usize = self
            .cands
            .iter()
            .take(at + 1)
            .map(|c| cells(c) + gap)
            .sum::<usize>()
            .saturating_sub(gap);
        let mut first = 0;
        while first < at && end > w {
            end = end.saturating_sub(
                self.cands.get(first).map_or(0, |c| cells(c)) + gap,
            );
            first += 1;
        }
        first
    }
}

/// A prompt that is open on items fixed when it opened.
#[derive(Debug)]
pub(crate) struct Prompt {
    pub(crate) field: Field,
    /// What the prompt writes.
    kind: Kind,
    /// The count the prompt was opened with.
    count: usize,
    /// The lines of the items, followed across reloads.
    places: Vec<Place>,
    pub(crate) comp: Option<Completion>,
}

impl Model {
    /// What the status line shows before the text of the prompt.
    pub(crate) fn prompt_label(&self) -> &'static str {
        self.line.as_ref().map_or(SEARCH_LABEL, |line| match line {
            Line::Search(_) => SEARCH_LABEL,
            Line::Command { .. } => COMMAND_LABEL,
            Line::Items(prompt) => prompt.kind.label(),
        })
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
            .map(|item| (item.source(), item.nr()))
            .collect();
        self.line = Some(Line::Items(Prompt {
            field: Field::new(""),
            kind,
            count,
            places,
            comp: None,
        }));
    }

    /// Follows the lines of the prompt's items to a new reading; those
    /// that are gone are left out.
    pub(crate) fn remap_prompt(&mut self, moved: &Remap) {
        if let Some(Line::Items(prompt)) = &mut self.line {
            prompt.places = prompt
                .places
                .iter()
                .filter_map(|at| moved.track(*at))
                .collect();
        }
    }

    /// The items of the prompt that are still in the files.
    fn prompt_items(&self) -> Vec<ItemId> {
        let Some(Line::Items(prompt)) = &self.line else {
            return Vec::new();
        };
        prompt
            .places
            .iter()
            .filter_map(|&(source, nr)| self.snap.at(source, nr))
            .map(lsty_core::list::Item::item_id)
            .collect()
    }

    /// `execute`: one operation over the items. On an error the prompt
    /// stays open with its text.
    pub(crate) fn execute_prompt(&mut self) {
        let Some(Line::Items(prompt)) = &self.line else {
            return;
        };
        let (kind, count) = (prompt.kind, prompt.count);
        let text = prompt.field.text().trim().to_owned();
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
        if self.line.as_ref().is_some_and(Line::can_complete) {
            let comp = self.start_completion();
            if let Some(line) = &mut self.line {
                line.set_completion(comp);
            }
        }
        if let Some(line) = &mut self.line {
            line.step_completion(by);
        }
        self.preview_completed();
    }

    /// Starts the candidates for the word under the cursor, when the
    /// line is the `:` line or a prompt.
    fn start_completion(&mut self) -> Option<Completion> {
        let line = self.line.as_ref()?;
        let field = line.field();
        let (from, to) = field.word_span();
        let typed = field.slice(from, to);
        let before = field.slice(0, from);
        let pool = match line {
            Line::Search(_) => return None,
            Line::Command { .. } => self.command_pool(&typed, &before),
            Line::Items(prompt) => {
                let kind = prompt.kind;
                self.pool(kind, &typed)
            }
        };
        let cands = matching(pool, &typed);
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
    pub(crate) fn names_of(&self, kind: TokenKind) -> Vec<String> {
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
        let line = self.line.as_ref()?;
        if let Some(comp) = line.completion() {
            return Some(comp.spans(usize::from(self.width)));
        }
        if matches!(line, Line::Search(_)) {
            return None;
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
        buf.set_style(Rect::new(0, y, cells, 1), Style::reset());
        put(buf, (0, y), w, &line, &self.styles);
    }
}

/// The candidates of `pool` that match `word`, whatever the case (see
/// [`rank`]): those that start with it first, then names matched at a
/// part, each group in byte order, each candidate once.
pub(crate) fn matching(pool: Vec<String>, word: &str) -> Vec<String> {
    let word = lower(word);
    let mut out: Vec<(Rank, String)> = pool
        .into_iter()
        .filter_map(|c| rank(&lower(&c), &word).map(|r| (r, c)))
        .collect();
    out.sort();
    out.dedup();
    out.into_iter().map(|(_, c)| c).collect()
}

/// How a candidate matches a typed word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    /// The candidate starts with the word.
    Start,
    /// A part of a `+` or `@` name after `.` or `/` starts with the word
    /// after its sigil: `@r` matches `@people.russ`.
    Part,
}

/// How `candidate` matches `word`, both in lower case; `None` when it
/// does not.
pub(crate) fn rank(candidate: &str, word: &str) -> Option<Rank> {
    if candidate.starts_with(word) {
        return Some(Rank::Start);
    }
    let mut chars = word.chars();
    let sigil = chars.next().filter(|c| matches!(c, '+' | '@'))?;
    let rest = chars.as_str();
    if rest.is_empty() || !candidate.starts_with(sigil) {
        return None;
    }
    candidate
        .match_indices(['.', '/'])
        .any(|(i, sep)| {
            candidate
                .get(i + sep.len()..)
                .is_some_and(|part| part.starts_with(rest))
        })
        .then_some(Rank::Part)
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
