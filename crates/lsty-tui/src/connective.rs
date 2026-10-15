//! The names a panel filters by, each with the connective that joins its
//! term to the names chosen before it, and the filter text they read as.

use lsty_core::{errs::lossy, query::Query, todotxt::Kind};

/// How a name joins the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Conn {
    /// A term of its own: the task has to match it.
    And,
    /// An alternative in the term of the name before it.
    Or,
    /// A term of its own that the task must not match.
    Not,
}

impl Conn {
    /// What the panel shows beside a name: `>` for the first name when
    /// it is a plain term.
    const fn mark(self, first: bool) -> &'static str {
        match self {
            Self::And if first => ">",
            Self::And => "&",
            Self::Or => "|",
            Self::Not => "-",
        }
    }
}

/// A name that filters, with its connective.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Term {
    pub(crate) name: Vec<u8>,
    pub(crate) conn: Conn,
}

impl Term {
    pub(crate) fn and(name: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            conn: Conn::And,
        }
    }
}

/// The state that follows `now` for a name that is `first` or not: off,
/// then or (not for the first), and, not, and off again.
pub(crate) fn cycle(first: bool, now: Option<Conn>) -> Option<Conn> {
    match now {
        None if first => Some(Conn::And),
        None => Some(Conn::Or),
        Some(Conn::Or) => Some(Conn::And),
        Some(Conn::And) => Some(Conn::Not),
        Some(Conn::Not) => None,
    }
}

/// Takes `name` to its next state. A name that is added goes last. One
/// that is left out (always from `-`) hands its connective to the
/// alternative that joined it, so what was excluded stays excluded.
pub(crate) fn step(active: &mut Vec<Term>, name: &[u8]) {
    let at = active.iter().position(|t| t.name == name);
    let first = at.map_or(active.is_empty(), |i| i == 0);
    let next = cycle(first, at.map(|i| active[i].conn));
    match (at, next) {
        (Some(i), Some(conn)) => active[i].conn = conn,
        (Some(i), None) => {
            let gone = active.remove(i).conn;
            let joined = active.get_mut(i);
            if let Some(after) = joined.filter(|t| t.conn == Conn::Or) {
                after.conn = gone;
            }
        }
        (None, conn) => active.extend(conn.map(|conn| Term {
            name: name.to_vec(),
            conn,
        })),
    }
}

/// Makes `name` the only name, or none when it is the one: the way the
/// Filters panel chooses.
pub(crate) fn pick(active: &mut Vec<Term>, name: &[u8]) {
    let was = active.first().is_some_and(|t| t.name == name);
    *active = if was {
        Vec::new()
    } else {
        vec![Term::and(name)]
    };
}

/// Takes the term that starts at `at`, with its alternatives, out.
pub(crate) fn clear_term(active: &mut Vec<Term>, at: usize) {
    if at < active.len() {
        active.remove(at);
    }
    while active.get(at).is_some_and(|t| t.conn == Conn::Or) {
        active.remove(at);
    }
}

/// What the panel shows beside `name`, when it is among `active`.
pub(crate) fn mark(active: &[Term], name: &[u8]) -> Option<&'static str> {
    let at = active.iter().position(|t| t.name == name)?;
    Some(active[at].conn.mark(at == 0))
}

/// The names of `active` in groups, each with the place of its first
/// name: a name that is not an alternative starts a group, and the
/// alternatives after it join it.
fn groups(active: &[Term]) -> Vec<(usize, &[Term])> {
    let mut out: Vec<(usize, &[Term])> = Vec::new();
    for (i, term) in active.iter().enumerate() {
        match out.last_mut() {
            Some((at, group)) if term.conn == Conn::Or => {
                *group = &active[*at..=i];
            }
            _ => out.push((i, &active[i..=i])),
        }
    }
    out
}

/// The terms of the filter text the names read as, each with the place of
/// its first name: `@a|@b @c -@d`. It is for showing: a name with a `|`
/// in it reads as two.
pub(crate) fn terms(sigil: &str, active: &[Term]) -> Vec<(usize, String)> {
    let text = |group: &[Term]| {
        let names: Vec<String> = group
            .iter()
            .map(|t| format!("{sigil}{}", lossy(&t.name)))
            .collect();
        let minus = if group[0].conn == Conn::Not { "-" } else { "" };
        format!("{minus}{}", names.join("|"))
    };
    groups(active)
        .into_iter()
        .map(|(at, group)| (at, text(group)))
        .collect()
}

/// What the names of `active`, which are of `kind`, filter by: the
/// terms of `terms`, built from the names themselves.
pub(crate) fn query(kind: Kind, active: &[Term]) -> Query {
    Query::and(groups(active).into_iter().map(|(_, group)| {
        let names = group.iter().map(|t| t.name.as_slice());
        Query::names(kind, names, group[0].conn == Conn::Not)
    }))
}
