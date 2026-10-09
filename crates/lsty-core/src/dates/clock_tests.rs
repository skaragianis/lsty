use std::collections::HashMap;
use std::ffi::OsString;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{Clock, Date};
use crate::errs::{Code, Field};

type Vars<'a> = &'a [(&'a str, &'a str)];
type Row<'a> = (Vars<'a>, &'a str, Date, Timestamp, bool);

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).expect("zone in the bundled database")
}

fn at(s: &str) -> Timestamp {
    s.parse().expect("a timestamp")
}

fn clock(
    vars: Vars<'_>,
    real: Timestamp,
    zone: TimeZone,
) -> crate::errs::Result<Clock> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    Clock::new(&|k| map.get(k).map(OsString::from), real, zone)
}

fn d(y: u16, m: u8, day: u8) -> Date {
    Date::new(y, m, day).expect("a real date")
}

#[test]
fn new_clock_rows() {
    let real = at("2026-09-29T21:30:00Z");
    let sydney = "Australia/Sydney";
    let rows: [Row<'_>; 5] = [
        (&[], sydney, d(2026, 9, 30), real, false),
        (
            &[("LSTY_NOW", "2026-09-29T2230Z")],
            sydney,
            d(2026, 9, 30),
            at("2026-09-29T22:30:00Z"),
            true,
        ),
        (
            &[("LSTY_TODAY", "2026-09-29")],
            "America/New_York",
            d(2026, 9, 29),
            at("2026-09-29T16:00:00Z"),
            true,
        ),
        (
            &[
                ("LSTY_TODAY", "2026-09-30"),
                ("LSTY_NOW", "2026-09-29T2230Z"),
            ],
            sydney,
            d(2026, 9, 30),
            at("2026-09-29T22:30:00Z"),
            true,
        ),
        (
            &[("LSTY_NOW", ""), ("LSTY_TODAY", "")],
            sydney,
            d(2026, 9, 30),
            real,
            false,
        ),
    ];
    for (vars, name, today, now, fixed) in rows {
        let c = clock(vars, real, zone(name)).expect("a clock");
        assert_eq!(c.today(), today, "{vars:?}");
        assert_eq!(c.now(), now, "{vars:?}");
        assert_eq!(c.is_fixed(), fixed, "{vars:?}");
    }
}

#[test]
fn stamp_rows() {
    let c = clock(
        &[("LSTY_NOW", "2026-09-29T2230Z")],
        Timestamp::UNIX_EPOCH,
        zone("Australia/Sydney"),
    )
    .expect("a clock");
    assert_eq!(c.stamp(), "2026-09-29T2230Z");

    let c = clock(&[], at("2026-09-29T13:05:59.000000999Z"), TimeZone::UTC)
        .expect("a clock");
    assert_eq!(c.stamp(), "2026-09-29T1305Z");
}

#[test]
fn invalid_rows() {
    let rows: [(Vars<'_>, &str); 16] = [
        (&[("LSTY_NOW", "bad"), ("LSTY_TODAY", "bad")], "LSTY_TODAY"),
        (
            &[
                ("LSTY_TODAY", "2026-09-30"),
                ("LSTY_NOW", "2026-09-29T2230Z"),
            ],
            "LSTY_TODAY",
        ),
        (&[("LSTY_NOW", "2026-09-29T09:00Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29")], "LSTY_NOW"),
        (&[("LSTY_TODAY", "tomorrow")], "LSTY_TODAY"),
        (&[("LSTY_TODAY", "2026-02-30")], "LSTY_TODAY"),
        (&[("LSTY_NOW", "2026-09-29T930Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-9-29T0930Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29T2400Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29T0960Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-02-30T0900Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29T0900z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29T090000Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", "2026-09-29T0900.5Z")], "LSTY_NOW"),
        (&[("LSTY_NOW", " 2026-09-29T0900Z")], "LSTY_NOW"),
        (&[("LSTY_TODAY", "2026-09-29 ")], "LSTY_TODAY"),
    ];
    for (vars, field) in rows {
        let Err(err) = clock(vars, Timestamp::UNIX_EPOCH, TimeZone::UTC)
        else {
            panic!("{vars:?} accepted");
        };
        assert_eq!(err.code(), Code::InvalidValue, "{vars:?}");
        assert!(err.text("field") == Some(field), "{vars:?}: {err:?}");
    }
}

#[test]
fn disagreeing_values_name_today_in_utc() {
    let vars = [
        ("LSTY_TODAY", "2026-09-30"),
        ("LSTY_NOW", "2026-09-29T2230Z"),
    ];
    let Err(err) = clock(&vars, Timestamp::UNIX_EPOCH, TimeZone::UTC) else {
        panic!("accepted");
    };
    assert_eq!(err.code(), Code::InvalidValue);
}

#[test]
fn local_date_of_a_late_utc_instant() {
    let berlin = zone("Europe/Berlin");
    let rows = [
        ("2026-09-28T23:30:00Z", d(2026, 9, 29)),
        ("2026-09-28T10:00:00Z", d(2026, 9, 28)),
    ];
    for (instant, want) in rows {
        assert_eq!(Date::local(at(instant), &berlin), Some(want));
    }
}

const RANGE_NOW: &str = "an instant from 0000-01-01T0000Z to 9999-12-30T2200Z";
const RANGE_TODAY: &str =
    "a date whose 12:00 local time is at most 9999-12-30T2200Z";
const RANGE_LOCAL_DATE: &str =
    "an instant whose local date is in the years 0000 to 9999";

fn assert_out_of_range(
    vars: Vars<'_>,
    zone_name: &str,
    field: &str,
    value: &str,
    accepted: &str,
) {
    let Err(err) = clock(vars, Timestamp::UNIX_EPOCH, zone(zone_name)) else {
        panic!("{vars:?} accepted");
    };
    assert_eq!(err.code(), Code::InvalidValue, "{vars:?}");
    assert_eq!(err.message(), format!("{field} is out of range: {value}"));
    let fields: Vec<_> = err.fields().collect();
    assert_eq!(
        fields,
        [
            ("accepted", &Field::Strs(vec![accepted.to_owned()])),
            ("field", &Field::Str(field.to_owned())),
            ("value", &Field::Str(value.to_owned())),
        ],
        "{vars:?}"
    );
}

#[test]
fn local_date_outside_the_years_is_out_of_range() {
    let vars = [("LSTY_NOW", "0000-01-01T0100Z")];
    assert_out_of_range(
        &vars,
        "America/New_York",
        "LSTY_NOW",
        "0000-01-01T0100Z",
        RANGE_LOCAL_DATE,
    );
}

#[test]
fn unrepresentable_overrides_are_out_of_range() {
    let rows = [
        ("UTC", "LSTY_TODAY", "9999-12-31", RANGE_TODAY),
        ("Pacific/Pago_Pago", "LSTY_TODAY", "9999-12-30", RANGE_TODAY),
        ("UTC", "LSTY_NOW", "9999-12-31T1200Z", RANGE_NOW),
        ("UTC", "LSTY_NOW", "9999-12-30T2201Z", RANGE_NOW),
    ];
    for (zone_name, name, text, accepted) in rows {
        let vars = [(name, text)];
        assert_out_of_range(&vars, zone_name, name, text, accepted);
    }
}

#[test]
fn edge_values_are_still_accepted() {
    type Edge<'a> = (Vars<'a>, &'a str, Date);
    let rows: [Edge<'_>; 3] = [
        (&[("LSTY_NOW", "9999-12-30T2200Z")], "UTC", d(9999, 12, 30)),
        (
            &[("LSTY_TODAY", "9999-12-31")],
            "Pacific/Kiritimati",
            d(9999, 12, 31),
        ),
        (
            &[("LSTY_NOW", "9999-12-30T2100Z")],
            "Australia/Sydney",
            d(9999, 12, 31),
        ),
    ];
    for (vars, zone_name, today) in rows {
        let c = clock(vars, Timestamp::UNIX_EPOCH, zone(zone_name))
            .expect("a clock");
        assert_eq!(c.today(), today, "{vars:?}");
    }
}

#[test]
fn a_clock_override_that_is_not_utf8_is_invalid() {
    use std::os::unix::ffi::OsStringExt as _;
    for name in ["LSTY_TODAY", "LSTY_NOW"] {
        let bad = OsString::from_vec(vec![0xe9]);
        let var = |k: &str| (k == name).then(|| bad.clone());
        let err = Clock::new(&var, Timestamp::UNIX_EPOCH, TimeZone::UTC)
            .expect_err("a bad override");
        assert_eq!(err.code(), Code::InvalidValue, "{name}");
        assert_eq!(err.text("field"), Some(name));
    }
}
