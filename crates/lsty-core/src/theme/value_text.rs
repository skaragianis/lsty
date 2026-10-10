//! A theme file's value as a warning shows it, in plain text without
//! quoting: `1.5`, `[1 a [true]]`, `map[x:1]`.

use toml::de::DeValue;

/// The text of `value`. Local dates and times are shown as if the local
/// zone were UTC.
pub(super) fn value_text(value: &DeValue<'_>) -> String {
    match value {
        DeValue::String(s) => s.to_string(),
        DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix())
            .map_or_else(|_| i.as_str().to_owned(), |n| n.to_string()),
        DeValue::Float(f) => f
            .as_str()
            .parse::<f64>()
            .map_or_else(|_| f.as_str().to_owned(), float_text),
        DeValue::Boolean(b) => b.to_string(),
        DeValue::Datetime(d) => datetime_text(&d.to_string()),
        DeValue::Array(items) => {
            let parts: Vec<String> =
                items.iter().map(|v| value_text(v.get_ref())).collect();
            format!("[{}]", parts.join(" "))
        }
        DeValue::Table(table) => {
            let mut parts: Vec<(String, String)> = table
                .iter()
                .map(|(k, v)| {
                    (k.get_ref().to_string(), value_text(v.get_ref()))
                })
                .collect();
            parts.sort();
            let parts: Vec<String> =
                parts.into_iter().map(|(k, v)| format!("{k}:{v}")).collect();
            format!("map[{}]", parts.join(" "))
        }
    }
}

/// A float in the shortest form that reads back as the same number:
/// plain digits, or an exponent below `1e-4` and from `1e6` up.
fn float_text(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        let sign = if value < 0.0 { '-' } else { '+' };
        return format!("{sign}Inf");
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or_default();
    let digits: String =
        mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i32 = exponent.parse().unwrap_or_default();
    if value == 0.0 {
        return format!("{sign}0");
    }
    let body = if (-4..6).contains(&exponent) {
        plain_form(&digits, exponent)
    } else {
        exponent_form(&digits, exponent)
    };
    format!("{sign}{body}")
}

fn exponent_form(digits: &str, exponent: i32) -> String {
    let (first, rest) = digits.split_at(1);
    let point = if rest.is_empty() {
        String::new()
    } else {
        format!(".{rest}")
    };
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{first}{point}e{sign}{:02}", exponent.abs())
}

fn plain_form(digits: &str, exponent: i32) -> String {
    match usize::try_from(exponent + 1) {
        Ok(0) | Err(_) => {
            let zeros = "0".repeat((exponent + 1..0).len());
            format!("0.{zeros}{digits}")
        }
        Ok(before) if before >= digits.len() => {
            format!("{digits}{}", "0".repeat(before - digits.len()))
        }
        Ok(before) => {
            let (whole, fraction) = digits.split_at(before);
            format!("{whole}.{fraction}")
        }
    }
}

/// A date, time or both as TOML writes it (`1979-05-27T07:32:00-08:00`),
/// as a warning shows it: date, time, offset and zone name.
fn datetime_text(toml: &str) -> String {
    let (date, rest, kind) = match toml.split_once('T') {
        Some((date, rest)) => (date, rest, ""),
        None if toml.contains(':') => ("0000-01-01", toml, "time-local"),
        None => (toml, "00:00:00", "date-local"),
    };
    let (clock, zone) = split_offset(rest);
    let clock = trim_fraction(clock);
    let zone = match (zone, kind) {
        (Some("Z"), _) => "+0000 UTC".to_owned(),
        (Some(offset), _) => {
            let compact = offset.replace(':', "");
            format!("{compact} {compact}")
        }
        (None, "") => "+0000 datetime-local".to_owned(),
        (None, local) => format!("+0000 {local}"),
    };
    format!("{date} {clock} {zone}")
}

/// A time of day and the offset that follows it, if any.
fn split_offset(text: &str) -> (&str, Option<&str>) {
    if let Some(clock) = text.strip_suffix('Z') {
        return (clock, Some("Z"));
    }
    match text.rfind(['+', '-']) {
        Some(at) => (&text[..at], Some(&text[at..])),
        None => (text, None),
    }
}

/// `07:32:00.500` as `07:32:00.5`, and `07:32:00.0` as `07:32:00`.
fn trim_fraction(clock: &str) -> String {
    match clock.split_once('.') {
        Some((whole, fraction)) => {
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                whole.to_owned()
            } else {
                format!("{whole}.{fraction}")
            }
        }
        None => clock.to_owned(),
    }
}
