use super::Date;
use crate::todotxt::RecUnit;

fn d(y: u16, m: u8, day: u8) -> Date {
    Date::new(y, m, day).expect("a real date")
}

#[test]
fn parse_date_rows() {
    let rows: [(&str, Option<Date>); 11] = [
        ("2026-10-10", Date::new(2026, 10, 10)),
        ("2028-02-29", Date::new(2028, 2, 29)),
        ("2026-02-29", None),
        ("26-10-10", None),
        ("2026-1-10", None),
        ("+026-10-10", None),
        ("", None),
        ("2026-13-01", None),
        ("2026-10-00", None),
        ("0000-01-01", Date::new(0, 1, 1)),
        ("９999-01-01", None),
    ];
    for (input, want) in rows {
        assert_eq!(Date::parse(input), want, "{input:?}");
    }
}

#[test]
fn add_rows() {
    use RecUnit::{Business, Day, Month, Week, Year};
    let rows = [
        (d(2026, 1, 31), 1, Month, d(2026, 2, 28)),
        (d(2026, 2, 28), 1, Month, d(2026, 3, 28)),
        (d(2028, 1, 31), 1, Month, d(2028, 2, 29)),
        (d(2026, 1, 15), -2, Month, d(2025, 11, 15)),
        (d(2026, 1, 31), -1, Month, d(2025, 12, 31)),
        (d(2026, 12, 31), 2, Month, d(2027, 2, 28)),
        (d(2028, 2, 29), 1, Year, d(2029, 2, 28)),
        (d(2028, 2, 29), 4, Year, d(2032, 2, 29)),
        (d(2026, 9, 29), 2, Week, d(2026, 10, 13)),
        (d(2026, 9, 29), -3, Day, d(2026, 9, 26)),
        (d(2026, 10, 3), 0, Business, d(2026, 10, 3)),
        (d(2026, 10, 3), 0, Day, d(2026, 10, 3)),
        (d(2026, 10, 5), -1, Business, d(2026, 10, 2)),
        (d(2026, 10, 3), 1, Business, d(2026, 10, 5)),
        (d(2026, 9, 29), 10, Business, d(2026, 10, 13)),
    ];
    for (from, n, unit, want) in rows {
        assert_eq!(from.add(n, unit), Some(want), "{from} {n} {unit:?}");
    }
}

#[test]
fn add_twice_does_not_remember_a_clamp() {
    let once = d(2026, 1, 31).add(1, RecUnit::Month);
    assert_eq!(
        once.and_then(|x| x.add(1, RecUnit::Month)),
        Some(d(2026, 3, 28))
    );
}

#[test]
fn add_out_of_range_is_none() {
    assert_eq!(d(9999, 12, 31).add(1, RecUnit::Day), None);
    assert_eq!(d(0, 1, 1).add(-1, RecUnit::Day), None);
    assert_eq!(d(9999, 12, 31).add(1, RecUnit::Business), None);
    assert_eq!(d(9999, 12, 30).add(i32::MAX, RecUnit::Business), None);
    assert_eq!(d(0, 1, 3).add(i32::MIN, RecUnit::Business), None);
    assert_eq!(d(2026, 1, 1).add(i32::MIN, RecUnit::Year), None);
    assert_eq!(d(2026, 1, 1).add(i32::MAX, RecUnit::Month), None);
    assert_eq!(d(2026, 1, 1).add(-30_000, RecUnit::Month), None);
}

#[test]
fn add_years_and_months_out_of_range_is_none() {
    assert_eq!(d(2026, 9, 29).add(9999, RecUnit::Year), None);
    assert_eq!(d(2026, 9, 29).add(-2027, RecUnit::Year), None);
    assert_eq!(d(9999, 1, 1).add(1, RecUnit::Year), None);
    assert_eq!(d(9999, 12, 1).add(1, RecUnit::Month), None);
    assert_eq!(d(0, 6, 1).add(-1, RecUnit::Year), None);
    assert_eq!(d(0, 1, 1).add(-1, RecUnit::Month), None);
}

#[test]
fn add_reaches_the_range_edges() {
    assert_eq!(d(0, 1, 1).add(9999, RecUnit::Year), Some(d(9999, 1, 1)));
    assert_eq!(d(2026, 9, 29).add(-2001, RecUnit::Year), Some(d(25, 9, 29)));
    assert_eq!(d(9998, 12, 31).add(1, RecUnit::Year), Some(d(9999, 12, 31)));
    assert_eq!(d(1, 1, 1).add(-1, RecUnit::Year), Some(d(0, 1, 1)));
    assert_eq!(d(0, 2, 1).add(-1, RecUnit::Month), Some(d(0, 1, 1)));
}

#[test]
fn display_pads() {
    assert_eq!(d(2026, 1, 5).to_string(), "2026-01-05");
    assert_eq!(d(7, 1, 5).to_string(), "0007-01-05");
}

#[test]
fn days_until_rows() {
    let rows = [
        (d(2026, 1, 31), d(2026, 2, 28), 28),
        (d(2026, 2, 28), d(2026, 1, 31), -28),
        (d(2026, 3, 1), d(2026, 3, 1), 0),
        (d(2028, 2, 28), d(2028, 3, 1), 2),
        (d(2026, 1, 1), d(2027, 1, 1), 365),
        (d(2026, 1, 1), d(2400, 1, 1), 136_600),
        (d(2026, 1, 1), d(9999, 12, 31), 2_912_442),
    ];
    for (a, b, want) in rows {
        assert_eq!(a.days_until(b), want, "{a} {b}");
    }
}

#[test]
fn new_rejects_unreal_dates() {
    assert_eq!(Date::new(2026, 2, 30), None);
    assert_eq!(Date::new(10_000, 1, 1), None);
    assert_eq!(Date::new(2026, 0, 1), None);
}
