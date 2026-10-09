//! `lsty next`: the item an agent works on next.

use std::os::unix::ffi::OsStrExt as _;

use lsty_core::errs::{Ref, Result};
use lsty_core::links::linked_files;
use lsty_core::list::{Item, Snapshot, SortField};
use lsty_core::ops::{self, NextParams};
use lsty_core::theme::Slot;

use crate::commands::Call;
use crate::output::{Field, Json, Output, ref_json};
use crate::parse::Opt;
use crate::read::{list_line, text_lines};
use crate::reflist::without_args;

/// `lsty next`: shows the first ready item, or with `--start` claims it.
pub(crate) fn next(call: &mut Call<'_>) -> Result<Output> {
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
            later_field(started.later),
            Field::bool("resumed", started.resumed),
        ];
        let mut output =
            call.mutation_output_with(&started.outcome, &extra, &tail);
        let after = started.outcome.after();
        output.lines = item_lines(call, after, started.outcome.items_after());
        let messages =
            text_lines(&call.paint, started.outcome.messages().iter());
        output.lines.extend(messages);
        return Ok(output);
    }
    let reading = ops::next_read(env, &params)?;
    let snap = &reading.snapshot;
    let items: Vec<&Item> = reading
        .item
        .and_then(|id| snap.item(id))
        .into_iter()
        .collect();
    let fields = if call.json() {
        vec![
            Field::new("items", call.item_json(snap).objects(&items)),
            stale_field(&reading.stale),
            later_field(reading.later),
            Field::array(
                "messages",
                reading.messages.iter().map(|m| Json::bytes(m)),
            ),
            Field::bool("resumed", reading.resumed),
        ]
    } else {
        Vec::new()
    };
    let mut lines = item_lines(call, snap, items.iter().copied());
    lines.extend(text_lines(&call.paint, reading.messages.iter()));
    Ok(Output { fields, lines })
}

fn stale_field(stale: &[Ref]) -> Field {
    Field::array("stale", stale.iter().map(ref_json))
}

fn later_field(later: usize) -> Field {
    Field::int("later_count", i64::try_from(later).unwrap_or(i64::MAX))
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
        for file in linked_files(snap, item, call.home.as_deref()) {
            let mut text = b"  ".to_vec();
            text.extend_from_slice(file.path.as_os_str().as_bytes());
            if !file.exists {
                text.extend_from_slice(b" (missing)");
            }
            lines.push(call.paint.paint(Slot::Link, &text));
        }
    }
    lines
}
