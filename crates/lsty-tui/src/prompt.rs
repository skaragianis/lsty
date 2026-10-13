//! The prompts of `gd`, `gt` and `gT`: a line in the status line that
//! asks for one thing and writes it to the items it was opened on.

use lsty_core::{
    case::lower,
    errs,
    keys::Action,
    list::{Item, ItemId},
    ops::{self, Env, Outcome, Target},
    theme::Slot,
    todotxt::{Kind as TokenKind, SEPARATORS},
};
use ratatui::{layout::Rect, style::Style};

use crate::{
    actions::{Chosen, archived_refusal, places},
    complete::ring,
    field::{Field, is_sep},
    history::Again,
    line::{Line, LineKind},
    messages,
    model::Model,
    text::{Span, fit, pad, put, width, width_of},
    track::{Place, Remap},
};

/// The keys a tag may have that completion offers.
pub(crate) const KNOWN_KEYS: [&str; 6] =
    ["due:", "t:", "rec:", "file:", "id:", "dep:"];

/// What stands between two candidates.
const GAP: &str = "  ";

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
        self.at = ring(self.at, by, self.cands.len() + 1);
        let word = self.chosen().unwrap_or(self.typed.as_str());
        field.replace(self.from, self.to, word);
        self.to = field.cursor;
    }

    /// The candidate in the text, if one is.
    pub(crate) fn chosen(&self) -> Option<&str> {
        let i = self.at.checked_sub(1)?;
        self.cands.get(i).map(String::as_str)
    }

    /// Whether a candidate rather than the typed word is in the text.
    pub(crate) const fn stepped(&self) -> bool {
        self.at > 0
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

    /// The first candidate shown: as many as fit before the one in the
    /// text, which ends within `w` cells, or one cell less when later
    /// candidates follow and `fit` ends the line with an ellipsis.
    fn first_shown(&self, w: usize) -> usize {
        let Some(at) = self.at.checked_sub(1) else {
            return 0;
        };
        let gap = width_of(GAP);
        let room = w.saturating_sub(usize::from(at + 1 < self.cands.len()));
        let mut used = self.cands.get(at).map_or(0, |s| width_of(s));
        let mut first = at;
        while let Some(prev) = first.checked_sub(1) {
            used += gap + self.cands.get(prev).map_or(0, |s| width_of(s));
            if used > room {
                break;
            }
            first = prev;
        }
        first
    }
}

/// A prompt that is open on items fixed when it opened.
#[derive(Debug)]
pub(crate) struct Prompt {
    /// What the prompt writes.
    kind: Kind,
    /// The count the prompt was opened with.
    count: usize,
    /// The lines of the items, followed across reloads.
    places: Vec<Place>,
}

impl LineKind {
    /// What the status line shows before the text of the line.
    pub(crate) const fn label(&self) -> &'static str {
        match self {
            Self::Search => "/",
            Self::Command => ":",
            Self::Items(prompt) => prompt.kind.label(),
        }
    }
}

impl Model {
    /// Opens the prompt of `kind` on the item under the cursor, or
    /// `count` items from it, unless one of them is archived.
    pub(crate) fn open_prompt(&mut self, kind: Kind, count: usize) {
        let items = self.items_of(count);
        let count = self.count_for(count, items.len());
        let chosen = self.chosen(&items);
        if chosen.is_empty() {
            return;
        }
        if let Some(text) = archived_refusal(&chosen) {
            self.alert(text);
            return;
        }
        let places = places(&self.snap, &items);
        let prompt = Prompt {
            kind,
            count,
            places,
        };
        self.line = Some(Line::new(LineKind::Items(prompt), ""));
    }

    /// Follows the lines of the prompt's items to a new reading; those
    /// that are gone are left out.
    pub(crate) fn remap_prompt(&mut self, moved: &Remap) {
        if let Some(Line {
            kind: LineKind::Items(prompt),
            ..
        }) = &mut self.line
        {
            prompt.places = prompt
                .places
                .iter()
                .filter_map(|at| moved.track(*at))
                .collect();
        }
    }

    /// The items of the prompt that are still in the files.
    fn prompt_items(&self) -> Vec<ItemId> {
        let Some(Line {
            kind: LineKind::Items(prompt),
            ..
        }) = &self.line
        else {
            return Vec::new();
        };
        prompt
            .places
            .iter()
            .filter_map(|&(source, nr)| self.snap.at(source, nr))
            .map(Item::item_id)
            .collect()
    }

    /// `execute`: one operation over the items. On an error the prompt
    /// stays open with its text.
    pub(crate) fn execute_prompt(&mut self) {
        let Some(Line {
            kind: LineKind::Items(prompt),
            field,
            ..
        }) = &self.line
        else {
            return;
        };
        let (kind, count) = (prompt.kind, prompt.count);
        let text = field.text().trim_matches(is_sep).to_owned();
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
        if let Some(refusal) = archived_refusal(&chosen) {
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
        let long = width_of(&shown) > room;
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
        if self.line.as_ref().is_some_and(|l| l.comp.is_none()) {
            let comp = self.start_completion();
            if let Some(line) = &mut self.line {
                line.comp = comp;
            }
        }
        if let Some(Line {
            field,
            comp: Some(comp),
            ..
        }) = &mut self.line
        {
            comp.step(field, by);
        }
        self.preview_completed();
    }

    /// Starts the candidates for the word under the cursor, when the
    /// line is the `:` line or a prompt.
    fn start_completion(&mut self) -> Option<Completion> {
        let line = self.line.as_ref()?;
        let field = &line.field;
        let (from, to) = field.word_span();
        let typed = field.slice(from, to);
        let before = field.slice(0, from);
        let pool = match &line.kind {
            LineKind::Search => return None,
            LineKind::Command => self.command_pool(&typed, &before),
            LineKind::Items(prompt) => {
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
    fn pool(&mut self, kind: Kind, word: &str) -> Vec<String> {
        match kind {
            Kind::SetDue => Vec::new(),
            Kind::AddTag => match word.chars().next() {
                Some('+' | '@') => self.name_pool(word),
                _ => KNOWN_KEYS.iter().map(|k| (*k).to_owned()).collect(),
            },
            Kind::RemoveTag => self.tokens_of_items(),
        }
    }

    /// The tokens other than words of the prompt's items, as written.
    fn tokens_of_items(&self) -> Vec<String> {
        self.prompt_items()
            .into_iter()
            .filter_map(|id| self.snap.item(id))
            .flat_map(|item| self.snap.line(item).tokens())
            .filter(|t| t.kind != TokenKind::Word)
            .map(|t| errs::lossy(t.name_text()))
            .collect()
    }

    /// The line above the status line: the candidates of the prompt, or
    /// its error.
    fn above(&self) -> Option<Vec<Span<'_>>> {
        let line = self.line.as_ref()?;
        if let Some(comp) = &line.comp {
            return Some(comp.spans(usize::from(self.width)));
        }
        if matches!(line.kind, LineKind::Search) {
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
/// [`rank`]): the word itself first, then those that start with it,
/// then names matched at a part, then those that hold it anywhere, each
/// group in byte order, each candidate once.
pub(crate) fn matching(pool: Vec<String>, word: &str) -> Vec<String> {
    let typed = word;
    let word = lower(word);
    let mut out: Vec<(bool, Rank, String)> = pool
        .into_iter()
        .filter_map(|c| rank(&lower(&c), &word).map(|r| (c != typed, r, c)))
        .collect();
    out.sort();
    out.dedup();
    out.into_iter().map(|(.., c)| c).collect()
}

/// How a candidate matches a typed word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// The candidate starts with the word.
    Start,
    /// A part of a `+` or `@` name after `.` or `/` starts with the word
    /// after its sigil: `@r` matches `@people.russ`.
    Part,
    /// The candidate holds the word (for a `+` or `@` word, its name holds
    /// the word after the sigil): `+ork` matches `+work`.
    Anywhere,
}

/// How `candidate` matches `word`, both in lower case; `None` when it
/// does not.
fn rank(candidate: &str, word: &str) -> Option<Rank> {
    if candidate.starts_with(word) {
        return Some(Rank::Start);
    }
    let mut chars = word.chars();
    let (name, rest) = match chars.next().filter(|c| matches!(c, '+' | '@')) {
        Some(sigil) => (candidate.strip_prefix(sigil)?, chars.as_str()),
        None => (candidate, word),
    };
    if name
        .match_indices(SEPARATORS)
        .any(|(i, _)| name[i + 1..].starts_with(rest))
    {
        return Some(Rank::Part);
    }
    name.contains(rest).then_some(Rank::Anywhere)
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
        text.split(is_sep)
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect()
    };
    match kind {
        Kind::SetDue => ops::due(env, &targets, text),
        Kind::AddTag => ops::tag(env, &targets, &words()),
        Kind::RemoveTag => ops::untag(env, &targets, &words()),
    }
}
