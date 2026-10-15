//! `lsty next`: the task an agent works on next.

use std::os::unix::ffi::OsStrExt as _;

use lsty_core::{
    errs::Result,
    links::linked_files,
    list::{Snapshot, SortField, Task},
    ops::{self, NextParams, Survey},
    theme::Slot,
};

use crate::{
    commands::Call,
    output::{Field, Json, Output, ref_json},
    parse::{Hide, Opt, usage},
    read::{list_line, text_lines},
    reflist::without_args,
};

/// `lsty next`: shows the first ready task, or with `--start` claims it.
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
    let task = reading.task.and_then(|id| snap.task(id));
    Ok(Output::either(
        call.json(),
        || {
            let mut fields = vec![Field::new(
                "tasks",
                call.task_json(snap).objects(task.as_slice()),
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
        },
        || {
            let mut lines = task_lines(call, snap, task.into_iter());
            lines.extend(text_lines(&call.paint, reading.messages.iter()));
            lines
        },
    ))
}

/// `next --start`: claims the first ready task.
fn start(call: &Call<'_>, params: &NextParams) -> Result<Output> {
    let started = ops::next_start(&call.ops, params)?;
    let seen = &started.seen;
    let after = started.outcome.after();
    let output = Output::either(
        call.json(),
        || {
            let [stale, later, blocked, claimed] = seen_fields(seen);
            let tail = [
                later,
                blocked,
                claimed,
                Field::bool("resumed", seen.resumed),
            ];
            call.mutation_fields(&started.outcome, &[stale], &tail)
        },
        || {
            let mut lines =
                task_lines(call, after, started.outcome.tasks_after());
            lines.extend(text_lines(
                &call.paint,
                started.outcome.messages().iter(),
            ));
            lines
        },
    );
    Ok(call.print_ids(output, after, started.outcome.tasks_after()))
}

/// The fields `next` reports about what it skipped.
fn seen_fields(seen: &Survey) -> [Field; 4] {
    [
        Field::array("stale", seen.stale.iter().map(ref_json)),
        Field::count("later_count", seen.later),
        Field::count("blocked_count", seen.blocked),
        Field::count("claimed_count", seen.claimed),
    ]
}

/// The list line of each task, then a line for each linked file.
fn task_lines<'a>(
    call: &Call<'_>,
    snap: &Snapshot,
    tasks: impl Iterator<Item = &'a Task>,
) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    for task in tasks {
        lines.push(list_line(&call.paint, snap, task, Hide::default()));
        for file in linked_files(snap, task, call.home) {
            let mut text = b"  ".to_vec();
            let path = file.path.as_os_str().as_bytes();
            call.paint.paint_into(&mut text, Slot::Link, path);
            if !file.path.exists() {
                text.extend_from_slice(b" (missing)");
            }
            lines.push(text);
        }
    }
    lines
}
