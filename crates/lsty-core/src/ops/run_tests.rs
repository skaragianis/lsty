use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::Duration;

use super::test_util::{field, folder, said};
use super::{
    Change, ChangeKind, Env, Path, PriPair, Target, Tx, parse_letter, pri, run,
};
use crate::discover::FileState;
use crate::errs::{self, Code, Nr, Source};
use crate::fsio::{self, Hooks};
use crate::list::{self, ItemId, Params, Snapshot};
use crate::lock::{self, Cancel, Mode, Want};

fn id(tx: &Tx, source: Source, nr: u32) -> ItemId {
    let at = Nr::new(nr).and_then(|nr| tx.snapshot().at(source, nr));
    at.expect("item").item_id()
}

fn pair(nr: i64, letter: &str) -> PriPair {
    PriPair {
        target: Target::todo(nr),
        letter: (!letter.is_empty())
            .then(|| parse_letter(letter).expect("a letter")),
    }
}

#[test]
fn missing_todo_file() {
    let mut dir = folder("", None);
    fs::remove_file(&dir.found.todo).unwrap();
    dir.found.state = FileState::Missing;
    let err = run(&dir.env(), "x", false, |_: &mut Tx| Ok(())).unwrap_err();
    assert_eq!(err.code(), Code::NoFile, "12.21");
    assert_eq!(field(&err, "found_by").as_deref(), Some("--file"), "12.21");
    assert!(err.message().contains("(chosen by --file)"), "12.21");

    let mut env = dir.env();
    env.create = true;
    let out = run(&env, "x", false, |_: &mut Tx| Ok(())).unwrap();
    assert!(out.no_op(), "12.22");
    assert_eq!(dir.todo(), "", "12.22");
    assert!(out.undo().is_none(), "12.22");
    assert_eq!(out.messages(), said(&["nothing changed"]), "12.22");
}

/// Name, the line to insert after, whether the lines renumber, the NR the
/// new line gets.
type InsertRow = (&'static str, usize, bool, u32);

const INSERTS: &[InsertRow] = &[
    ("12.24 insert after line 1", 1, true, 2),
    ("12.25 insert at the end", 3, false, 4),
];

#[test]
fn renumbered_after_an_insert() {
    for (name, after, want, nr) in INSERTS {
        let dir = folder("a\nb\nc\n", None);
        let out = run(&dir.env(), "add", false, |tx| {
            let line = tx.insert(Source::Todo, *after, "new", Path::Action);
            tx.primary(&[line]);
            Ok(())
        })
        .expect(name);
        assert_eq!(out.renumbered(), *want, "{name}");
        let items: Vec<_> = out.items_after().collect();
        assert_eq!(items.len(), 1, "{name}");
        assert_eq!(items[0].nr().get(), *nr, "{name}");
        assert!(out.before().is_empty(), "{name}");
    }
}

#[test]
fn an_outcome_keeps_the_lines_as_read_and_as_left() {
    let dir = folder("(A) Buy\nCall\n", None);
    let out = pri(&dir.env(), &[pair(1, "B")]).unwrap();
    assert_eq!(out.before().len(), 1);
    assert_eq!(out.items().len(), 1);
    let was = out.read().item(out.before()[0]).expect("item as read");
    let now = out.after().item(out.items()[0]).expect("item as left");
    assert_eq!(out.read().line(was).raw(), b"(A) Buy");
    assert_eq!(out.after().line(now).raw(), b"(B) Buy");
    assert_eq!(out.files_changed(), [Source::Todo]);
    assert!(out.spawned().is_empty() && out.changed().is_empty());
}

#[test]
fn undo_record_of_a_change() {
    let dir = folder("(A) Buy\n\nCall\n", None);
    let out = pri(&dir.env(), &[pair(1, "B")]).unwrap();
    let undo = out.undo().expect("12.26");
    assert_eq!(undo.action, "pri");
    assert_eq!(undo.files.len(), 1);
    let file = &undo.files[0];
    assert_eq!(file.source, Source::Todo);
    assert_eq!(file.pre(), shared(&["(A) Buy", "", "Call"]));
    assert_eq!(file.post(), shared(&["(B) Buy", "", "Call"]));
    assert_eq!(
        file.changes,
        [Change {
            kind: ChangeKind::Changed,
            before: b"(A) Buy".to_vec(),
            after: b"(B) Buy".to_vec(),
            nr_before: Nr::new(1),
            nr_after: Nr::new(1),
        }]
    );
}

#[test]
fn a_line_added_at_the_top_goes_before_lines_added_elsewhere() {
    let dir = folder("a\nb\nc\n", None);
    run(&dir.env(), "add", false, |tx| {
        let a = id(tx, Source::Todo, 1);
        tx.insert(Source::Todo, 1, "X", Path::Action);
        tx.relocate(&[a], 3);
        tx.insert(Source::Todo, 0, "Y", Path::Action);
        Ok(())
    })
    .unwrap();
    assert_eq!(dir.todo(), "Y\nX\nb\nc\na\n");
}

#[test]
fn undo_record_kinds_and_numbers() {
    let dir = folder("a\nb\n\nc\n", None);
    let out = run(&dir.env(), "mix", false, |tx| {
        let (a, c) = (id(tx, Source::Todo, 1), id(tx, Source::Todo, 4));
        tx.blank(a);
        tx.relocate(&[c], 1);
        tx.insert(Source::Todo, 0, "top", Path::Action);
        tx.todo.compact();
        Ok(())
    })
    .unwrap();
    let got: Vec<_> = out.undo().expect("undo").files[0]
        .changes
        .iter()
        .map(|c| {
            (
                c.kind,
                str::from_utf8(&c.before).unwrap(),
                str::from_utf8(&c.after).unwrap(),
                c.nr_before.map_or(0, Nr::get),
                c.nr_after.map_or(0, Nr::get),
            )
        })
        .collect();
    let want = [
        (ChangeKind::Removed, "a", "", 1, 0),
        (ChangeKind::Removed, "", "", 3, 0),
        (ChangeKind::Moved, "c", "c", 4, 2),
        (ChangeKind::Added, "", "top", 0, 1),
    ];
    assert_eq!(got, want);
    assert_eq!(dir.todo(), "top\nc\nb\n");
}

#[test]
fn undo_record_of_a_blanked_line() {
    let dir = folder("a\nb\n", None);
    let out = run(&dir.env(), "del", false, |tx| {
        let first = id(tx, Source::Todo, 1);
        tx.blank(first);
        Ok(())
    })
    .unwrap();
    let changes = &out.undo().expect("undo").files[0].changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Blanked);
    assert_eq!(changes[0].before, b"a");
    assert_eq!(dir.todo(), "\nb\n");
}

#[test]
fn orphans_are_removed() {
    let dir = folder("Buy\n", None);
    let orphan = dir.dir().join(".todo.txt.lsty-99.tmp");
    fs::write(&orphan, "junk").unwrap();
    pri(&dir.env(), &[pair(1, "A")]).unwrap();
    assert!(!orphan.exists(), "12.27");
}

#[test]
fn lock_timeout_leaves_the_file() {
    let dir = folder("Buy\n", None);
    let mut env = dir.env();
    env.timeout = Duration::from_millis(200);
    let want = [Want {
        file: dir.found.todo.clone(),
        mode: Mode::Exclusive,
    }];
    let held = lock::acquire(&want, Duration::from_secs(1), &env.cancel);
    let err = pri(&env, &[pair(1, "A")]).unwrap_err();
    assert_eq!(err.code(), Code::LockTimeout, "12.28");
    assert_eq!(dir.todo(), "Buy\n", "12.28");
    drop(held);
}

#[test]
fn a_failed_plan_writes_nothing() {
    let dir = folder("a\nb\n", None);
    let err = run(&dir.env(), "x", false, |tx| {
        let (a, b) = (id(tx, Source::Todo, 1), id(tx, Source::Todo, 2));
        tx.set(a, "A", Path::Action);
        tx.set(b, "B", Path::Action);
        Err(errs::Error::new(Code::Io, "plan failed"))
    })
    .unwrap_err();
    assert_eq!(err.message(), "plan failed", "12.30");
    assert_eq!(dir.todo(), "a\nb\n", "12.30");
}

#[test]
fn the_content_is_read_under_the_lock() {
    let dir = folder("Buy\n", None);
    let params = Params {
        clock: dir.clock.clone(),
        stale_after: dir.config.claim_stale_after,
    };
    let before = list::load(
        &dir.found,
        dir.config.lock_timeout,
        &Cancel::new(),
        &params,
    )
    .unwrap();
    assert!(before.at(Source::Todo, Nr::new(1).unwrap()).is_some());
    fs::write(&dir.found.todo, "Other\nBuy\n").unwrap();
    pri(&dir.env(), &[pair(2, "A")]).unwrap();
    assert_eq!(dir.todo(), "Other\n(A) Buy\n", "12.31");
}

#[test]
fn an_archived_target_and_a_gone_one() {
    let dir = folder("Buy\n", Some("x 2026-09-28 Fix\n\n"));
    run(&dir.env(), "show", false, |tx| {
        let archived = tx.target(&Target::done(1).archived())?;
        assert!(matches!(archived, super::Picked::Item(_)));
        let gone = tx.target(&Target::done(2))?;
        assert_eq!(
            gone,
            super::Picked::Gone("done:2 already restored".to_owned())
        );
        Ok(())
    })
    .unwrap();
}

/// Records the files renamed over and crashes on the second.
struct CrashSecond(RefCell<Vec<String>>);

impl Hooks for CrashSecond {
    fn before_rename(&self, target: &FsPath) -> io::Result<()> {
        let name = target.file_name().unwrap_or_default();
        let mut seen = self.0.borrow_mut();
        seen.push(name.to_string_lossy().into_owned());
        if seen.len() == 2 {
            return Err(fsio::crash());
        }
        Ok(())
    }
}

#[test]
fn the_destination_is_written_first() {
    let dir = folder("a\n", Some("b\n"));
    let hooks = CrashSecond(RefCell::new(Vec::new()));
    let mut env: Env<'_> = dir.env();
    env.hooks = &hooks;
    let err = run(&env, "move", true, |tx| {
        let (a, b) = (id(tx, Source::Todo, 1), id(tx, Source::Done, 1));
        tx.dest(Source::Done);
        tx.set(a, "A", Path::Action);
        tx.set(b, "B", Path::Action);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.code(), Code::Io);
    assert_eq!(*hooks.0.borrow(), ["done.txt", "todo.txt"]);
    assert_eq!(dir.done(), "B\n");
    assert_eq!(dir.todo(), "a\n");
}

#[test]
fn same_text_stages_nothing() {
    let dir = folder("a\n", None);
    let out = run(&dir.env(), "x", false, |tx| {
        let a = id(tx, Source::Todo, 1);
        tx.set(a, "a", Path::Text);
        Ok(())
    })
    .unwrap();
    assert!(out.no_op());
}

#[test]
fn messages_come_before_warnings() {
    let dir = folder("a id:k1\nb id:k1\n", None);
    let out = run(&dir.env(), "x", false, |tx| {
        let a = id(tx, Source::Todo, 1);
        let line = tx.line_ref(a).expect("line");
        tx.primary(&[line]);
        tx.say("hello");
        Ok(())
    })
    .unwrap();
    assert_eq!(out.messages().len(), 2, "{:?}", out.messages());
    assert_eq!(out.messages()[0], b"hello");
    let warning = String::from_utf8_lossy(&out.messages()[1]);
    assert!(warning.contains("k1"), "{warning}");
}

#[test]
fn a_message_can_carry_bytes_that_are_not_text() {
    let dir = folder("caf\u{e9} x\n", None);
    fs::write(&dir.found.todo, b"caf\xe9 x\n").unwrap();
    let out = run(&dir.env(), "x", false, |tx| {
        let line = tx.line(id(tx, Source::Todo, 1)).expect("line");
        let mut said = b"1 done: ".to_vec();
        said.extend_from_slice(line.raw());
        tx.say(said);
        Ok(())
    })
    .unwrap();
    assert_eq!(out.messages(), [b"1 done: caf\xe9 x".to_vec()]);
}

/// What a plan stages in the done file while it holds only a shared lock.
const SHARED_DONE: &[fn(&mut Tx)] = &[
    |tx| {
        let b = id(tx, Source::Done, 1);
        tx.set(b, "B", Path::Action);
    },
    |tx| tx.dest(Source::Done),
];

#[test]
fn the_done_file_is_not_staged_under_a_shared_lock() {
    for stage in SHARED_DONE {
        let dir = folder("a\n", Some("b\n"));
        let err = run(&dir.env(), "move", false, |tx| {
            stage(tx);
            let a = id(tx, Source::Todo, 1);
            tx.set(a, "A", Path::Action);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.code(), Code::Io);
        assert_eq!(
            err.message(),
            "move changed the done file without locking it for writing"
        );
        assert_eq!((dir.todo().as_str(), dir.done().as_str()), ("a\n", "b\n"));
    }
}

#[test]
fn undo_is_recorded_only_when_asked_for() {
    let dir = folder("a\nb\n", None);
    let mut env = dir.env();
    let change = |tx: &mut Tx| {
        let line = tx.append(Source::Todo, "c", Path::Action);
        tx.primary(&[line]);
        Ok(())
    };
    env.record_undo = false;
    let quiet = run(&env, "add", false, change).unwrap();
    assert!(!quiet.no_op());
    assert_eq!(quiet.files_changed(), [Source::Todo]);
    assert!(quiet.undo().is_none());
    assert_eq!(dir.todo(), "a\nb\nc\n");

    env.record_undo = true;
    let kept = run(&env, "add", false, change).unwrap();
    assert_eq!(kept.files_changed(), [Source::Todo]);
    let undo = kept.undo().expect("a record");
    assert_eq!(undo.action, "add");
    assert_eq!(undo.files[0].pre().len(), 3);
    assert_eq!(undo.files[0].post().len(), 4);
    assert_eq!(dir.todo(), "a\nb\nc\nc\n");
}

#[test]
fn the_list_after_is_rebuilt_only_when_something_staged_changes() {
    let dir = folder("a\nb\n", None);
    let env = dir.env();
    run(&env, "x", false, |tx: &mut Tx| {
        let same = |a: &Arc<Snapshot>, b: &Arc<Snapshot>| Arc::ptr_eq(a, b);
        let first = tx.snapshot_after(&env);
        let again = tx.snapshot_after(&env);
        assert!(same(&first, &again));
        let at = tx.append(Source::Todo, "c", Path::Action);
        let added = tx.snapshot_after(&env);
        assert!(!same(&first, &added));
        assert_eq!(added.todo().lines.len(), 3);
        // a change and its reverse still rebuild: the list is right
        // either way
        let row = tx.todo.row_mut(at.index).expect("the added row");
        row.raw = Arc::from(&b"d"[..]);
        let set = tx.snapshot_after(&env);
        assert_eq!(set.todo().lines[2].raw(), b"d");
        let row = tx.todo.row_mut(at.index).expect("the added row");
        row.raw = Arc::from(&b"c"[..]);
        let back = tx.snapshot_after(&env);
        assert!(!same(&set, &back));
        assert_eq!(back.todo().lines[2].raw(), b"c");
        tx.done.set_order(Vec::new());
        assert!(!same(&back, &tx.snapshot_after(&env)));
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_staged_file_never_takes_a_state_twice() {
    let dir = folder("a\n", Some("x b\n"));
    let env = dir.env();
    run(&env, "x", false, |tx: &mut Tx| {
        let mut seen = std::collections::HashSet::new();
        assert!(seen.insert(tx.todo.state()));
        assert!(seen.insert(tx.done.state()));
        let steps: [fn(&mut Tx); 10] = [
            |tx| {
                tx.append(Source::Todo, "c", Path::Action);
            },
            |tx| {
                tx.todo.row_mut(1);
            },
            |tx| {
                tx.todo.rows_mut();
            },
            |tx| tx.todo.end_open_after(0),
            |tx| {
                tx.todo.insert(0, Arc::from(&b"d"[..]), Path::Action);
            },
            |tx| tx.todo.relocate(&[0], 0),
            |tx| tx.todo.compact(),
            |tx| {
                tx.todo.push_row(super::staged::Row::added(
                    Arc::from(&b"e"[..]),
                    Path::Action,
                    None,
                ));
            },
            |tx| tx.todo.set_order(vec![0]),
            |tx| tx.todo.leave_out_last(),
        ];
        for (i, step) in steps.into_iter().enumerate() {
            step(tx);
            assert!(seen.insert(tx.todo.state()), "step {i}");
        }
        Ok(())
    })
    .unwrap();
}

/// Lines as a record keeps them.
fn shared(lines: &[&str]) -> Vec<Arc<[u8]>> {
    lines.iter().map(|l| l.as_bytes().into()).collect()
}
