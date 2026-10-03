use std::time::Duration;

use crate::errs::{Code, Error, Field};

const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;

/// Reads a positive sum of numbers with units `ms`, `s`, `m`, `h` or `d`,
/// such as `1h30m`. A sum beyond 292 years is an error.
pub fn parse_duration(s: &str) -> Result<Duration, Error> {
    let mut total: u128 = 0;
    let mut rest = s;
    while !rest.is_empty() {
        let (part, next) = next_part(rest).ok_or_else(|| invalid(s))?;
        total += part;
        rest = next;
    }
    let limit = u128::try_from(i64::MAX).unwrap_or(u128::MAX);
    if total == 0 || total > limit {
        return Err(invalid(s));
    }
    let secs = u64::try_from(total / 1_000_000_000).map_err(|_| invalid(s))?;
    let nanos =
        u32::try_from(total % 1_000_000_000).map_err(|_| invalid(s))?;
    Ok(Duration::new(secs, nanos))
}

/// One `number unit` part in nanoseconds, and what follows it.
fn next_part(s: &str) -> Option<(u128, &str)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let (number, tail) = s.split_at(digits);
    let letters = tail.bytes().take_while(u8::is_ascii_lowercase).count();
    let (unit, rest) = tail.split_at(letters);
    let nanos: u128 = match unit {
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60_000_000_000,
        "h" => 3_600_000_000_000,
        "d" => 86_400_000_000_000,
        _ => return None,
    };
    let n: u64 = number.parse().ok()?;
    let part = u128::from(n) * nanos;
    // Each part must fit on its own, so the sum cannot overflow.
    (part <= u128::from(u64::MAX) * 1_000).then_some((part, rest))
}

fn invalid(s: &str) -> Error {
    Error::new(Code::InvalidValue, format!("not a duration: {s}"))
        .with_field("value", Field::Str(s.to_owned()))
        .with_field(
            "accepted",
            Field::Strs(vec![
                "a number and a unit: 2s, 500ms, 4h, 2d, 1h30m".to_owned(),
            ]),
        )
}

/// Formats `d` as whole minutes, hours or days, rounded down. The elapsed
/// time of an item is already a `Duration`, so it passes as it is.
pub fn format_elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= SECS_PER_DAY {
        format!("{}d", secs / SECS_PER_DAY)
    } else if secs >= SECS_PER_HOUR {
        format!("{}h", secs / SECS_PER_HOUR)
    } else {
        format!("{}m", secs / SECS_PER_MINUTE)
    }
}
