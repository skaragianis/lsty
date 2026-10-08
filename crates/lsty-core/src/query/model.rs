use jiff::tz::TimeZone;

use crate::config::Hidden;
use crate::dates::{Clock, Date, Fuzzy};
use crate::todotxt::Kind;

/// A kind of item that commands hide unless asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Items whose `t:` date is still ahead.
    Later,
    /// Done items.
    Done,
    /// Items in the done file.
    Archived,
    /// Items waiting on a prerequisite.
    Blocked,
}

impl Category {
    /// Every category, in the order [`Query::overrides`] lists them.
    pub(crate) const ALL: [Self; 4] =
        [Self::Later, Self::Done, Self::Archived, Self::Blocked];

    /// The category's name as the `is:` term writes it.
    #[cfg(test)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Later => "later",
            Self::Done => "done",
            Self::Archived => "archived",
            Self::Blocked => "blocked",
        }
    }
}

impl From<Hidden> for Category {
    /// The category an entry of `hidden_items` names.
    fn from(hidden: Hidden) -> Self {
        match hidden {
            Hidden::Later => Self::Later,
            Hidden::Blocked => Self::Blocked,
            Hidden::Done => Self::Done,
            Hidden::Archive => Self::Archived,
        }
    }
}

/// What a query needs to resolve dates when it is evaluated.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The date `today` and the offsets of a query count from.
    pub today: Date,
    /// The zone that turns a `started:` instant into a date.
    pub zone: &'a TimeZone,
}

impl<'a> Context<'a> {
    /// The context of one reading of the clock.
    pub fn from_clock(clock: &'a Clock) -> Self {
        Self {
            today: clock.today(),
            zone: clock.zone(),
        }
    }
}

/// A parsed filter. The empty query matches every item.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub(super) terms: Vec<Term>,
}

/// Alternatives that all stand or fall together, possibly negated.
#[derive(Clone, Debug)]
pub(super) struct Term {
    pub(super) negated: bool,
    pub(super) alts: Vec<Alt>,
}

#[derive(Clone, Debug)]
pub(super) struct Alt {
    pub(super) test: Test,
    /// The category a matching `is:` alternative asks for.
    pub(super) category: Option<Category>,
}

impl Alt {
    pub(super) const fn new(test: Test) -> Self {
        Self {
            test,
            category: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Test {
    /// A project or context, `name` and everything below it; any when
    /// `name` is empty.
    Token {
        kind: Kind,
        name: Vec<u8>,
    },
    /// Literal text in the description.
    Word(Word),
    /// A tag with that key and, unless empty, that value.
    Tag {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    State(StateTest),
    /// An effective priority from `lo` to `hi`, as letters.
    Priority {
        lo: u8,
        hi: u8,
    },
    /// The item has a valid date in the field.
    HasDate(Field),
    /// The field's date compared with a date that is resolved late.
    Date {
        field: Field,
        op: Op,
        when: When,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Word {
    /// The text: as written when exact, otherwise in lower case.
    pub(super) needle: Vec<u8>,
    /// The characters of a lower-case needle.
    pub(super) folded: Vec<char>,
    pub(super) exact: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum StateTest {
    Open,
    Done,
    Doing,
    Blocked,
    Later,
    Overdue,
    Archived,
    Dangling,
    Stale,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Field {
    Due,
    Threshold,
    Started,
    Created,
    Completed,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

/// The date a comparison is made against.
#[derive(Clone, Debug)]
pub(super) enum When {
    /// A fuzzy date, placed against today at evaluation.
    Fuzzy(Fuzzy),
    /// Today.
    Today,
    /// This many days after today, however far that is.
    DaysAhead(u64),
}
