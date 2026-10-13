//! `lsty show REF`: one item, with the facts about it.

use std::{os::unix::ffi::OsStrExt as _, path::Path};

use lsty_core::{
    errs::Result,
    links::{Link, LinkedFile, links},
    list::{Item, ItemId, Snapshot, State},
    ops::{parse_one, resolve},
    todotxt::{Kind, is_structural},
};

use crate::{
    commands::Call,
    item_json::rec_text,
    output::{Field, Output},
    parse::{Opt, usage},
    read::{list_line, text_lines},
};

/// `lsty show`.
pub(crate) fn show(call: &Call<'_>) -> Result<Output> {
    let [arg] = call.parsed.args.as_slice() else {
        return Err(usage("show takes one reference"));
    };
    let mut target = parse_one("show", arg)?;
    call.bind([&mut target])?;
    let snap = call.load()?;
    let item = resolve(&snap, &target)?;
    let messages = snap.warnings_for([item]);
    Ok(Output::either(
        call.json(),
        || {
            vec![
                Field::new("items", call.item_json(&snap).objects(&[item])),
                Field::strs("messages", &messages),
            ]
        },
        || {
            if call.has(Opt::Raw) {
                vec![snap.line(item).raw().to_vec()]
            } else {
                block(call, &snap, item, &messages)
            }
        },
    ))
}

fn block(
    call: &Call<'_>,
    snap: &Snapshot,
    item: &Item,
    messages: &[String],
) -> Vec<Vec<u8>> {
    let mut lines = vec![list_line(&call.paint, snap, item)];
    lines.extend(text_lines(&call.paint, facts(call, snap, item).iter()));
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

fn facts(call: &Call<'_>, snap: &Snapshot, item: &Item) -> Vec<Vec<u8>> {
    let line = snap.line(item);
    let tags = line.tags();
    let mut facts = vec![
        labelled("text", &line.text()),
        labelled("state", item.state().as_str().as_bytes()),
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
    let threshold = tags.threshold.map(|t| item.threshold_text(t));
    maybe("t", threshold.as_deref().map(str::as_bytes));
    maybe("rec", rec_text(line, &tags));
    maybe("id", tags.id);
    facts.extend(claim_fact(call, item, tags.owner));
    facts.extend(relation_facts(snap, item));
    facts.extend(link_facts(call.home, item, snap));
    facts.extend(tag_facts(snap, item));
    facts
}

fn claim_fact(
    call: &Call<'_>,
    item: &Item,
    owner: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let text = item.claim_text(owner)?;
    let clock = call.ops.clock;
    let since = match item.state() {
        State::Doing => item
            .started_text(clock, true)
            .map(|t| format!(" since {t}")),
        State::Done => item.ended_text(clock).map(|end| {
            let began = item
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

fn relation_facts(snap: &Snapshot, item: &Item) -> Vec<Vec<u8>> {
    let mut facts = Vec::new();
    let blockers: Vec<Vec<u8>> = item
        .blockers()
        .iter()
        .filter_map(|&b| snap.item(b))
        .map(|b| blocker_text(snap, b))
        .collect();
    if !blockers.is_empty() {
        facts.push(labelled("blocked by", &blockers.join(&b", "[..])));
    }
    let dangling: Vec<&[u8]> = snap.dangling(item).collect();
    if !dangling.is_empty() {
        let list = dangling.join(&b", "[..]);
        facts.push(labelled("missing prerequisite", &list));
    }
    if item.cycle() {
        let text = cycle_text(snap, item);
        facts.push(labelled("dependency cycle", text.as_bytes()));
    }
    facts
}

fn blocker_text(snap: &Snapshot, blocker: &Item) -> Vec<u8> {
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

fn cycle_text(snap: &Snapshot, item: &Item) -> String {
    let path: Vec<ItemId> =
        snap.cycle_path(item.item_id()).unwrap_or_default();
    let mut parts: Vec<String> = path
        .iter()
        .filter_map(|&m| snap.item(m))
        .map(Item::handle)
        .collect();
    parts.push(item.handle());
    parts.join(" → ")
}

fn link_facts(
    home: Option<&Path>,
    item: &Item,
    snap: &Snapshot,
) -> Vec<Vec<u8>> {
    links(snap, item, home)
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

fn tag_facts(snap: &Snapshot, item: &Item) -> Vec<Vec<u8>> {
    let line = snap.line(item);
    let done = item.state() == State::Done;
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
