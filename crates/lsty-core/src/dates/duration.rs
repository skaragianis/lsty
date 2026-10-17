use std::time::Duration;

use crate::errs::Error;

const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;

/// Reads a positive sum of numbers with units `ms`, `s`, `m`, `h` or `d`,
/// such as `1h30m`. A sum beyond 292 years is an error.
pub fn parse_duration(s: &str) -> Result<Duration, Error> {
    let mut total: u64 = 0;
    let mut rest = s;
    while !rest.is_empty() {
        let (part, next) = next_part(rest).ok_or_else(|| invalid(s))?;
        total = total.checked_add(part).ok_or_else(|| invalid(s))?;
        rest = next;
    }
    if total == 0 || i64::try_from(total).is_err() {
        return Err(invalid(s));
    }
    Ok(Duration::from_nanos(total))
}

/// One `number unit` part in nanoseconds, and what follows it.
fn next_part(s: &str) -> Option<(u64, &str)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let (number, tail) = s.split_at(digits);
    let letters = tail.bytes().take_while(u8::is_ascii_lowercase).count();
    let (unit, rest) = tail.split_at(letters);
    let nanos: u64 = match unit {
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60_000_000_000,
        "h" => 3_600_000_000_000,
        "d" => 86_400_000_000_000,
        _ => return None,
    };
    let part = number.parse::<u64>().ok()?.checked_mul(nanos)?;
    Some((part, rest))
}

fn invalid(s: &str) -> Error {
    let accepted = ["a number and a unit: 2s, 500ms, 4h, 2d, 1h30m"];
    Error::invalid_value(format!("not a duration: {s}"), s, &accepted)
}

/// Formats `d` as whole minutes, hours or days, rounded down.
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
