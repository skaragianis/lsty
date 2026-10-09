//! How `:set NAME?` shows an option.

use std::time::Duration;

use lsty_core::config::{Config, LineNumbers};

use crate::setform::NO;

/// Whether `nu` and `rnu` are on for the line numbers `mode`.
pub(crate) const fn number_flags(mode: LineNumbers) -> (bool, bool) {
    match mode {
        LineNumbers::Hybrid => (true, true),
        LineNumbers::Absolute => (true, false),
        LineNumbers::Relative => (false, true),
        LineNumbers::Off => (false, false),
    }
}

/// The line numbers mode that shows `nu` and `rnu` as given.
pub(crate) const fn number_mode(nu: bool, rnu: bool) -> LineNumbers {
    match (nu, rnu) {
        (true, true) => LineNumbers::Hybrid,
        (true, false) => LineNumbers::Absolute,
        (false, true) => LineNumbers::Relative,
        (false, false) => LineNumbers::Off,
    }
}

fn flag(name: &str, on: bool) -> String {
    if on {
        name.to_owned()
    } else {
        format!("{NO}{name}")
    }
}

/// What `:set NAME?` says about the option `name`.
pub(crate) fn value_text(cfg: &Config, name: &str) -> String {
    let (nu, rnu) = number_flags(cfg.line_numbers);
    let value = match name {
        "nu" => return flag(name, nu),
        "rnu" => return flag(name, rnu),
        "show_sidebar" => return flag(name, cfg.show_sidebar),
        "show_detail" => return flag(name, cfg.show_detail),
        "auto_archive" => return flag(name, cfg.auto_archive),
        "keep_priority" => return flag(name, cfg.keep_priority),
        "keep_adding" => return flag(name, cfg.keep_adding),
        "include_creation" => return flag(name, cfg.include_creation),
        "priority_range" => {
            char::from(cfg.priority_range.letter()).to_string()
        }
        "default_priority" => cfg
            .default_priority
            .map(|p| char::from(p.letter()).to_string())
            .unwrap_or_default(),
        "upcoming_days" => cfg.upcoming_days.to_string(),
        "line_numbers" => cfg.line_numbers.as_str().to_owned(),
        "claim_stale_after" => duration_text(cfg.claim_stale_after),
        "lock_timeout" => duration_text(cfg.lock_timeout),
        "sidebar_min_width" => cfg.sidebar_min_width.to_string(),
        "detail_min_width" => cfg.detail_min_width.to_string(),
        "hidden_items" => cfg
            .hidden_items
            .iter()
            .map(|h| h.as_str())
            .collect::<Vec<_>>()
            .join(","),
        _ => String::new(),
    };
    format!("{name}={value}")
}

/// A duration as a short text: `2s`, `1m30s`, `4h`.
pub(crate) fn duration_text(d: Duration) -> String {
    let mut text = precise(d);
    if text.ends_with("m0s") {
        text.truncate(text.len() - 2);
    }
    if text.ends_with("h0m") {
        text.truncate(text.len() - 2);
    }
    text
}

/// The text of `d` in hours, minutes and seconds, or in a smaller unit
/// when it is under a second.
fn precise(d: Duration) -> String {
    let secs = d.as_secs();
    if secs == 0 {
        return small(d.subsec_nanos());
    }
    let nanos = u64::from(d.subsec_nanos());
    let (hours, minutes, seconds) = (secs / 3600, secs / 60 % 60, secs % 60);
    let mut out = String::new();
    if hours > 0 {
        out += &hours.to_string();
        out += "h";
    }
    if hours > 0 || minutes > 0 {
        out += &minutes.to_string();
        out += "m";
    }
    out += &with_fraction(seconds, nanos, 9);
    out += "s";
    out
}

/// A time under a second, in the largest unit it fills.
fn small(nanos: u32) -> String {
    let nanos = u64::from(nanos);
    match nanos {
        0 => "0s".to_owned(),
        1..1_000 => format!("{nanos}ns"),
        1_000..1_000_000 => {
            format!("{}µs", with_fraction(nanos / 1_000, nanos % 1_000, 3))
        }
        _ => format!(
            "{}ms",
            with_fraction(nanos / 1_000_000, nanos % 1_000_000, 6)
        ),
    }
}

/// `whole` and the `digits` digits of `frac`, without trailing zeros.
fn with_fraction(whole: u64, frac: u64, digits: usize) -> String {
    if frac == 0 {
        return whole.to_string();
    }
    let decimals = format!("{frac:0digits$}");
    format!("{whole}.{}", decimals.trim_end_matches('0'))
}
