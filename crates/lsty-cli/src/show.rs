//! `lsty show REF`: one task, with the facts about it.

use std::{os::unix::ffi::OsStrExt as _, path::Path};

use lsty_core::{
    errs::Result,
    links::{Link, LinkedFile, links},
    list::{Snapshot, State, Task, TaskId},
    ops::{parse_one, resolve},
    todotxt::{Kind, is_structural},
};

use crate::{
    commands::Call,
    output::{Field, Output},
    parse::{Opt, usage},
    read::{list_line, text_lines},
    task_json::rec_text,
};

/// `lsty show`.
pub(crate) fn show(call: &Call<'_>) -> Result<Output> {
    let [arg] = call.parsed.args.as_slice() else {
        return Err(usage("show takes one reference"));
    };
    let mut target = parse_one("show", arg)?;
    call.bind([&mut target])?;
    let snap = call.load()?;
    let task = resolve(&snap, &target)?;
    let messages = snap.warnings_for([task]);
    Ok(Output::either(
        call.json(),
        || {
            vec![
                Field::new("tasks", call.task_json(&snap).objects(&[task])),
                Field::strs("messages", &messages),
            ]
        },
        || {
            if call.has(Opt::Raw) {
                vec![snap.line(task).raw().to_vec()]
            } else {
                block(call, &snap, task, &messages)
            }
        },
    ))
}

fn block(
    call: &Call<'_>,
    snap: &Snapshot,
    task: &Task,
    messages: &[String],
) -> Vec<Vec<u8>> {
    let mut lines = vec![list_line(&call.paint, snap, task)];
    lines.extend(text_lines(&call.paint, facts(call, snap, task).iter()));
    lines.extend(text_lines(&call.paint, messages.iter()));
    lines
}

/// A fact as an indented line made of `parts`.
fn fact(parts: &[&[u8]]) -> Vec<u8> {
    let mut line = b"  ".to_vec();
    for part in parts {
        line.extend_from_slice(part);
    }
    line
}

/// A fact as a line: `label: value`.
fn labelled(label: &str, value: &[u8]) -> Vec<u8> {
    fact(&[label.as_bytes(), b": ", value])
}

fn facts(call: &Call<'_>, snap: &Snapshot, task: &Task) -> Vec<Vec<u8>> {
    let line = snap.line(task);
    let tags = line.tags();
    let mut facts = vec![
        labelled("text", &line.text()),
        labelled("state", task.state().as_str().as_bytes()),
    ];
    let mut maybe = |label: &str, value: Option<&[u8]>| {
        if let Some(value) = value {
            facts.push(labelled(label, value));
        }
    };
    let pri = line.effective_priority().map(|p| [p.letter()]);
    maybe("priority", pri.as_ref().map(<[u8; 1]>::as_slice));
    maybe("created", line.created().map(str::as_bytes));
    maybe("completed", line.completed().map(str::as_bytes));
    maybe("due", tags.due.map(str::as_bytes));
    let threshold = tags.threshold.map(|t| task.threshold_text(t));
    maybe("t", threshold.as_deref().map(str::as_bytes));
    maybe("rec", rec_text(line, &tags));
    maybe("id", tags.id);
    facts.extend(claim_fact(call, task, tags.owner));
    facts.extend(relation_facts(snap, task));
    facts.extend(link_facts(call.home, task, snap));
    facts.extend(tag_facts(snap, task));
    facts
}

fn claim_fact(
    call: &Call<'_>,
    task: &Task,
    owner: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let text = task.claim_text(owner)?;
    let clock = call.ops.clock;
    let since = match task.state() {
        State::Doing => task
            .started_text(clock, true)
            .map(|t| format!(" since {t}")),
        State::Done => task.ended_text(clock).map(|end| {
            let began = task
                .started_text(clock, false)
                .map(|t| format!(", started {t}"))
                .unwrap_or_default();
            format!("{began}, ended {end}")
        }),
        State::Todo => None,
    };
    let since = since.unwrap_or_default().into_bytes();
    Some(fact(&[&text, &since]))
}

fn relation_facts(snap: &Snapshot, task: &Task) -> Vec<Vec<u8>> {
    let mut facts = Vec::new();
    let blockers: Vec<Vec<u8>> = task
        .blockers()
        .iter()
        .filter_map(|&b| snap.task(b))
        .map(|b| blocker_text(snap, b))
        .collect();
    if !blockers.is_empty() {
        facts.push(labelled("blocked by", &blockers.join(&b", "[..])));
    }
    let dangling: Vec<&[u8]> = snap.dangling(task).collect();
    if !dangling.is_empty() {
        let list = dangling.join(&b", "[..]);
        facts.push(labelled("missing prerequisite", &list));
    }
    if task.cycle() {
        let text = cycle_text(snap, task);
        facts.push(labelled("dependency cycle", text.as_bytes()));
    }
    facts
}

fn blocker_text(snap: &Snapshot, blocker: &Task) -> Vec<u8> {
    let mut text = blocker.handle().into_bytes();
    text.push(b' ');
    text.extend_from_slice(&snap.line(blocker).text());
    if let Some(id) = snap.id_of(blocker) {
        text.extend_from_slice(b" (id:");
        text.extend_from_slice(id);
        text.push(b')');
    }
    text
}

fn cycle_text(snap: &Snapshot, task: &Task) -> String {
    let path: Vec<TaskId> =
        snap.cycle_path(task.task_id()).unwrap_or_default();
    let mut parts: Vec<String> = path
        .iter()
        .filter_map(|&m| snap.task(m))
        .map(Task::handle)
        .collect();
    parts.push(task.handle());
    parts.join(" → ")
}

fn link_facts(
    home: Option<&Path>,
    task: &Task,
    snap: &Snapshot,
) -> Vec<Vec<u8>> {
    links(snap, task, home)
        .into_iter()
        .map(|link| match link {
            Link::File(LinkedFile { path, .. }) => {
                let state = if path.exists() { "exists" } else { "missing" };
                let path = path.as_os_str().as_bytes();
                fact(&[b"file: ", path, b" (", state.as_bytes(), b")"])
            }
            Link::Url(url) => fact(&[b"url: ", &url]),
        })
        .collect()
}

fn tag_facts(snap: &Snapshot, task: &Task) -> Vec<Vec<u8>> {
    let line = snap.line(task);
    let done = task.state() == State::Done;
    let mut facts: Vec<Vec<u8>> = line
        .tokens()
        .filter(|t| t.kind == Kind::Tag && !is_structural(t.key, done))
        .map(|t| fact(&[b"tag: ", t.text]))
        .collect();
    for extra in &line.tags().extra {
        facts.push(fact(&[
            b"extra: ",
            extra.text,
            b" (ignored, the first ",
            extra.key,
            b": counts)",
        ]));
    }
    facts
}
