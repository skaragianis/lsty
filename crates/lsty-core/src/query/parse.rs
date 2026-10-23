use super::{
    atom,
    model::{Alt, Query, Term},
};
use crate::errs::{Error, Field, quote};

const TERM_FORMS: [&str; 3] = [
    "a word: invoice, or a phrase: \"two words\"",
    "alternatives: +work|@phone",
    "negation of a whole term: -@waiting",
];

/// The hint of a term that starts or ends with a bar, which a space
/// around the bar most likely made.
const BAR_HINT: &str = "write a|b, without spaces around the bar";

/// The error of a term that is not valid: `invalid_value` for the field
/// `filter`.
pub(super) fn invalid(value: &str, problem: &str, accepted: &[&str]) -> Error {
    let message = format!("filter: {} {problem}", quote(value));
    Error::invalid_value(message, value, accepted)
        .with_field("field", Field::Str("filter".to_owned()))
}

impl Query {
    /// Parses a filter string.
    pub fn parse(s: &str) -> Result<Self, Error> {
        let texts = term_texts(s)?;
        let terms: Result<Vec<Term>, Error> =
            texts.iter().copied().map(parse_term).collect();
        Ok(Self {
            terms: terms?,
            warnings: operator_warnings(&texts),
        })
    }

    /// Parses a filter string into its terms, each as written with the
    /// query of that term alone.
    pub fn parse_terms(s: &str) -> crate::errs::Result<Vec<(&str, Self)>> {
        term_texts(s)?
            .into_iter()
            .map(|text| {
                let query = Self {
                    terms: vec![parse_term(text)?],
                    warnings: Vec::new(),
                };
                Ok((text, query))
            })
            .collect()
    }

    /// Parses command line arguments, one term each. An argument with a
    /// space is a phrase, and a leading `-` still negates it.
    pub fn parse_args<S: AsRef<str>>(args: &[S]) -> Result<Self, Error> {
        let terms: Result<Vec<Term>, Error> =
            args.iter().map(|a| arg_term(a.as_ref())).collect();
        Ok(Self {
            terms: terms?,
            warnings: operator_warnings(args),
        })
    }
}

/// The terms of a filter string as written.
fn term_texts(s: &str) -> Result<Vec<&str>, Error> {
    let (parts, closed) = split_outside(s, b' ');
    let texts: Vec<&str> =
        parts.into_iter().filter(|p| !p.is_empty()).collect();
    if !closed {
        let last = texts.last().copied().unwrap_or(s);
        let problem = "has a quote that is not closed";
        return Err(invalid(last, problem, &TERM_FORMS));
    }
    Ok(texts)
}

/// One warning for each term that is exactly `not`, `or` or
/// `and`, in any ASCII case: they are plain words, not operators.
fn operator_warnings<S: AsRef<str>>(terms: &[S]) -> Vec<String> {
    terms
        .iter()
        .filter_map(|term| {
            let word = term.as_ref();
            let (_, advice) = [
                ("not", "use --filter -a for not"),
                ("or", "use a|b for either, --filter -a for not"),
                ("and", "terms already all have to match"),
            ]
            .into_iter()
            .find(|(op, _)| word.eq_ignore_ascii_case(op))?;
            Some(format!(
                "filter: {} is read as a word; {advice}",
                quote(word)
            ))
        })
        .collect()
}

fn arg_term(arg: &str) -> Result<Term, Error> {
    if !arg.contains(' ') {
        return parse_term(arg);
    }
    let (negated, body) = cut_negation(arg);
    Ok(Term {
        negated,
        alts: vec![atom::word(body)],
    })
}

fn cut_negation(text: &str) -> (bool, &str) {
    match text.strip_prefix('-') {
        Some(body) => (true, body),
        None => (false, text),
    }
}

fn parse_term(text: &str) -> Result<Term, Error> {
    let (negated, body) = cut_negation(text);
    if body.is_empty() && negated {
        return Err(invalid(text, "negates nothing", &TERM_FORMS));
    }
    let (parts, _) = split_outside(body, b'|');
    let edge = parts.first() == Some(&"") || parts.last() == Some(&"");
    let alts: Result<Vec<Alt>, Error> = parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| parse_alt(part, text, i > 0, edge))
        .collect();
    Ok(Term {
        negated,
        alts: alts?,
    })
}

/// Parses an alternative of the term `text`, a `later` one or the
/// first; `edge` when the term starts or ends with a bar.
fn parse_alt(
    part: &str,
    text: &str,
    later: bool,
    edge: bool,
) -> Result<Alt, Error> {
    if part.is_empty() {
        let problem = "has an empty alternative";
        let hint = if edge { BAR_HINT } else { "" };
        return Err(invalid(text, problem, &TERM_FORMS).with_hint(hint));
    }
    if part.starts_with('"') {
        return parse_phrase(part, text);
    }
    if later && part.starts_with('-') {
        let problem = "negates inside an alternative";
        return Err(invalid(text, problem, &TERM_FORMS));
    }
    if part.contains('"') {
        let problem = "has a quote inside a word";
        return Err(invalid(text, problem, &TERM_FORMS));
    }
    atom::compile(part)
}

fn parse_phrase(part: &str, text: &str) -> Result<Alt, Error> {
    let rest = part.strip_prefix('"').unwrap_or(part);
    let problem = match rest.split_once('"') {
        None => "has a quote that is not closed",
        Some(("", _)) => "has an empty phrase",
        Some((inner, "")) => return Ok(atom::word(inner)),
        Some(_) => "has text after a phrase",
    };
    Err(invalid(text, problem, &TERM_FORMS))
}

/// Splits `s` at every `sep` that is outside quotes, and says whether
/// every quote was closed. `sep` is an ASCII byte.
fn split_outside(s: &str, sep: u8) -> (Vec<&str>, bool) {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, b) in s.bytes().enumerate() {
        if b == b'"' {
            quoted = !quoted;
        } else if b == sep && !quoted {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    (parts, !quoted)
}
