use jiff::tz::TimeZone;

use crate::{
    dates::{Clock, Date, Fuzzy},
    todotxt::Kind,
};

/// A kind of task that commands hide unless asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Tasks whose `t:` date is still ahead.
    Later,
    /// Done tasks.
    Done,
    /// Tasks in the done file.
    Archived,
    /// Tasks waiting on a prerequisite.
    Blocked,
}

impl Category {
    /// Every category, in declaration order.
    pub(crate) const ALL: [Self; 4] =
        [Self::Later, Self::Done, Self::Archived, Self::Blocked];
}

/// What a query needs to resolve dates when it is evaluated.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The date `today` and the offsets of a query count from.
    pub today: Date,
    /// The zone that turns a `started:` or `ended:` instant into a date.
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

/// A parsed filter. The empty query matches every task.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub(super) terms: Vec<Term>,
    /// What the filter text suggests the writer meant otherwise.
    pub(super) warnings: Vec<String>,
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
    /// The task has a valid date in the field.
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
    Ended,
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
    /// This many days after today, however far that is.
    DaysAhead(u64),
}
