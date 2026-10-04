//! `lsty next`: the item an agent works on next.

use std::fs;
use std::os::unix::ffi::OsStrExt as _;

use lsty_core::errs::{Ref, Result};
use lsty_core::list::{Item, Snapshot, SortField};
use lsty_core::ops::{self, NextParams};
use lsty_core::theme::Slot;

use crate::commands::Call;
use crate::output::{Field, Json, Output, ref_json};
use crate::read::{list_line, text_lines};
use crate::reflist::without_args;

/// `lsty next`: shows the first ready item, or with `--start` claims it.
pub fn next(call: &mut Call<'_, '_>) -> Result<Output> {
    without_args(call)?;
    let params = NextParams {
        filter: call.parse_query(&[])?,
        sort: call
            .option("sort")
            .and_then(SortField::parse)
            .unwrap_or(SortField::Priority),
    };
    let env = call.ops_env();
    if call.has("start") {
        let started = ops::next_start(&env, &params)?;
        let extra = [stale_field(&started.stale)];
        let tail = [hidden_field(started.hidden)];
        let mut output =
            call.mutation_output_with(&started.outcome, &extra, &tail);
        let after = started.outcome.after();
        output.lines = item_lines(call, after, started.outcome.items_after());
        let messages =
            text_lines(&call.paint, started.outcome.messages().iter());
        output.lines.extend(messages);
        return Ok(output);
    }
    let reading = ops::next_read(&env, &params)?;
    let snap = &reading.snapshot;
    let items: Vec<&Item> = reading
        .items
        .iter()
        .filter_map(|id| snap.item(*id))
        .collect();
    let fields = if call.json() {
        vec![
            Field::new("items", call.item_json(snap).objects(&items)),
            stale_field(&reading.stale),
            hidden_field(reading.hidden),
            Field::array(
                "messages",
                reading.messages.iter().map(|m| Json::bytes(m)),
            ),
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

fn hidden_field(hidden: usize) -> Field {
    Field::int("hidden_count", i64::try_from(hidden).unwrap_or(i64::MAX))
}

/// The list line of each item, then a line for each linked file.
fn item_lines<'a>(
    call: &Call<'_, '_>,
    snap: &Snapshot,
    items: impl Iterator<Item = &'a Item>,
) -> Vec<Vec<u8>> {
    let json = call.item_json(snap);
    let mut lines = Vec::new();
    for item in items {
        lines.push(list_line(&call.paint, snap, item));
        for link in &snap.line(item).tags().files {
            let path = json.resolve(link);
            let mut text = b"  ".to_vec();
            text.extend_from_slice(path.as_os_str().as_bytes());
            if fs::metadata(&path).is_err() {
                text.extend_from_slice(b" (missing)");
            }
            lines.push(call.paint.paint(Slot::Link, &text));
        }
    }
    lines
}
