//! `lsty next`: the item an agent works on next.

use std::os::unix::ffi::OsStrExt as _;

use lsty_core::{
    errs::Result,
    links::linked_files,
    list::{Item, Snapshot, SortField},
    ops::{self, NextParams, Skipped},
    theme::Slot,
};

use crate::{
    commands::Call,
    output::{Field, Json, Output, ref_json},
    parse::{Opt, usage},
    read::{list_line, text_lines},
    reflist::without_args,
};

/// `lsty next`: shows the first ready item, or with `--start` claims it.
pub(crate) fn next(call: &Call<'_>) -> Result<Output> {
    without_args(call)?;
    let params = NextParams {
        filter: call.parse_query(&[])?,
        sort: call
            .option(Opt::Sort)
            .and_then(SortField::parse)
            .unwrap_or(SortField::Priority),
    };
    if call.has(Opt::Start) {
        return start(call, &params);
    }
    if call.has(Opt::PrintId) {
        return Err(usage("--print-id needs --start"));
    }
    let reading = ops::next_read(&call.ops, &params)?;
    let snap = &reading.snapshot;
    let item = reading.item.and_then(|id| snap.item(id));
    let fields = if call.json() {
        let mut fields = vec![Field::new(
            "items",
            call.item_json(snap).objects(item.as_slice()),
        )];
        fields.extend(seen_fields(&reading.seen));
        fields.extend([
            Field::array(
                "messages",
                reading.messages.iter().map(|m| Json::bytes(m)),
            ),
            Field::bool("resumed", reading.seen.resumed),
        ]);
        fields
    } else {
        Vec::new()
    };
    let mut lines = item_lines(call, snap, item.into_iter());
    lines.extend(text_lines(&call.paint, reading.messages.iter()));
    Ok(Output {
        fields,
        lines,
        ..Output::default()
    })
}

/// `next --start`: claims the first ready item.
fn start(call: &Call<'_>, params: &NextParams) -> Result<Output> {
    let started = ops::next_start(&call.ops, params)?;
    let seen = &started.seen;
    let [stale, later, blocked, claimed] = seen_fields(seen);
    let tail = [
        later,
        blocked,
        claimed,
        Field::bool("resumed", seen.resumed),
    ];
    let mut output =
        call.mutation_output_with(&started.outcome, &[stale], &tail);
    let after = started.outcome.after();
    output.lines = item_lines(call, after, started.outcome.items_after());
    let messages = text_lines(&call.paint, started.outcome.messages().iter());
    output.lines.extend(messages);
    Ok(call.print_ids(output, after, started.outcome.items_after()))
}

/// The fields `next` reports about what it skipped.
fn seen_fields(seen: &Skipped) -> [Field; 4] {
    [
        Field::array("stale", seen.stale.iter().map(ref_json)),
        Field::count("later_count", seen.later),
        Field::count("blocked_count", seen.blocked),
        Field::count("claimed_count", seen.claimed),
    ]
}

/// The list line of each item, then a line for each linked file.
fn item_lines<'a>(
    call: &Call<'_>,
    snap: &Snapshot,
    items: impl Iterator<Item = &'a Item>,
) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    for item in items {
        lines.push(list_line(&call.paint, snap, item));
        for file in linked_files(snap, item, call.home) {
            let mut text = b"  ".to_vec();
            text.extend_from_slice(file.path.as_os_str().as_bytes());
            if !file.path.exists() {
                text.extend_from_slice(b" (missing)");
            }
            lines.push(call.paint.paint(Slot::Link, &text));
        }
    }
    lines
}
