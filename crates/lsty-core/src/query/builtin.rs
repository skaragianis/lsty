use super::model::{Alt, Field, Op, Query, StateTest, Term, Test, When};

/// The narrowing filter that the doing count of the status line selects.
pub const DOING: &str = "Doing";

impl Query {
    /// The narrowing filters, in the order the Filters panel lists them:
    /// `Today`, `Overdue`, `Upcoming`, `Doing`, `Waiting` and
    /// `No project`. `upcoming_days` is how far `Upcoming` looks ahead.
    #[must_use]
    pub fn builtins(upcoming_days: u64) -> Vec<(&'static str, Self)> {
        let text = |text| Self::parse(text).unwrap_or_default();
        vec![
            ("Today", text("due:<=today is:open")),
            ("Overdue", text("is:overdue")),
            ("Upcoming", upcoming(upcoming_days)),
            (DOING, text("is:doing")),
            ("Waiting", text("is:blocked")),
            ("No project", text("-+")),
        ]
    }

    /// The query of one word term, taken as literal text.
    pub fn word(text: &str) -> Self {
        Self::single(super::atom::word(text))
    }

    /// The query of one project or context name and its subtree.
    pub fn name(kind: crate::todotxt::Kind, name: &[u8]) -> Self {
        Self::single(super::atom::named(kind, name))
    }

    fn single(alt: Alt) -> Self {
        Self {
            terms: vec![Term {
                negated: false,
                alts: vec![alt],
            }],
        }
    }
}

/// `due:>=today due:<=+<days>d is:open`, for any number of days.
fn upcoming(days: u64) -> Query {
    let due = |op, when| {
        Query::single(Alt::new(Test::Date {
            field: Field::Due,
            op,
            when,
        }))
    };
    Query::and([
        due(Op::Ge, When::Today),
        due(Op::Le, When::DaysAhead(days)),
        Query::single(Alt::new(Test::State(StateTest::Open))),
    ])
}
