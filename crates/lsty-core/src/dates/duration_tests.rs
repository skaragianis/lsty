use std::time::Duration;

use jiff::SignedDuration;

use super::{format_elapsed, parse_duration};
use crate::errs::Code;

#[test]
fn parse_rows() {
    let rows = [
        ("2s", Duration::from_secs(2)),
        ("500ms", Duration::from_millis(500)),
        ("4h", Duration::from_hours(4)),
        ("2d", Duration::from_hours(48)),
        ("1h30m", Duration::from_mins(90)),
        ("1d1h1m", Duration::from_mins(1501)),
    ];
    for (input, want) in rows {
        assert_eq!(parse_duration(input).ok(), Some(want), "{input}");
    }
}

#[test]
fn parse_invalid_rows() {
    let inputs = [
        "0s",
        "-1s",
        "5",
        "1w",
        "",
        "s",
        "1.5h",
        "1h30",
        "1h 30m",
        "1H",
        "999999999999d",
        "106751d106751d",
        "0h0m",
        "+1s",
        "h1",
        "99999999999999999999s",
    ];
    for input in inputs {
        let Err(err) = parse_duration(input) else {
            panic!("{input:?} parsed");
        };
        assert_eq!(err.code(), Code::InvalidValue, "{input:?}");
    }
}

#[test]
fn format_elapsed_rows() {
    let rows = [
        (SignedDuration::from_secs(30), "0m"),
        (SignedDuration::from_mins(12), "12m"),
        (SignedDuration::from_mins(119), "1h"),
        (SignedDuration::from_hours(47), "1d"),
        (SignedDuration::from_hours(49), "2d"),
        (SignedDuration::from_secs(59 * 60 + 59), "59m"),
        (SignedDuration::from_hours(1), "1h"),
        (SignedDuration::from_hours(24), "1d"),
        (SignedDuration::from_mins(-1), "0m"),
    ];
    for (input, want) in rows {
        assert_eq!(format_elapsed(input), want, "{input:?}");
    }
}
