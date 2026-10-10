//! `lsty next`: the item an agent works on next.

use std::os::unix::ffi::OsStrExt as _;

use lsty_core::errs::{Ref, Result};
use lsty_core::links::linked_files;
use lsty_core::list::{Item, Snapshot, SortField};
use lsty_core::ops::{self, NextParams};
use lsty_core::theme::Slot;

use crate::commands::Call;
use crate::output::{Field, Json, Output, ref_json};
use crate::parse::{Opt, usage};
use crate::read::{list_line, text_lines};
use crate::reflist::without_args;

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
    let env = &call.ops;
    if call.has(Opt::Start) {
        let started = ops::next_start(env, &params)?;
        let extra = [stale_field(&started.stale)];
        let tail = [
            count_field("later_count", started.later),
            count_field("blocked_count", started.blocked),
            count_field("claimed_count", started.claimed),
            Field::bool("resumed", started.resumed),
        ];
        let mut output =
            call.mutation_output_with(&started.outcome, &extra, &tail);
        let after = started.outcome.after();
        output.lines = item_lines(call, after, started.outcome.items_after());
        let messages =
            text_lines(&call.paint, started.outcome.messages().iter());
        output.lines.extend(messages);
        return Ok(call.print_ids(
            output,
            after,
            started.outcome.items_after(),
        ));
    }
    if call.has(Opt::PrintId) {
        return Err(usage("--print-id needs --start"));
    }
    let reading = ops::next_read(env, &params)?;
    let snap = &reading.snapshot;
    let item = reading.item.and_then(|id| snap.item(id));
    let fields = if call.json() {
        vec![
            Field::new("items", call.item_json(snap).objects(item.as_slice())),
            stale_field(&reading.stale),
            count_field("later_count", reading.later),
            count_field("blocked_count", reading.blocked),
            count_field("claimed_count", reading.claimed),
            Field::array(
                "messages",
                reading.messages.iter().map(|m| Json::bytes(m)),
            ),
            Field::bool("resumed", reading.resumed),
        ]
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

fn stale_field(stale: &[Ref]) -> Field {
    Field::array("stale", stale.iter().map(ref_json))
}

fn count_field(name: &'static str, count: usize) -> Field {
    Field::int(name, i64::try_from(count).unwrap_or(i64::MAX))
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
