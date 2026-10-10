use jiff::civil::Weekday;

use super::date::Date;
use crate::errs::{Code, Error, Field};
use crate::todotxt::RecUnit;

const MAX_OFFSET_DIGITS: usize = 4;

const ACCEPTED: [&str; 4] = [
    "today, tomorrow (tom), yesterday",
    "a weekday: monday … sunday, mon … sun",
    "an offset: 3d, +2w, -1m, 1y, 5b",
    "a date: YYYY-MM-DD",
];

/// The result of [`resolve`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    /// The empty input: the date is removed.
    Clear,
    /// The date the input names.
    On(Date),
}

/// A fuzzy date that has been read but not yet placed on the calendar.
///
/// Reading checks only the form: a keyword, weekday, offset or ISO date.
/// [`Fuzzy::on`] places it relative to a day and fails when the result
/// falls outside the years 0000 to 9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fuzzy {
    /// A fixed number of days from today: `today`, `tomorrow`, `yesterday`.
    Days(i32),
    /// The next day that is this weekday, never today.
    Weekday(Weekday),
    /// An offset such as `3d`, `-1m` or `5b`.
    Offset(i32, RecUnit),
    /// A written date.
    Exact(Date),
}

impl Fuzzy {
    /// Reads `input` as a fuzzy date; `None` when it is none of the forms.
    /// The empty input is not a form.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        let word = input.to_ascii_lowercase();
        if let Some(days) = keyword(&word) {
            return Some(Self::Days(days));
        }
        if let Some(target) = weekday(&word) {
            return Some(Self::Weekday(target));
        }
        if let Some((n, unit)) = offset(&word) {
            return Some(Self::Offset(n, unit));
        }
        Date::parse(input).map(Self::Exact)
    }

    /// The date this names when today is `today`, or `None` when it falls
    /// outside the years 0000 to 9999.
    pub(crate) fn on(self, today: Date) -> Option<Date> {
        match self {
            Self::Days(n) => today.add(n, RecUnit::Day),
            Self::Weekday(target) => today.after_weekday(target),
            Self::Offset(n, unit) => today.add(n, unit),
            Self::Exact(date) => Some(date),
        }
    }
}

/// Turns a fuzzy date (`fri`, `+3d`, `tomorrow`, `YYYY-MM-DD`) into a date
/// relative to `today`. The empty input clears the date.
pub(crate) fn resolve(input: &str, today: Date) -> Result<Resolved, Error> {
    if input.is_empty() {
        return Ok(Resolved::Clear);
    }
    Fuzzy::parse(input)
        .and_then(|fuzzy| fuzzy.on(today))
        .map(Resolved::On)
        .ok_or_else(|| invalid(input))
}

fn invalid(input: &str) -> Error {
    Error::new(Code::InvalidValue, format!("not a date: {input}"))
        .with_field("value", Field::Str(input.to_owned()))
        .with_field(
            "accepted",
            Field::Strs(ACCEPTED.iter().map(|s| (*s).to_owned()).collect()),
        )
}

fn keyword(word: &str) -> Option<i32> {
    match word {
        "today" => Some(0),
        "tom" | "tomorrow" => Some(1),
        "yesterday" => Some(-1),
        _ => None,
    }
}

fn weekday(word: &str) -> Option<Weekday> {
    match word {
        "monday" | "mon" => Some(Weekday::Monday),
        "tuesday" | "tue" => Some(Weekday::Tuesday),
        "wednesday" | "wed" => Some(Weekday::Wednesday),
        "thursday" | "thu" => Some(Weekday::Thursday),
        "friday" | "fri" => Some(Weekday::Friday),
        "saturday" | "sat" => Some(Weekday::Saturday),
        "sunday" | "sun" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn offset(word: &str) -> Option<(i32, RecUnit)> {
    let (sign, body) = match word.as_bytes().first()? {
        b'-' => (-1, &word[1..]),
        b'+' => (1, &word[1..]),
        _ => (1, word),
    };
    let (number, unit) = body.split_at_checked(body.len().checked_sub(1)?)?;
    if number.is_empty() || number.len() > MAX_OFFSET_DIGITS {
        return None;
    }
    if !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let unit = match unit {
        "d" => RecUnit::Day,
        "w" => RecUnit::Week,
        "m" => RecUnit::Month,
        "y" => RecUnit::Year,
        "b" => RecUnit::Business,
        _ => return None,
    };
    Some((sign * number.parse::<i32>().ok()?, unit))
}
