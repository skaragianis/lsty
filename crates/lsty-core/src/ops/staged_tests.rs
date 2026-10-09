use std::sync::Arc;

use super::test_util::{folder, snapshot as test_snapshot};
use super::{Path, Tx};
use crate::errs::Source;
use crate::lineid::fuzzing::Lcg;
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
        tx.todo.compact();
        tx.done.compact();
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
            tx.relocate(&[id], 0);
        }
    }),
    ("relocate the first to the end", |tx| {
        if let Some(id) = first(tx) {
            tx.relocate(&[id], usize::MAX);
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
        tx.todo.compact();
    }),
];

fn snapshot(todo: &[u8], done: &[u8]) -> Snapshot {
    test_snapshot(&folder("", None), todo, done)
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
        for (_, edit) in EDITS {
            let mut tx = Tx::new(snapshot(todo, done), false);
            edit(&mut tx);
            // `after_file` checks itself against the parse in debug builds
            for file in [&tx.todo, &tx.done] {
                drop(file.after_file(&tx.snap));
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
    let todo = tx.todo.after_file(&tx.snap);
    let done = tx.done.after_file(&tx.snap);
    assert!(reused(&tx.snap, &todo, 1));
    assert!(!reused(&tx.snap, &todo, 2));
    assert!(reused(&tx.snap, &done, 1));
    assert!(
        reused(&tx.snap, &done, 2),
        "a moved line is the line it was"
    );
}

#[test]
fn an_appended_line_takes_the_first_line_ending_and_keeps_the_mark() {
    // 01.28
    for (before, after) in [
        (&b"a\r\nb"[..], &b"a\r\nb\r\nc\r\n"[..]),
        (b"a\r\nb\n", b"a\r\nb\nc\r\n"),
        (b"\xEF\xBB\xBFa\n", b"\xEF\xBB\xBFa\nc\n"),
    ] {
        let mut tx = Tx::new(snapshot(before, b""), false);
        tx.append(Source::Todo, "c", Path::Text);
        assert_eq!(tx.todo.seal().bytes, after);
    }
}

#[test]
fn a_line_that_reads_back_otherwise_makes_the_file_parse_whole() {
    // the last line had no ending and ends in a carriage return: given a
    // newline, it reads back as a line ending in CR LF
    let mut tx = Tx::new(snapshot(b"a\nb\r", b""), false);
    tx.append(Source::Todo, "c", Path::Text);
    let bytes = tx.todo.seal().bytes;
    assert_eq!(bytes, b"a\nb\r\nc\n");
    let after = tx.todo.after_file(&tx.snap);
    assert_eq!(after.lines[1].raw(), b"b");
    assert!(!reused(&tx.snap, &after, 1));

    // a byte order mark that opens the file once the line before is gone
    let mut tx = Tx::new(snapshot(b"a\n\xEF\xBB\xBFb\n", b""), false);
    if let Some(id) = first(&tx) {
        tx.blank(id);
    }
    tx.todo.compact();
    let after = tx.todo.after_file(&tx.snap);
    assert!(after.bom);
}

#[test]
fn a_file_is_changed_when_its_bytes_differ_from_the_file_read() {
    for (todo, done) in FILES {
        for (name, edit) in EDITS {
            let mut tx = Tx::new(snapshot(todo, done), false);
            edit(&mut tx);
            for (file, read) in [(&tx.todo, todo), (&tx.done, done)] {
                let sealed = file.seal();
                assert_eq!(
                    sealed.changed,
                    sealed.bytes != *read,
                    "{name}: {todo:?} {done:?} -> {:?}",
                    sealed.bytes,
                );
            }
        }
    }
}

#[test]
fn a_line_set_back_to_its_text_leaves_the_file_unchanged() {
    let mut tx = Tx::new(snapshot(b"a\nb\nc", b""), false);
    let ids = items(&tx);
    tx.set(ids[1], "other", Path::Action);
    assert!(tx.todo.seal().changed);
    tx.set(ids[1], "b", Path::Action);
    assert!(!tx.todo.seal().changed);
}

#[test]
fn staged_lines_share_the_bytes_read() {
    let mut tx = Tx::new(snapshot(b"a\nb\n", b"x d\n"), false);
    let ids = items(&tx);
    tx.set(ids[0], "changed", Path::Action);
    let read = tx.snap.todo();
    let [first, second] = [&tx.todo.rows()[0], &tx.todo.rows()[1]];
    assert!(Arc::ptr_eq(&first.orig, &read.lines[0].shared()));
    assert!(!Arc::ptr_eq(&first.raw, &first.orig));
    assert!(Arc::ptr_eq(&second.orig, &read.lines[1].shared()));
    assert!(Arc::ptr_eq(&second.raw, &second.orig));
}

/// Where a line goes that follows original line `nr`, worked out the
/// slow way over a plain order.
fn model_after(order: &[usize], orig_len: usize, nr: usize) -> usize {
    nr.min(orig_len)
        .checked_sub(1)
        .and_then(|i| order.iter().position(|&o| o == i))
        .map_or(0, |at| at + 1)
}

/// The order a line added after `after` makes, worked out the slow way.
fn model_insert(
    file: &super::staged::StagedFile,
    orig_len: usize,
    after: usize,
) -> Vec<usize> {
    let mut order = file.order().to_vec();
    let mut at = model_after(&order, orig_len, after);
    while let Some(&next) = order.get(at) {
        let row = &file.rows()[next];
        if !(row.added && row.anchor == Some(after)) {
            break;
        }
        at += 1;
    }
    order.insert(at, file.rows().len());
    order
}

#[test]
fn many_insertions_keep_the_order_a_slow_walk_gives() {
    let todo: Vec<u8> = (0..40)
        .flat_map(|i| {
            let blank = i % 7 == 3;
            if blank {
                "\n".to_owned()
            } else {
                format!("t{i}\n")
            }
            .into_bytes()
        })
        .collect();
    let mut tx = Tx::new(snapshot(&todo, b"x 2026-01-01 d\n"), false);
    let orig_len = tx.todo.rows().len();
    let mut rng = Lcg(12345);
    for step in 0..600 {
        let file = &mut tx.todo;
        match rng.below(10) {
            0 => {
                let index = rng.below(orig_len);
                let after = rng.below(orig_len + 3);
                if after.min(orig_len).checked_sub(1) == Some(index) {
                    continue;
                }
                file.relocate(&[index], after);
            }
            1 => {
                let mut expect = file.order().to_vec();
                expect.retain(|&i| !file.rows()[i].raw.is_empty());
                file.compact();
                assert_eq!(file.order(), expect, "step {step}");
            }
            2 => {
                let index = file.rows().len();
                file.append(Arc::from(&b"tail"[..]), Path::Text);
                assert_eq!(file.order().last(), Some(&index), "step {step}");
            }
            _ => {
                // mostly the end of the file, as a move into it is
                let after = if rng.below(3) == 0 {
                    rng.below(orig_len + 3)
                } else {
                    orig_len
                };
                let expect = model_insert(file, orig_len, after);
                file.insert(after, Arc::from(&b"new"[..]), Path::Action);
                assert_eq!(file.order(), expect, "step {step}");
            }
        }
    }
}

/// A todo file of 300 lines, every third done, and a done file of one.
fn many_done() -> Tx {
    let todo: String = (0..300)
        .map(|i| {
            if i % 3 == 0 {
                format!("x 2026-02-02 moved{i}\n")
            } else {
                format!("open{i}\n")
            }
        })
        .collect();
    Tx::new(snapshot(todo.as_bytes(), b"x 2026-01-01 d\n"), false)
}

fn done_ids(tx: &Tx) -> Vec<ItemId> {
    items(tx)
        .into_iter()
        .filter(|&id| tx.line(id).is_some_and(|l| l.raw().starts_with(b"x ")))
        .collect()
}

/// Seals the done file and checks the moved lines follow the old one in order.
fn assert_moved_in_order(tx: &Tx) {
    let bytes = tx.done.seal().bytes;
    let moved: Vec<String> = (0..300)
        .step_by(3)
        .map(|i| format!("x 2026-02-02 moved{i}\n"))
        .collect();
    let moved = moved.concat();
    let expect = format!("x 2026-01-01 d\n{moved}");
    assert_eq!(String::from_utf8_lossy(&bytes), expect);
    assert_eq!(tx.primary.len(), 100);
}

#[test]
fn moving_many_done_lines_keeps_file_order_after_the_old_ones() {
    let mut tx = many_done();
    for id in done_ids(&tx) {
        let raw = tx.line(id).map(|l| l.raw().to_vec()).unwrap_or_default();
        tx.send_to_done(id, &raw);
    }
    assert_moved_in_order(&tx);
}

#[test]
fn archiving_many_done_lines_keeps_file_order_after_the_old_ones() {
    let mut tx = many_done();
    assert_eq!(tx.move_done_lines().0, 100);
    assert_moved_in_order(&tx);
}
