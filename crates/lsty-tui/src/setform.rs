//! The words of `:set`: which options there are and how a word is taken
//! apart.

/// What kind of value an option takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// On or off.
    Flag,
    /// One value.
    Value,
    /// One value that is a number when it reads as one.
    Number,
    /// A list.
    List,
}

/// The options `:set` knows, with their kinds.
pub(crate) const OPTIONS: [(&str, Kind); 23] = [
    ("nu", Kind::Flag),
    ("rnu", Kind::Flag),
    ("show_sidebar", Kind::Flag),
    ("show_detail", Kind::Flag),
    ("inline_projects", Kind::Flag),
    ("inline_contexts", Kind::Flag),
    ("show_ids", Kind::Flag),
    ("auto_archive", Kind::Flag),
    ("keep_priority", Kind::Flag),
    ("keep_adding", Kind::Flag),
    ("inherit_project", Kind::Flag),
    ("mouse", Kind::Flag),
    ("include_creation", Kind::Flag),
    ("priority_range", Kind::Value),
    ("default_priority", Kind::Value),
    ("upcoming_days", Kind::Number),
    ("line_numbers", Kind::Value),
    ("detail_position", Kind::Value),
    ("claim_stale_after", Kind::Value),
    ("lock_timeout", Kind::Value),
    ("sidebar_min_width", Kind::Number),
    ("detail_min_width", Kind::Number),
    ("hidden_items", Kind::List),
];

/// The prefix of the word that switches a flag off.
pub(crate) const NO: &str = "no";

/// How a word of `:set` acts on its option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// The name alone: a flag on.
    On,
    /// `noNAME`.
    Off,
    /// `NAME=VALUE`.
    Assign,
    /// `NAME+=VALUE`.
    Add,
    /// `NAME-=VALUE`.
    Remove,
    /// `NAME?`.
    Query,
}

/// One word of `:set`, taken apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Form {
    pub(crate) name: String,
    pub(crate) op: Op,
    pub(crate) value: String,
}

impl Form {
    fn bare(name: &str, op: Op) -> Self {
        Self {
            name: name.to_owned(),
            op,
            value: String::new(),
        }
    }
}

/// The kind of the option called `name`, if it is one of the options.
pub(crate) fn kind_of(name: &str) -> Option<Kind> {
    OPTIONS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

/// Takes `word` apart.
pub(crate) fn parse(word: &str) -> Form {
    if let Some(name) = word.strip_suffix('?') {
        return Form::bare(name, Op::Query);
    }
    if let Some(at) = word.find('=').filter(|at| *at > 0) {
        return assignment(word, at);
    }
    if let Some(rest) = word.strip_prefix(NO)
        && kind_of(word).is_none()
        && kind_of(rest) == Some(Kind::Flag)
    {
        return Form::bare(rest, Op::Off);
    }
    Form::bare(word, Op::On)
}

/// `NAME=VALUE`, `NAME+=VALUE` or `NAME-=VALUE`, the `=` being at `at`.
fn assignment(word: &str, at: usize) -> Form {
    let value = word.get(at + 1..).unwrap_or_default().to_owned();
    let (name, op) = match word.as_bytes().get(at - 1) {
        Some(b'+') => (word.get(..at - 1), Op::Add),
        Some(b'-') => (word.get(..at - 1), Op::Remove),
        _ => (word.get(..at), Op::Assign),
    };
    Form {
        name: name.unwrap_or_default().to_owned(),
        op,
        value,
    }
}

/// The words `:set` completes: the options, and the `no` forms of the
/// flags.
pub(crate) fn names() -> Vec<String> {
    let mut out: Vec<String> =
        OPTIONS.iter().map(|(n, _)| (*n).to_owned()).collect();
    out.extend(
        OPTIONS
            .iter()
            .filter(|(_, k)| *k == Kind::Flag)
            .map(|(n, _)| format!("{NO}{n}")),
    );
    out
}
