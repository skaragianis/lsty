use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path as FsPath;

use super::test_util::{Dir, folder};
use super::{
    AddParams, ChangeKind, ClaimOpts, DoOpts, Env, Followed, PriPair, Target,
    Tracking, UndoRecord, add, archive, del, down, finish, parse_refs, pri,
    redo, restore, undo, up,
};
use crate::errs::{Code, Source};
use crate::fsio::Hooks;
use crate::todotxt::{File, Priority};

fn lines(rows: &[&str]) -> String {
    let mut out = rows.join("\n");
    out.push('\n');
    out
}

/// The lines of a record as it left them, each followed to itself.
struct Held {
    files: Vec<Side>,
}

/// One file of a record: which, as left, and where its lines are.
type Side = (Source, File, Vec<Option<usize>>);

impl Held {
    fn of(rec: &UndoRecord) -> Self {
        let files = rec
            .files
            .iter()
            .map(|f| {
                let at = (0..f.post().len()).map(Some).collect();
                (f.source, File::parse(&f.data), at)
            })
            .collect();
        Self { files }
    }

    fn tracking(&self) -> Tracking<'_> {
        let of = |source| {
            self.files
                .iter()
                .find(|(s, _, _)| *s == source)
                .map(|(_, seen, at)| Followed { seen, at })
        };
        Tracking {
            todo: of(Source::Todo),
            done: of(Source::Done),
        }
    }
}

fn undo_it(env: &Env<'_>, rec: &UndoRecord) -> crate::errs::Result<()> {
    let held = Held::of(rec);
    undo(env, rec, &held.tracking()).map(drop)
}

/// A list where an archive compacted two blank lines.
fn archived(dir: &Dir) -> UndoRecord {
    let mut outcome = archive(&dir.env()).expect("archive");
    outcome.take_undo().expect("record")
}

const ARCHIVE_TODO: &[&str] =
    &["A", "x 2026-09-01 D1", "B", "", "x 2026-09-02 D2", "C"];
const ARCHIVE_DONE: &[&str] = &["x 2026-08-01 Old"];

struct Case {
    name: &'static str,
    todo: &'static str,
    done: &'static str,
    want_todo: &'static [&'static str],
    want_done: &'static [&'static str],
}

const ARCHIVE_CASES: &[Case] = &[
    Case {
        name: "36.27 compacted archive",
        todo: "",
        done: "",
        want_todo: &["A", "x 2026-09-01 D1", "B", "x 2026-09-02 D2", "C"],
        want_done: &["x 2026-08-01 Old"],
    },
    Case {
        name: "36.29 other line in the todo file",
        todo: "A\nB +t\nC\n",
        done: "",
        want_todo: &["A", "x 2026-09-01 D1", "x 2026-09-02 D2", "B +t", "C"],
        want_done: &["x 2026-08-01 Old"],
    },
    Case {
        name: "36.29 other line in the done file",
        todo: "",
        done: "x 2026-08-01 Old +t\nx 2026-09-01 D1\nx 2026-09-02 D2\n",
        want_todo: &["A", "x 2026-09-01 D1", "B", "x 2026-09-02 D2", "C"],
        want_done: &["x 2026-08-01 Old +t"],
    },
];

fn outside(dir: &Dir, todo: &str, done: &str) {
    if !todo.is_empty() {
        fs::write(&dir.found.todo, todo).expect("write todo");
    }
    if !done.is_empty() {
        fs::write(&dir.found.done, done).expect("write done");
    }
}

#[test]
fn undoing_an_archive_returns_the_lines() {
    for case in ARCHIVE_CASES {
        let dir = folder(&lines(ARCHIVE_TODO), Some(&lines(ARCHIVE_DONE)));
        let rec = archived(&dir);
        outside(&dir, case.todo, case.done);
        undo_it(&dir.env(), &rec).expect(case.name);
        assert_eq!(dir.todo(), lines(case.want_todo), "{}", case.name);
        assert_eq!(dir.done(), lines(case.want_done), "{}", case.name);
    }
}

#[test]
fn a_changed_done_line_refuses_the_undo_of_an_archive() {
    let dir = folder(&lines(ARCHIVE_TODO), Some(&lines(ARCHIVE_DONE)));
    let rec = archived(&dir);
    outside(
        &dir,
        "",
        &lines(&["x 2026-08-01 Old", "x 2026-09-01 D1 +t", "x 2026-09-02 D2"]),
    );
    let (todo, done) = (dir.todo(), dir.done());
    let err = undo_it(&dir.env(), &rec).expect_err("36.28 refused");
    assert_eq!(err.code(), Code::ExpectMismatch);
    assert_eq!(
        err.message(),
        "can't undo: \"x 2026-09-01 D1\" was changed since"
    );
    assert_eq!((dir.todo(), dir.done()), (todo, done), "36.28 wrote");
}

/// Records the files renamed over.
struct Order(RefCell<Vec<String>>);

impl Hooks for Order {
    fn before_rename(&self, target: &FsPath) -> io::Result<()> {
        let name = target.file_name().unwrap_or_default();
        self.0
            .borrow_mut()
            .push(name.to_string_lossy().into_owned());
        Ok(())
    }
}

fn target(arg: &str) -> Vec<Target> {
    parse_refs(arg).expect("reference")
}

#[test]
fn an_undo_writes_the_file_the_lines_return_to_first() {
    let mut auto = folder("A\n", None);
    auto.config.auto_archive = true;
    let mut outcome =
        finish(&auto.env(), &target("1"), DoOpts::default()).expect("do");
    let kept = folder("A\n", Some("x 2026-09-01 Old\n"));
    let mut restored =
        restore(&kept.env(), &target("done:1"), ClaimOpts::default())
            .expect("restore");
    let rows = [
        (&auto, outcome.take_undo(), ["todo.txt", "done.txt"]),
        (&kept, restored.take_undo(), ["done.txt", "todo.txt"]),
    ];
    for (dir, rec, want) in rows {
        let rec = rec.expect("record");
        let order = Order(RefCell::new(Vec::new()));
        let mut env = dir.env();
        env.hooks = &order;
        undo_it(&env, &rec).expect("undo");
        assert_eq!(*order.0.borrow(), want, "36.30");
    }
}

#[test]
fn a_redo_does_the_action_again() {
    let dir = folder("A\nB\n", None);
    let pair = PriPair {
        target: target("2").remove(0),
        letter: Some(Priority::A),
    };
    let mut done = pri(&dir.env(), &[pair]).expect("pri");
    let rec = done.take_undo().expect("record");
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    assert_eq!(dir.todo(), "A\nB\n");
    let back = undone.take_undo().expect("record");
    let held = Held::of(&back);
    redo(&dir.env(), &back, &held.tracking()).expect("redo");
    assert_eq!(dir.todo(), "A\n(A) B\n");
}

#[test]
fn a_redo_says_so_when_it_is_refused() {
    let dir = folder("A\n", None);
    let pair = PriPair {
        target: target("1").remove(0),
        letter: Some(Priority::A),
    };
    let mut done = pri(&dir.env(), &[pair]).expect("pri");
    let rec = done.take_undo().expect("record");
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    outside(&dir, "A +t\n", "");
    let back = undone.take_undo().expect("record");
    let held = Held::of(&back);
    let err = redo(&dir.env(), &back, &held.tracking()).expect_err("refused");
    assert_eq!(err.message(), "can't redo: \"A\" was changed since");
}

#[test]
fn an_undo_does_not_run_the_rules() {
    let dir = folder("A id:a dep:b\nB id:b\n", None);
    let mut done =
        del(&dir.env(), &target("1"), ClaimOpts::default()).expect("del");
    let rec = done.take_undo().expect("record");
    outside(&dir, "\nB id:b dep:a\n", "");
    undo_it(&dir.env(), &rec).expect("the undo restores the line");
    assert_eq!(dir.todo(), "A id:a dep:b\nB id:b dep:a\n");
}

/// Lines of a list in which every third item is done, and some are
/// alike.
fn mixed(count: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|i| match i {
            i if i % 3 == 0 => format!("x 2026-09-01 Same {}", i % 4),
            i => format!("Item {}", i % 5),
        })
        .collect();
    lines(&rows.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn undoing_a_large_archive_puts_every_line_back_in_its_place() {
    let todo = mixed(400);
    let dir = folder(&todo, Some("x 2026-08-01 Old\n"));
    let rec = archived(&dir);
    assert_ne!(dir.todo(), todo);
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), todo);
    let done = dir.done();
    assert!(done.starts_with("x 2026-08-01 Old\n"));
    assert!(done.lines().skip(1).all(str::is_empty));
}

const SEVEN: &[&str] = &["L1", "L2", "L3", "L4", "L5", "L6", "L7"];

/// Which items are deleted, and then put back.
const DELETED: &[&str] = &[
    "1",
    "4",
    "7",
    "1,2",
    "6,7",
    "3,4",
    "1,3,5,7",
    "1,7",
    "2,3,4,5,6",
    "1,2,3,4,5,6,7",
];

#[test]
fn undoing_a_delete_puts_every_line_back_wherever_it_stood() {
    let base = lines(SEVEN);
    for refs in DELETED {
        let dir = folder(&base, None);
        let mut out =
            del(&dir.env(), &target(refs), ClaimOpts::default()).expect("del");
        let rec = out.take_undo().expect("record");
        assert_ne!(dir.todo(), base, "{refs}");
        undo_it(&dir.env(), &rec).expect("undo");
        assert_eq!(dir.todo(), base, "{refs}");
    }
}

#[test]
fn undoing_a_delete_next_to_lines_that_changed_outside_keeps_those() {
    let dir = folder(&lines(SEVEN), None);
    let mut out =
        del(&dir.env(), &target("3,4"), ClaimOpts::default()).expect("del");
    let rec = out.take_undo().expect("record");
    fs::write(&dir.found.todo, "L1\nL2\nL5 outside\nL6\nL7\n").expect("write");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), "L1\nL2\nL3\nL4\nL5 outside\nL6\nL7\n");
}

#[test]
fn undoing_an_added_item_takes_it_out_and_a_changed_one_back() {
    let dir = folder("A\nB\n", None);
    let params = AddParams {
        texts: vec!["N".to_owned()],
        ..AddParams::default()
    };
    let mut added = add(&dir.env(), &params).expect("add");
    let rec = added.take_undo().expect("record");
    assert_eq!(dir.todo().lines().count(), 3);
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), "A\nB\n");

    let pair = PriPair {
        target: target("1").remove(0),
        letter: Some(Priority::A),
    };
    let mut changed = pri(&dir.env(), &[pair]).expect("pri");
    let rec = changed.take_undo().expect("record");
    assert_eq!(dir.todo(), "(A) A\nB\n");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), "A\nB\n");
}

#[test]
fn undoing_an_add_gives_back_a_file_without_a_final_newline() {
    let dir = folder("A\nB", None);
    let params = AddParams {
        texts: vec!["N".to_owned()],
        ..AddParams::default()
    };
    let mut added = add(&dir.env(), &params).expect("add");
    let rec = added.take_undo().expect("record");
    assert_eq!(dir.todo(), "A\nB\n2026-09-29 N\n");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), "A\nB");
}

/// The item moved, how far, and which way.
const SHIFTS: &[(&str, bool, i64)] = &[
    ("3", true, 1),
    ("3", false, 1),
    ("1,2", false, 1),
    ("4,5", true, 2),
    ("2,4", true, 1),
    ("5", true, 4),
    ("1", false, 4),
];

#[test]
fn undoing_a_move_puts_the_lines_back_in_their_order() {
    let base = lines(&SEVEN[..5]);
    for (refs, up_it, n) in SHIFTS {
        let dir = folder(&base, None);
        let targets = target(refs);
        let mut out = if *up_it {
            up(&dir.env(), &targets, *n)
        } else {
            down(&dir.env(), &targets, *n)
        }
        .expect("move");
        let rec = out.take_undo().expect("record");
        assert_ne!(dir.todo(), base, "{refs} {up_it} {n}");
        undo_it(&dir.env(), &rec).expect("undo");
        assert_eq!(dir.todo(), base, "{refs} {up_it} {n}");
    }
}

/// The kinds of change an undo of `archived(dir)` records for the done
/// file, with `appended` written to the done file in between.
fn done_kinds(appended: &str) -> (Vec<ChangeKind>, String) {
    let dir = folder("A\nx 2026-09-01 D1\n", Some("x 2026-08-01 Old"));
    let rec = archived(&dir);
    if !appended.is_empty() {
        let mut done = fs::read(&dir.found.done).expect("read done");
        done.extend_from_slice(appended.as_bytes());
        fs::write(&dir.found.done, done).expect("write done");
    }
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    let back = undone.take_undo().expect("record");
    let kinds = back
        .files
        .iter()
        .filter(|f| f.source == Source::Done)
        .flat_map(|f| f.changes.iter().map(|c| c.kind))
        .collect();
    (kinds, dir.done())
}

#[test]
fn an_undo_removes_the_lines_an_archive_added_while_they_are_the_end() {
    let (kinds, done) = done_kinds("");
    assert_eq!(kinds, [ChangeKind::Removed]);
    assert_eq!(done, "x 2026-08-01 Old");
}

#[test]
fn undoing_an_archive_gives_back_both_files_byte_for_byte() {
    let dir = folder("A\nx 2026-09-28 B", Some("x 2026-09-01 Z"));
    let rec = archived(&dir);
    assert_eq!(dir.done(), "x 2026-09-01 Z\nx 2026-09-28 B\n");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.done(), "x 2026-09-01 Z");
    assert_eq!(dir.todo(), "A\nx 2026-09-28 B");
}

#[test]
fn an_undo_blanks_them_when_a_line_was_appended_after_them() {
    let (kinds, done) = done_kinds("x 2026-09-05 Later\n");
    assert_eq!(kinds, [ChangeKind::Blanked]);
    assert_eq!(done, "x 2026-08-01 Old\n\nx 2026-09-05 Later\n");
}

#[test]
fn a_redo_after_such_an_undo_archives_again() {
    let dir = folder("A\nx 2026-09-01 D1\n", Some("x 2026-08-01 Old"));
    let rec = archived(&dir);
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    assert_eq!(dir.done(), "x 2026-08-01 Old");
    let back = undone.take_undo().expect("record");
    redo(&dir.env(), &back, &Held::of(&back).tracking()).expect("redo");
    assert_eq!(dir.done(), "x 2026-08-01 Old\nx 2026-09-01 D1\n");
    assert_eq!(dir.todo(), "A\n");
}

#[test]
fn records_compare_by_content_not_by_what_was_worked_out() {
    use std::sync::Arc;

    use super::FileRecord;

    let line: Arc<[u8]> = Arc::from(&b"a"[..]);
    let make = || {
        FileRecord::new(
            Source::Todo,
            vec![Arc::clone(&line)],
            vec![Arc::clone(&line)],
            b"a\n".to_vec(),
            Vec::new(),
        )
    };
    let (worked, fresh) = (make(), make());
    assert_eq!(worked.diff_pre_to_post(), &[Some(0)]);
    assert_eq!(worked, fresh);
}
