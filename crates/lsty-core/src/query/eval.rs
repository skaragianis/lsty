use std::cmp::Ordering;

use super::model::{
    Alt, Category, Context, Field, Op, Query, StateTest, Term, Test, When,
};
use crate::dates::Date;
use crate::list::{Item, Snapshot, State};
use crate::todotxt::{Kind, Line, is_single_valued, name_has_prefix};

/// What an alternative reads: the snapshot, the item and the context.
struct Subject<'a> {
    snap: &'a Snapshot,
    item: &'a Item,
    ctx: &'a Context<'a>,
}

impl Query {
    /// The query that holds when every one of `queries` holds.
    pub fn and(queries: impl IntoIterator<Item = Self>) -> Self {
        Self {
            terms: queries.into_iter().flat_map(|q| q.terms).collect(),
        }
    }

    /// Whether `item`, which comes from `snap`, satisfies every term.
    pub fn matches(
        &self,
        snap: &Snapshot,
        item: &Item,
        ctx: &Context<'_>,
    ) -> bool {
        let subject = Subject { snap, item, ctx };
        self.terms.iter().all(|t| t.matches(&subject))
    }

    /// The categories the query asks for in a term that is not negated,
    /// in the order of [`Category::ALL`].
    pub fn overrides(&self) -> Vec<Category> {
        Category::ALL
            .into_iter()
            .filter(|&c| self.asks_for(c))
            .collect()
    }

    /// Whether an alternative of a term that is not negated names `cat`.
    pub fn asks_for(&self, cat: Category) -> bool {
        self.terms.iter().any(|t| {
            !t.negated && t.alts.iter().any(|a| a.category == Some(cat))
        })
    }

    /// Whether a later item is one the query did not ask for: it is
    /// counted apart where later items are skipped.
    pub(crate) fn skips_later(&self, item: &Item) -> bool {
        item.later() && !self.asks_for(Category::Later)
    }
}

impl Term {
    fn matches(&self, subject: &Subject<'_>) -> bool {
        let hit = self.alts.iter().any(|a| a.matches(subject));
        hit != self.negated
    }
}

impl Alt {
    fn matches(&self, subject: &Subject<'_>) -> bool {
        let line = subject.snap.line(subject.item);
        match &self.test {
            Test::Token { kind, name } => has_token(line, *kind, name),
            Test::Word(word) => word.found_in(line.description()),
            Test::Tag { key, value } => has_tag(line, key, value),
            Test::State(state) => state_holds(*state, subject),
            Test::Priority { lo, hi } => in_priority(line, *lo, *hi),
            Test::HasDate(field) => field_date(*field, subject).is_some(),
            Test::Date { field, op, when } => {
                compare(*field, *op, when, subject)
            }
        }
    }
}

fn has_token(line: &Line, kind: Kind, name: &[u8]) -> bool {
    line.tokens().any(|t| {
        t.kind == kind && (name.is_empty() || name_has_prefix(t.value, name))
    })
}

/// A tag with `key` and, unless `value` is empty, exactly `value`. Only
/// the first token of a single-valued key counts.
fn has_tag(line: &Line, key: &[u8], value: &[u8]) -> bool {
    let mut tags = line
        .tokens()
        .filter(|t| t.kind == Kind::Tag && t.key == key);
    let fits = |v: &[u8]| value.is_empty() || v == value;
    if is_single_valued(key) {
        tags.next().is_some_and(|t| fits(t.value))
    } else {
        tags.any(|t| fits(t.value))
    }
}

fn in_priority(line: &Line, lo: u8, hi: u8) -> bool {
    line.effective_priority()
        .is_some_and(|p| (lo..=hi).contains(&p.letter()))
}

fn state_holds(state: StateTest, subject: &Subject<'_>) -> bool {
    let item = subject.item;
    match state {
        StateTest::Open => item.state() != State::Done,
        StateTest::Done => item.state() == State::Done,
        StateTest::Doing => item.state() == State::Doing,
        StateTest::Blocked => item.blocked(),
        StateTest::Later => item.later(),
        StateTest::Overdue => item.overdue(),
        StateTest::Archived => item.source() == crate::errs::Source::Done,
        StateTest::Dangling => item.has_dangling(),
        StateTest::Stale => item.stale(),
    }
}

/// The date of the first tag called `key`, when it is a real date.
fn tag_date(line: &Line, key: &[u8]) -> Option<Date> {
    let tok = line
        .tokens()
        .find(|t| t.kind == Kind::Tag && t.key == key)?;
    Date::parse(std::str::from_utf8(tok.value).ok()?)
}

/// The date of an item's field. An instant whose local date is outside
/// the calendar still has a date: it lies before or after all of them.
#[derive(Clone, Copy)]
enum Have {
    On(Date),
    Before,
    After,
}

fn field_date(field: Field, subject: &Subject<'_>) -> Option<Have> {
    let line = subject.snap.line(subject.item);
    let on = |date: Option<Date>| date.map(Have::On);
    match field {
        Field::Due => on(tag_date(line, b"due")),
        Field::Threshold => on(tag_date(line, b"t")),
        Field::Created => on(Date::parse(line.created()?)),
        Field::Completed => on(Date::parse(line.completed()?)),
        Field::Started => started_date(subject),
    }
}

fn started_date(subject: &Subject<'_>) -> Option<Have> {
    let at = subject.item.started()?;
    let zone = subject.ctx.zone;
    if let Some(date) = Date::local(at, zone) {
        return Some(Have::On(date));
    }
    let before = zone.to_datetime(at).date().year() < 0;
    Some(if before { Have::Before } else { Have::After })
}

/// How `have` stands to the date `when` names today, or `None` when
/// that date does not exist.
fn order(have: Have, when: &When, today: Date) -> Option<Ordering> {
    let want = match when {
        When::Today => Some(today),
        When::DaysAhead(_) => None,
        When::Fuzzy(fuzzy) => Some(fuzzy.on(today)?),
    };
    match (have, want) {
        (Have::Before, _) => Some(Ordering::Less),
        (Have::After, _) => Some(Ordering::Greater),
        (Have::On(date), Some(want)) => Some(date.cmp(&want)),
        (Have::On(date), None) => {
            let When::DaysAhead(n) = when else {
                return None;
            };
            let days = i128::from(today.days_until(date));
            Some(days.cmp(&i128::from(*n)))
        }
    }
}

fn compare(field: Field, op: Op, when: &When, subject: &Subject<'_>) -> bool {
    let Some(have) = field_date(field, subject) else {
        return false;
    };
    let Some(found) = order(have, when, subject.ctx.today) else {
        return false;
    };
    match op {
        Op::Lt => found.is_lt(),
        Op::Le => found.is_le(),
        Op::Gt => found.is_gt(),
        Op::Ge => found.is_ge(),
        Op::Eq => found.is_eq(),
    }
}
