use super::{
    model::{Alt, Category, Field, Op, StateTest, Test, When, Word},
    parse::invalid,
};
use crate::{dates::Fuzzy, errs::Error, todotxt::Kind};

const OPERATORS: [(&str, Op); 5] = [
    ("<=", Op::Le),
    (">=", Op::Ge),
    ("<", Op::Lt),
    (">", Op::Gt),
    ("=", Op::Eq),
];

/// An `is:` form, its test and the category it belongs to.
type StateForm = (&'static str, StateTest, Option<Category>);

const STATES: [StateForm; 9] = [
    ("is:open", StateTest::Open, None),
    ("is:done", StateTest::Done, Some(Category::Done)),
    ("is:doing", StateTest::Doing, None),
    ("is:blocked", StateTest::Blocked, Some(Category::Blocked)),
    ("is:later", StateTest::Later, Some(Category::Later)),
    ("is:overdue", StateTest::Overdue, None),
    ("is:archived", StateTest::Archived, Some(Category::Archived)),
    ("is:dangling", StateTest::Dangling, None),
    ("is:stale", StateTest::Stale, None),
];

/// The words after `is:` that name a state, in table order.
pub fn state_words() -> impl Iterator<Item = &'static str> {
    STATES.iter().filter_map(|s| s.0.strip_prefix("is:"))
}

const PRIORITY_FORMS: [&str; 8] = [
    "pri:", "pri:B", "pri:A-C", "pri:<=B", "pri:<B", "pri:>=B", "pri:>B",
    "pri:=B",
];

const DATE_FORMS: [&str; 3] = [
    "<field>:<date>, <field>:<op><date>",
    "operators: < <= > >= =",
    "a date: today, fri, +3d, -7d, YYYY-MM-DD",
];

/// Turns one atom (not a phrase) into an alternative. `text` is not empty.
pub(super) fn compile(text: &str) -> Result<Alt, Error> {
    if let Some(alt) = token(text) {
        return Ok(alt);
    }
    let Some((key, value)) = tag_form(text) else {
        return Ok(word(text));
    };
    match key {
        "is" => state(text, value),
        "pri" => priority(text, value),
        _ => match field(key) {
            Some(field) => date(text, field, value),
            None => Ok(tag(key, value)),
        },
    }
}

/// An alternative that tests `text` as literal text.
pub(super) fn word(text: &str) -> Alt {
    Alt::new(Test::Word(Word::new(text)))
}

/// An alternative for a project or context and everything below it.
pub(super) fn named(kind: Kind, name: &[u8]) -> Alt {
    Alt::new(Test::Token {
        kind,
        name: name.to_vec(),
    })
}

fn token(text: &str) -> Option<Alt> {
    let (kind, name) = if let Some(name) = text.strip_prefix('+') {
        (Kind::Project, name)
    } else {
        (Kind::Context, text.strip_prefix('@')?)
    };
    Some(named(kind, name.as_bytes()))
}

/// The key and value of an atom with exactly one colon and a key.
fn tag_form(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    if colon == 0 || text.matches(':').count() != 1 {
        return None;
    }
    Some((text.get(..colon)?, text.get(colon + 1..)?))
}

fn tag(key: &str, value: &str) -> Alt {
    Alt::new(Test::Tag {
        key: key.as_bytes().to_vec(),
        value: value.as_bytes().to_vec(),
    })
}

fn cut_op(form: &str) -> (Op, &str) {
    for (text, op) in OPERATORS {
        if let Some(rest) = form.strip_prefix(text) {
            return (op, rest);
        }
    }
    (Op::Eq, form)
}

fn state(text: &str, word: &str) -> Result<Alt, Error> {
    let found = STATES
        .iter()
        .find(|s| s.0.strip_prefix("is:") == Some(word));
    let Some(&(_, which, category)) = found else {
        let forms = STATES.map(|s| s.0);
        return Err(invalid(text, "is not a state", &forms));
    };
    Ok(Alt {
        test: Test::State(which),
        category,
    })
}

fn priority(text: &str, form: &str) -> Result<Alt, Error> {
    match priority_range(form) {
        Some((lo, hi)) => Ok(Alt::new(Test::Priority { lo, hi })),
        None => Err(invalid(text, "is not a priority", &PRIORITY_FORMS)),
    }
}

/// The letters a priority form covers, or `None` when it is not one.
fn priority_range(form: &str) -> Option<(u8, u8)> {
    if form.is_empty() {
        return Some((b'A', b'Z'));
    }
    if let &[lo, b'-', hi] = form.as_bytes() {
        return (lo.is_ascii_uppercase()
            && hi.is_ascii_uppercase()
            && lo <= hi)
            .then_some((lo, hi));
    }
    let (op, rest) = cut_op(form);
    let &[c] = rest.as_bytes() else {
        return None;
    };
    c.is_ascii_uppercase().then(|| op_range(op, c))
}

fn op_range(op: Op, c: u8) -> (u8, u8) {
    match op {
        Op::Le => (b'A', c),
        Op::Lt => (b'A', c - 1),
        Op::Ge => (c, b'Z'),
        Op::Gt => (c + 1, b'Z'),
        Op::Eq => (c, c),
    }
}

fn field(key: &str) -> Option<Field> {
    match key {
        "due" => Some(Field::Due),
        "t" => Some(Field::Threshold),
        "started" => Some(Field::Started),
        "created" => Some(Field::Created),
        "completed" => Some(Field::Completed),
        _ => None,
    }
}

fn date(text: &str, field: Field, form: &str) -> Result<Alt, Error> {
    if form.is_empty() {
        return Ok(Alt::new(Test::HasDate(field)));
    }
    let (op, value) = cut_op(form);
    if value.is_empty() {
        let problem = "has no date after the operator";
        return Err(invalid(text, problem, &DATE_FORMS));
    }
    let Some(fuzzy) = Fuzzy::parse(value) else {
        return Err(invalid(text, "is not a date", &DATE_FORMS));
    };
    Ok(Alt::new(Test::Date {
        field,
        op,
        when: When::Fuzzy(fuzzy),
    }))
}
