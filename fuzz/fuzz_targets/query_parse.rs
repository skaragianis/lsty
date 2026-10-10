//! Any text either parses or is refused as an invalid `filter` value, and
//! a parsed query can be evaluated.

#![no_main]

use std::{path::PathBuf, sync::OnceLock, time::Duration};

use jiff::tz::TimeZone;
use libfuzzer_sys::fuzz_target;
use lsty_core::{
    dates::Clock,
    discover::{FileState, Found, FoundBy},
    errs::Code,
    list::{Params, Snapshot},
    query::{Context, Query},
    todotxt::File,
};

const LINE: &[u8] =
    b"(A) 2026-09-22 Call +work @phone due:2026-10-02 started:2026-09-29T0800Z\n";

fn build() -> (Snapshot, Params) {
    let now = "2026-09-29T09:00Z".parse().expect("the instant is refused");
    let clock = Clock::new(&|_| None, now, TimeZone::UTC)
        .expect("the fixed clock is refused");
    let found = Found {
        todo: PathBuf::from("/d/todo.txt"),
        done: PathBuf::from("/d/done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    let params = Params {
        clock,
        stale_after: Duration::from_hours(4),
    };
    let snap =
        Snapshot::build(found, File::parse(LINE), File::parse(b""), &params);
    (snap, params)
}

fn matching(q: &Query, snap: &Snapshot, ctx: &Context<'_>) -> Vec<bool> {
    snap.items()
        .iter()
        .map(|i| q.matches(snap, i, ctx))
        .collect()
}

/// Text without a quote reads the same as its words given as arguments.
fn check_args(text: &str, q: &Query, snap: &Snapshot, ctx: &Context<'_>) {
    if text.contains('"') {
        return;
    }
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let from_args = Query::parse_args(&words).unwrap_or_else(|e| {
        panic!("parse accepted {text:?} but its arguments are refused: {e:?}")
    });
    assert_eq!(matching(q, snap, ctx), matching(&from_args, snap, ctx));
}

fn check(text: &str, (snap, params): &(Snapshot, Params)) {
    let ctx = Context::from_clock(&params.clock);
    match Query::parse(text) {
        Ok(q) => {
            matching(&q, snap, &ctx);
            q.overrides();
            check_args(text, &q, snap, &ctx);
        }
        Err(e) => {
            assert_eq!(e.code(), Code::InvalidValue);
            assert_eq!(e.text("field"), Some("filter"));
        }
    }
}

fn snapshot() -> &'static (Snapshot, Params) {
    static SNAPSHOT: OnceLock<(Snapshot, Params)> = OnceLock::new();
    SNAPSHOT.get_or_init(build)
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    check(text, snapshot());
    drop(Query::parse_args(&[text]));
});
