use std::{cell::RefCell, fs, io, path::Path as FsPath};

use super::{
    AddParams, Change, ChangeKind, ClaimOpts, DoOpts, Env, FileRecord,
    Followed, Outcome, PriPair, Target, UndoRecord, add, archive, del, down,
    finish, parse_refs, pri, redo, restore,
    test_util::{Dir, folder, shared},
    undo, up,
};
use crate::{
    errs::{Code, Nr, Result, Source},
    fsio::Hooks,
    todotxt::{File, Priority},
};

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
type Side = (File, Vec<Option<usize>>);

impl Held {
    fn of(rec: &UndoRecord) -> Self {
        let files = rec
            .files
            .iter()
            .map(|f| {
                let at = (0..f.post().len()).map(Some).collect();
                (File::parse(&f.data), at)
            })
            .collect();
        Self { files }
    }

    fn tracking(&self) -> Vec<Followed<'_>> {
        self.files
            .iter()
            .map(|(seen, at)| Followed { seen, at })
            .collect()
    }
}

fn undo_it(env: &Env<'_>, rec: &UndoRecord) -> Result<()> {
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
        let dir = folder(lines(ARCHIVE_TODO), Some(&lines(ARCHIVE_DONE)));
        let rec = archived(&dir);
        outside(&dir, case.todo, case.done);
        undo_it(&dir.env(), &rec).expect(case.name);
        assert_eq!(dir.todo(), lines(case.want_todo), "{}", case.name);
        assert_eq!(dir.done(), lines(case.want_done), "{}", case.name);
    }
}

#[test]
fn a_changed_done_line_refuses_the_undo_of_an_archive() {
    let dir = folder(lines(ARCHIVE_TODO), Some(&lines(ARCHIVE_DONE)));
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
        priority: Some(Priority::A),
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
        priority: Some(Priority::A),
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

/// Lines of a list in which every third task is done, and some are
/// alike.
fn mixed(count: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|i| match i {
            i if i % 3 == 0 => format!("x 2026-09-01 Same {}", i % 4),
            i => format!("Task {}", i % 5),
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

/// Which tasks are deleted, and then put back.
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

/// Runs `op` on a folder holding `base`, checks that the file changed and
/// that undoing it gives `base` back, and returns the changed text.
fn round_trip(
    base: &str,
    label: &str,
    op: impl FnOnce(&Env<'_>) -> Result<Outcome>,
) -> String {
    let dir = folder(base, None);
    let rec = op(&dir.env()).expect("op").take_undo().expect("record");
    let changed = dir.todo();
    assert_ne!(changed, base, "{label}");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), base, "{label}");
    changed
}

#[test]
fn undoing_a_delete_puts_every_line_back_wherever_it_stood() {
    let base = lines(SEVEN);
    for refs in DELETED {
        round_trip(&base, refs, |env| {
            del(env, &target(refs), ClaimOpts::default())
        });
    }
}

#[test]
fn undoing_a_delete_gives_a_line_of_white_space_back() {
    let changed = round_trip("A\n \t\nB\n", "2,3", |env| {
        del(env, &target("2,3"), ClaimOpts::default())
    });
    assert_eq!(changed, "A\n\n\n");
}

#[test]
fn undoing_a_delete_next_to_lines_that_changed_outside_keeps_those() {
    let dir = folder(lines(SEVEN), None);
    let mut out =
        del(&dir.env(), &target("3,4"), ClaimOpts::default()).expect("del");
    let rec = out.take_undo().expect("record");
    fs::write(&dir.found.todo, "L1\nL2\nL5 outside\nL6\nL7\n").expect("write");
    undo_it(&dir.env(), &rec).expect("undo");
    assert_eq!(dir.todo(), "L1\nL2\nL3\nL4\nL5 outside\nL6\nL7\n");
}

#[test]
fn undoing_an_added_task_takes_it_out_and_a_changed_one_back() {
    let params = AddParams {
        texts: vec!["N".to_owned()],
        ..AddParams::default()
    };
    let added = "A\nB\n2026-09-29 N id:aaaa\n";
    assert_eq!(round_trip("A\nB\n", "add", |env| add(env, &params)), added);
    // A file without a final newline gets one and gives it up again.
    assert_eq!(round_trip("A\nB", "add", |env| add(env, &params)), added);
    let pair = PriPair {
        target: target("1").remove(0),
        priority: Some(Priority::A),
    };
    let set = round_trip("A\nB\n", "pri", |env| pri(env, &[pair]));
    assert_eq!(set, "(A) A\nB\n");
}

type Shift = fn(&Env<'_>, &[Target], i64) -> Result<Outcome>;

/// The tasks moved, how they are moved, and how far.
const SHIFTS: &[(&str, Shift, i64)] = &[
    ("3", up, 1),
    ("3", down, 1),
    ("1,2", down, 1),
    ("4,5", up, 2),
    ("2,4", up, 1),
    ("5", up, 4),
    ("1", down, 4),
];

#[test]
fn undoing_a_move_puts_the_lines_back_in_their_order() {
    let base = lines(&SEVEN[..5]);
    for (at, (refs, shift, n)) in SHIFTS.iter().enumerate() {
        round_trip(&base, &format!("{at} {refs}"), |env| {
            shift(env, &target(refs), *n)
        });
    }
}

/// Undoes an archive of `todo` into `done` after `appended` was written
/// to the done file; gives the folder and the record of the undo.
fn undone(todo: &str, done: &str, appended: &str) -> (Dir, UndoRecord) {
    let dir = folder(todo, Some(done));
    let rec = archived(&dir);
    let mut bytes = fs::read(&dir.found.done).expect("read done");
    bytes.extend_from_slice(appended.as_bytes());
    fs::write(&dir.found.done, bytes).expect("write done");
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    let back = undone.take_undo().expect("record");
    (dir, back)
}

/// The kinds of change an undo of an archive records for the done file,
/// with `appended` written to the done file in between.
fn done_kinds(appended: &str) -> (Vec<ChangeKind>, String) {
    let (dir, back) =
        undone("A\nx 2026-09-01 D1\n", "x 2026-08-01 Old", appended);
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
fn a_duplicate_of_the_old_last_line_appended_later_keeps_its_newline() {
    let (dir, _) =
        undone("A\nx 2026-09-28 B", "x 2026-09-01 Z", "x 2026-09-01 Z\n");
    assert_eq!(dir.done(), "x 2026-09-01 Z\n\nx 2026-09-01 Z\n");
}

#[test]
fn a_redo_after_such_an_undo_archives_again() {
    let (dir, back) = undone("A\nx 2026-09-01 D1\n", "x 2026-08-01 Old", "");
    assert_eq!(dir.done(), "x 2026-08-01 Old");
    redo(&dir.env(), &back, &Held::of(&back).tracking()).expect("redo");
    assert_eq!(dir.done(), "x 2026-08-01 Old\nx 2026-09-01 D1\n");
    assert_eq!(dir.todo(), "A\n");
}

#[test]
fn undoing_and_redoing_a_block_move_puts_every_line_back() {
    let rows: Vec<String> = (1..=300).map(|i| format!("L{i}")).collect();
    let base = lines(&rows.iter().map(String::as_str).collect::<Vec<_>>());
    let dir = folder(&base, None);
    let refs: Vec<String> = (1..=100).map(|i| i.to_string()).collect();
    let place = super::Placement::Position(300);
    let mut moved =
        super::reorder(&dir.env(), &target(&refs.join(",")), &place)
            .expect("reorder");
    let rec = moved.take_undo().expect("record");
    let changed = dir.todo();
    assert_ne!(changed, base);
    let mut undone =
        undo(&dir.env(), &rec, &Held::of(&rec).tracking()).expect("undo");
    assert_eq!(dir.todo(), base);
    let back = undone.take_undo().expect("record");
    redo(&dir.env(), &back, &Held::of(&back).tracking()).expect("redo");
    assert_eq!(dir.todo(), changed);
}

#[test]
fn a_moved_block_leaves_the_unstaged_lines_paired() {
    let change = |before, after| Change {
        kind: ChangeKind::Moved,
        before: Vec::new(),
        after: Vec::new(),
        nr_before: Nr::new(before),
        nr_after: Nr::new(after),
        own: true,
    };
    let rec = FileRecord::new(
        Source::Todo,
        shared(&["A", "B", "C", "D"]),
        shared(&["B", "C", "D", "A"]),
        Vec::new(),
        vec![change(1, 4)],
    );
    assert_eq!(rec.diff_pre_to_post(), [None, Some(0), Some(1), Some(2)]);
    assert_eq!(rec.pre_to_post(), [Some(3), Some(0), Some(1), Some(2)]);
}

/// The own changes of a record as `source kind text`, then the number
/// of changes that are not.
fn owned(rec: &UndoRecord) -> (Vec<String>, usize) {
    let all = rec.files.iter().flat_map(|f| {
        f.changes.iter().map(move |c| {
            let text = if c.after.is_empty() {
                &c.before
            } else {
                &c.after
            };
            let text = String::from_utf8_lossy(text);
            (c.own, format!("{:?} {:?} {text}", f.source, c.kind))
        })
    });
    let (own, side): (Vec<_>, Vec<_>) = all.partition(|(own, _)| *own);
    (own.into_iter().map(|(_, c)| c).collect(), side.len())
}

#[test]
fn a_record_marks_the_changes_the_action_made_itself() {
    let dir = folder("B\nA\n", None);
    let mut outcome =
        super::dep(&dir.env(), &target("1"), &target("2"), None).expect("dep");
    let rec = outcome.take_undo().expect("record");
    let (own, side) = owned(&rec);
    assert_eq!(own.len(), 1, "{own:?}");
    assert!(own[0].starts_with("Todo Changed B dep:"), "{own:?}");
    assert_eq!(side, 1, "the id given to A");
    let mut at = rec;
    for step in ["undo", "redo", "undo"] {
        let held = Held::of(&at);
        let mut next = if step == "redo" {
            redo(&dir.env(), &at, &held.tracking())
        } else {
            undo(&dir.env(), &at, &held.tracking())
        }
        .expect(step);
        at = next.take_undo().expect("record");
        let (own, side) = owned(&at);
        assert_eq!((own.len(), side), (1, 1), "{step}: {own:?}");
        assert!(own[0].contains(" B"), "{step}: {own:?}");
    }
}

#[test]
fn a_move_to_the_done_file_is_the_actions_own_and_the_spawn_is_not() {
    let mut dir = folder("A rec:1w due:2026-09-30\nB\n", None);
    dir.config.auto_archive = true;
    let mut outcome =
        finish(&dir.env(), &target("1"), DoOpts::default()).expect("do");
    let rec = outcome.take_undo().expect("record");
    let (own, side) = owned(&rec);
    assert_eq!(own.len(), 2, "{own:?}");
    assert_eq!(side, 1, "the next occurrence");
    let held = Held::of(&rec);
    let mut undone = undo(&dir.env(), &rec, &held.tracking()).expect("undo");
    let (own, side) = owned(&undone.take_undo().expect("record"));
    assert_eq!((own.len(), side), (2, 1), "{own:?}");
}
