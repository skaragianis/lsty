//! `lsty show REF`: one item, with the facts about it.

use std::{os::unix::ffi::OsStrExt as _, path::Path};

use lsty_core::{
    errs::Result,
    links::{LinkedFile, linked_files},
    list::{Item, ItemId, Snapshot, State},
    ops::{parse_one, resolve},
    todotxt::{Kind, is_structural},
};

use crate::{
    commands::Call,
    item_json::first_tag,
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
    call.bind(&mut [&mut target])?;
    let snap = call.load()?;
    let item = resolve(&snap, &target)?;
    let messages = snap.warnings_for([item]);
    let lines = if call.has(Opt::Raw) {
        vec![snap.line(item).raw().to_vec()]
    } else {
        block(call, &snap, item, &messages)
    };
    let fields = if call.json() {
        vec![
            Field::new("items", call.item_json(&snap).objects(&[item])),
            Field::strs("messages", &messages),
        ]
    } else {
        Vec::new()
    };
    Ok(Output {
        fields,
        lines,
        ..Output::default()
    })
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
    [b"  ".as_slice(), &parts.concat()].concat()
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
    let mut maybe = |label: &str, value: Option<Vec<u8>>| {
        if let Some(value) = value {
            facts.push(labelled(label, &value));
        }
    };
    maybe(
        "priority",
        line.effective_priority().map(|p| vec![p.letter()]),
    );
    maybe("created", line.created().map(|s| s.as_bytes().to_vec()));
    maybe("completed", line.completed().map(|s| s.as_bytes().to_vec()));
    maybe("due", tags.due.map(|s| s.as_bytes().to_vec()));
    maybe(
        "t",
        tags.threshold.map(|t| item.threshold_text(t).into_bytes()),
    );
    maybe(
        "rec",
        tags.rec
            .and_then(|_| first_tag(line, b"rec"))
            .map(<[u8]>::to_vec),
    );
    maybe("id", tags.id.map(<[u8]>::to_vec));
    facts.extend(claim_fact(call, item, tags.owner));
    facts.extend(relation_facts(snap, item));
    facts.extend(file_facts(call.home, item, snap));
    facts.extend(tag_facts(snap, item));
    facts
}

fn claim_fact(
    call: &Call<'_>,
    item: &Item,
    owner: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let text = item.claim_text(owner)?;
    let clock = &call.ops.clock;
    let since = item
        .started_text(clock, true)
        .filter(|_| item.state() == State::Doing)
        .map_or_else(Vec::new, |t| format!(" since {t}").into_bytes());
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

fn file_facts(
    home: Option<&Path>,
    item: &Item,
    snap: &Snapshot,
) -> Vec<Vec<u8>> {
    linked_files(snap, item, home)
        .into_iter()
        .map(|LinkedFile { path, .. }| {
            let state = if path.exists() { "exists" } else { "missing" };
            let path = path.as_os_str().as_bytes();
            fact(&[b"file: ", path, b" (", state.as_bytes(), b")"])
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
