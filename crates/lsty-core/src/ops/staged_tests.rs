use std::sync::Arc;

use super::test_util::folder;
use super::{Path, Tx};
use crate::errs::Source;
use crate::list::{Item, ItemId, Snapshot};
use crate::todotxt::File;

/// A todo file and a done file.
type Pair = (&'static [u8], &'static [u8]);

/// Todo and done files that end in every way a file can.
const FILES: &[Pair] = &[
    (b"a\nb\nc\n", b"x 2026-01-01 d\n"),
    (b"a\nb\nc", b"x 2026-01-01 d"),
    (b"(A) a\r\nb +p\r\nc @x\r\n", b"x d\r\n"),
    (b"\xEF\xBB\xBFa\nb\n", b"\xEF\xBB\xBFx d\n"),
    (b"a\n\nb\n  \nc\n", b"\n"),
    (b"a\nb\xFF\nc\r", b""),
    (b"a\n\xEF\xBB\xBFb\nc\n", b"x d\n\xEF\xBB\xBFx e\n"),
    (b"", b""),
    (b"a\rb\nc id:c1\nd dep:c1\n", b"x 2026-01-01 e id:e1\n"),
    (b"x done here\nopen\n", b"x d\nx e"),
    (b"a\r\nb\nc\r\n", b"x d\n"),
];

type Edit = fn(&mut Tx);

fn items(tx: &Tx) -> Vec<ItemId> {
    tx.snap
        .in_source(Source::Todo)
        .iter()
        .map(Item::item_id)
        .collect()
}

fn first(tx: &Tx) -> Option<ItemId> {
    items(tx).first().copied()
}

fn last(tx: &Tx) -> Option<ItemId> {
    items(tx).last().copied()
}

const EDITS: &[(&str, Edit)] = &[
    ("nothing", |_| {}),
    ("set the first", |tx| {
        if let Some(id) = first(tx) {
            tx.set(id, "z +new", Path::Action);
        }
    }),
    ("set the last", |tx| {
        if let Some(id) = last(tx) {
            tx.set(id, "(B) z due:2026-01-01", Path::Action);
        }
    }),
    ("set the first to end with a carriage return", |tx| {
        if let Some(id) = first(tx) {
            tx.set(id, "q\r", Path::Action);
        }
    }),
    ("set the last to end with a carriage return", |tx| {
        if let Some(id) = last(tx) {
            tx.set(id, "q\r", Path::Action);
        }
    }),
    ("set the first to two lines", |tx| {
        if let Some(id) = first(tx) {
            tx.set(id, "p\nq", Path::Action);
        }
    }),
    ("blank the first", |tx| {
        if let Some(id) = first(tx) {
            tx.blank(id);
        }
    }),
    ("blank the last", |tx| {
        if let Some(id) = last(tx) {
            tx.blank(id);
        }
    }),
    ("blank the first and compact", |tx| {
        if let Some(id) = first(tx) {
            tx.blank(id);
        }
        tx.compact(Source::Todo);
        tx.compact(Source::Done);
    }),
    ("insert at the top", |tx| {
        tx.insert(Source::Todo, 0, "top", Path::Text);
    }),
    ("insert after line 1, twice", |tx| {
        tx.insert(Source::Todo, 1, "one", Path::Text);
        tx.insert(Source::Todo, 1, "two", Path::Text);
    }),
    ("append to both", |tx| {
        tx.append(Source::Todo, "end", Path::Text);
        tx.append(Source::Done, "x 2026-02-02 end", Path::Text);
    }),
    ("relocate the last to the top", |tx| {
        if let Some(id) = last(tx) {
            tx.relocate(id, 0);
        }
    }),
    ("relocate the first to the end", |tx| {
        if let Some(id) = first(tx) {
            tx.relocate(id, usize::MAX);
        }
    }),
    ("move the first to done as read", |tx| {
        if let Some(id) = first(tx) {
            let raw =
                tx.line(id).map(|l| l.raw().to_vec()).unwrap_or_default();
            tx.send_to_done(id, &raw);
        }
    }),
    ("move the last to done, changed", |tx| {
        if let Some(id) = last(tx) {
            tx.send_to_done(id, b"x 2026-03-03 moved");
        }
        tx.compact(Source::Todo);
    }),
];

fn snapshot(todo: &[u8], done: &[u8]) -> Snapshot {
    let dir = folder("", None);
    let env = dir.env();
    Snapshot::build(
        dir.found.clone(),
        File::parse(todo),
        File::parse(done),
        &env.params(),
    )
}

/// Whether `line` of `after` shares its bytes with a line `snap` holds.
fn reused(snap: &Snapshot, after: &File, nr: usize) -> bool {
    let Some(line) = after.lines.get(nr - 1) else {
        return false;
    };
    let shared = line.shared();
    [snap.todo(), snap.done()]
        .into_iter()
        .flat_map(|file| &file.lines)
        .any(|read| Arc::ptr_eq(&read.shared(), &shared))
}

#[test]
fn the_file_after_a_write_is_what_its_bytes_parse_to() {
    for (todo, done) in FILES {
        for (name, edit) in EDITS {
            let mut tx = Tx::new(snapshot(todo, done), false);
            edit(&mut tx);
            for file in [&tx.todo, &tx.done] {
                let bytes = file.seal().bytes;
                let after = file.after_file(&tx.snap, &bytes);
                assert_eq!(
                    after,
                    File::parse(&bytes),
                    "{name}: {todo:?} {done:?} -> {bytes:?}",
                );
            }
        }
    }
}

#[test]
fn lines_as_read_are_not_parsed_again() {
    let mut tx = Tx::new(snapshot(b"a\nb\nc\n", b"x d\n"), false);
    let ids = items(&tx);
    tx.set(ids[1], "changed", Path::Action);
    let raw = tx
        .line(ids[2])
        .map(|l| l.raw().to_vec())
        .unwrap_or_default();
    tx.send_to_done(ids[2], &raw);
    let todo = tx.todo.after_file(&tx.snap, &tx.todo.seal().bytes);
    let done = tx.done.after_file(&tx.snap, &tx.done.seal().bytes);
    assert!(reused(&tx.snap, &todo, 1));
    assert!(!reused(&tx.snap, &todo, 2));
    assert!(reused(&tx.snap, &done, 1));
    assert!(
        reused(&tx.snap, &done, 2),
        "a moved line is the line it was"
    );
}

#[test]
fn a_line_that_reads_back_otherwise_makes_the_file_parse_whole() {
    // the last line had no ending and ends in a carriage return: given a
    // newline, it reads back as a line ending in CR LF
    let mut tx = Tx::new(snapshot(b"a\nb\r", b""), false);
    tx.append(Source::Todo, "c", Path::Text);
    let bytes = tx.todo.seal().bytes;
    assert_eq!(bytes, b"a\nb\r\nc\n");
    let after = tx.todo.after_file(&tx.snap, &bytes);
    assert_eq!(after, File::parse(&bytes));
    assert_eq!(after.lines[1].raw(), b"b");
    assert!(!reused(&tx.snap, &after, 1));

    // a byte order mark that opens the file once the line before is gone
    let mut tx = Tx::new(snapshot(b"a\n\xEF\xBB\xBFb\n", b""), false);
    if let Some(id) = first(&tx) {
        tx.blank(id);
    }
    tx.compact(Source::Todo);
    let bytes = tx.todo.seal().bytes;
    let after = tx.todo.after_file(&tx.snap, &bytes);
    assert!(after.bom);
    assert_eq!(after, File::parse(&bytes));
}
