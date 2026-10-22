use super::model::{Alt, Field, Op, Query, StateTest, Term, Test, When};

/// The narrowing filter that the doing count of the status line selects.
pub const DOING: &str = "Doing";

/// Whether a term of names excludes them, and the names.
pub type Names<'a> = (bool, Vec<&'a [u8]>);

impl Query {
    /// The narrowing filters, in the order the Filters panel lists them:
    /// `Today`, `Overdue`, `Upcoming`, `Doing`, `Waiting` and
    /// `No project`. `upcoming_days` is how far `Upcoming` looks ahead.
    #[must_use]
    pub fn builtins(upcoming_days: u64) -> [(&'static str, Self); 6] {
        let text = |text| Self::parse(text).unwrap_or_default();
        [
            ("Today", text("due:today|completed:today")),
            ("Overdue", text("is:overdue")),
            ("Upcoming", upcoming(upcoming_days)),
            (DOING, text("is:doing")),
            ("Waiting", text("is:blocked")),
            ("No project", text("-+")),
        ]
    }

    /// The query of one project or context name and its subtree.
    pub fn name(kind: crate::todotxt::Kind, name: &[u8]) -> Self {
        Self::names(kind, [name], false)
    }

    /// The query of one term: any of `names` with their subtrees, or
    /// none of them when `negated`.
    pub fn names<'a>(
        kind: crate::todotxt::Kind,
        names: impl IntoIterator<Item = &'a [u8]>,
        negated: bool,
    ) -> Self {
        let alts = names
            .into_iter()
            .map(|n| super::atom::named(kind, n))
            .collect();
        Self {
            terms: vec![Term { negated, alts }],
            warnings: Vec::new(),
        }
    }

    /// The negation and the names of a query that [`Query::names`]
    /// could build for `kind`: one term whose alternatives are all named
    /// projects, or all named contexts.
    pub fn names_of(&self, kind: crate::todotxt::Kind) -> Option<Names<'_>> {
        let [term] = self.terms.as_slice() else {
            return None;
        };
        let names = term.alts.iter().map(|alt| {
            if let Test::Token { kind: k, name } = &alt.test
                && *k == kind
                && !name.is_empty()
            {
                Some(name.as_slice())
            } else {
                None
            }
        });
        let names = names.collect::<Option<_>>()?;
        Some((term.negated, names))
    }

    fn single(alt: Alt) -> Self {
        Self {
            terms: vec![Term {
                negated: false,
                alts: vec![alt],
            }],
            warnings: Vec::new(),
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
        due(Op::Ge, When::DaysAhead(0)),
        due(Op::Le, When::DaysAhead(days)),
        Query::single(Alt::new(Test::State(StateTest::Open))),
    ])
}
