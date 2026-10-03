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
pub enum Resolved {
    /// The empty input: the date is removed.
    Clear,
    /// The date the input names.
    On(Date),
}

/// Turns a fuzzy date (`fri`, `+3d`, `tomorrow`, `YYYY-MM-DD`) into a date
/// relative to `today`. The empty input clears the date.
pub fn resolve(input: &str, today: Date) -> Result<Resolved, Error> {
    if input.is_empty() {
        return Ok(Resolved::Clear);
    }
    if let Some((n, unit)) = relative(&input.to_ascii_lowercase(), today) {
        return today
            .add(n, unit)
            .map(Resolved::On)
            .ok_or_else(|| invalid(input));
    }
    Date::parse(input)
        .map(Resolved::On)
        .ok_or_else(|| invalid(input))
}

fn invalid(input: &str) -> Error {
    Error::new(Code::InvalidValue, format!("not a date: {input}"))
        .field("value", Field::Str(input.to_owned()))
        .field(
            "accepted",
            Field::Strs(ACCEPTED.iter().map(|s| (*s).to_owned()).collect()),
        )
}

/// The move from today that a keyword, weekday or offset names, or `None`
/// when `word` is none of them.
fn relative(word: &str, today: Date) -> Option<(i32, RecUnit)> {
    if let Some(days) = keyword(word) {
        return Some((days, RecUnit::Day));
    }
    if let Some(target) = weekday(word) {
        let ahead = (target - today.weekday_from_monday() + 7) % 7;
        return Some((if ahead == 0 { 7 } else { ahead }, RecUnit::Day));
    }
    offset(word)
}

fn keyword(word: &str) -> Option<i32> {
    match word {
        "today" => Some(0),
        "tom" | "tomorrow" => Some(1),
        "yesterday" => Some(-1),
        _ => None,
    }
}

/// The weekday as days since Monday.
fn weekday(word: &str) -> Option<i32> {
    match word {
        "monday" | "mon" => Some(0),
        "tuesday" | "tue" => Some(1),
        "wednesday" | "wed" => Some(2),
        "thursday" | "thu" => Some(3),
        "friday" | "fri" => Some(4),
        "saturday" | "sat" => Some(5),
        "sunday" | "sun" => Some(6),
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
