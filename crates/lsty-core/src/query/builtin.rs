use super::model::{Alt, Field, Op, Query, StateTest, Term, Test, When};

impl Query {
    /// The narrowing filter called `name`: `Today`, `Overdue`, `Upcoming`,
    /// `Doing`, `Waiting` or `No project`. `upcoming_days` is how far
    /// `Upcoming` looks ahead.
    pub fn builtin(name: &str, upcoming_days: u64) -> Option<Self> {
        let text = match name {
            "Upcoming" => return Some(upcoming(upcoming_days)),
            "Today" => "due:<=today is:open",
            "Overdue" => "is:overdue",
            "Doing" => "is:doing",
            "Waiting" => "is:blocked",
            "No project" => "-+",
            _ => return None,
        };
        Self::parse(text).ok()
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
