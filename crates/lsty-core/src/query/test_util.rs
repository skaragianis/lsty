use jiff::tz::TimeZone;

use super::{Context, Query};
use crate::dates::Date;
use crate::list::test_util::{snapshot, snapshot_of};

pub(super) fn date(y: u16, m: u8, d: u8) -> Date {
    Date::new(y, m, d).expect("date")
}

pub(super) fn parse(text: &str) -> Query {
    Query::parse(text).expect("parse")
}

/// Whether `query` matches the one item `line`, on 2026-09-29 in UTC.
pub(super) fn hits(query: &Query, line: &str) -> bool {
    hits_on(query, line, date(2026, 9, 29), &TimeZone::UTC)
}

pub(super) fn hits_on(
    query: &Query,
    line: &str,
    today: Date,
    zone: &TimeZone,
) -> bool {
    let snap = snapshot(&[line], &[]);
    let item = snap.items().first().expect("item");
    query.matches(&snap, item, &Context { today, zone })
}

/// Whether `query` matches a one-line todo file holding exactly `bytes`.
pub(super) fn hits_bytes(query: &Query, bytes: &[u8]) -> bool {
    let snap = snapshot_of(bytes);
    let item = snap.items().first().expect("item");
    let today = date(2026, 9, 29);
    query.matches(
        &snap,
        item,
        &Context {
            today,
            zone: &TimeZone::UTC,
        },
    )
}
