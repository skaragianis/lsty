use super::{
    AddParams, ChangeKind, Edited, Pin, Place, Seen, Settle, Tracked, add,
    edit_pinned,
    test_util::{Dir, folder, snapshot},
};
use crate::errs::{Code, Nr};

const BASE: &str = "(A) 2026-09-20 Call client +work id:c1";
const RING: &str = "(A) 2026-09-20 Ring client +work id:c1";

fn seen_of(dir: &Dir, text: &str) -> (Seen, Vec<String>) {
    let snap = snapshot(dir, text.as_bytes(), b"");
    let lines = text.lines().map(str::to_owned).collect();
    (Seen::of(&snap), lines)
}

fn tracked(dir: &Dir, seen: &str, at: usize, settle: Settle) -> Tracked {
    let (seen, lines) = seen_of(dir, seen);
    let nr = Nr::new(u32::try_from(at + 1).unwrap()).unwrap();
    Tracked::new(Pin::new(nr, lines[at].as_str(), settle), seen)
}

/// Name, the lines seen, the file now, the done file now, the line
/// edited, the draft, the file after, the done file after and what is
/// said at the end.
struct Row {
    name: &'static str,
    seen: String,
    todo: String,
    done: Option<&'static str>,
    at: usize,
    draft: String,
    want: String,
    said: &'static str,
}

fn row(name: &'static str, seen: String, todo: String) -> Row {
    Row {
        name,
        seen,
        todo,
        done: None,
        at: 0,
        draft: RING.to_owned(),
        want: String::new(),
        said: "",
    }
}

const DELETED: &str = "the item was deleted meanwhile \u{2014} your edit was added as a new item";
const ARCHIVED: &str = "the item was archived meanwhile \u{2014} your edit was added as a new item";
const DONE_C1: &str = "x 2026-09-29 2026-09-20 Call client +work id:c1 pri:A";
const DONE_C1_NL: &str =
    "x 2026-09-29 2026-09-20 Call client +work id:c1 pri:A\n";

fn rows() -> Vec<Row> {
    let mut all = moved_rows();
    all.extend(gone_rows());
    all
}

fn moved_rows() -> Vec<Row> {
    let claim = " owner:agent1";
    vec![
        Row {
            want: format!("New\n{RING}{claim}\n"),
            ..row(
                "40.16 merged where the line moved",
                format!("{BASE}\n"),
                format!("New\n{BASE}{claim}\n"),
            )
        },
        Row {
            at: 1,
            want: format!("N1\nN2\nTop\n{RING}\nTail\n"),
            ..row(
                "40.17 written as typed at the new place",
                format!("Top\n{BASE}\n"),
                format!("N1\nN2\nTop\n{BASE}\nTail\n"),
            )
        },
        Row {
            want: "x 2026-09-29 2026-09-20 Ring client +work id:c1 pri:A\n"
                .to_owned(),
            said: "the item was completed meanwhile",
            ..row(
                "40.32 completed meanwhile",
                format!("{BASE}\n"),
                format!("{DONE_C1}\n"),
            )
        },
    ]
}

fn gone_rows() -> Vec<Row> {
    let started = "started:2026-09-29T0900Z owner:agent1";
    vec![
        Row {
            at: 1,
            want: "Top\n(A) 2026-09-29 Ring client +work\nEnd\n".to_owned(),
            said: DELETED,
            ..row(
                "40.24 a copy in the blank line",
                format!("Top\n{BASE}\nEnd\n"),
                "Top\n\nEnd\n".to_owned(),
            )
        },
        Row {
            at: 1,
            want: "Top\n(A) 2026-09-29 Ring client +work\nEnd\n".to_owned(),
            ..row(
                "40.25 a copy after the nearest earlier neighbour",
                format!("Top\n{BASE}\nEnd\n"),
                "Top\nEnd\n".to_owned(),
            )
        },
        Row {
            want: "(A) 2026-09-29 Ring client +work\nEnd\n".to_owned(),
            ..row(
                "a copy at the top",
                format!("{BASE}\nEnd\n"),
                "End\n".to_owned(),
            )
        },
        Row {
            at: 1,
            done: Some(DONE_C1_NL),
            want: "Top\n(A) 2026-09-29 Ring client +work\n".to_owned(),
            said: ARCHIVED,
            ..row(
                "40.26 an archived item stays in the done file",
                format!("Top\n{BASE}\n"),
                "Top\n".to_owned(),
            )
        },
        Row {
            at: 1,
            draft: format!("{RING} {started}"),
            want: "Top\n(A) 2026-09-29 Ring client +work\n".to_owned(),
            ..row(
                "40.27 a copy has no claim",
                format!("Top\n{BASE} {started}\n"),
                "Top\n".to_owned(),
            )
        },
    ]
}

#[test]
fn an_edit_is_written_where_its_line_is_now() {
    for r in rows() {
        let dir = folder(&r.todo, r.done);
        let tr = tracked(&dir, &r.seen, r.at, Settle::LineOrBlank);
        let Ok(Edited::Written(out)) =
            edit_pinned(&dir.env(), &tr, r.draft.as_bytes(), &[])
        else {
            panic!("{}: not written", r.name);
        };
        assert_eq!(dir.todo(), r.want, "{}", r.name);
        if let Some(done) = r.done {
            assert_eq!(dir.done(), done, "{}", r.name);
        }
        let said: Vec<String> = out
            .messages()
            .iter()
            .map(|m| String::from_utf8_lossy(m).into_owned())
            .collect();
        assert!(
            r.said.is_empty() || said.iter().any(|m| m == r.said),
            "{}: {said:?}",
            r.name
        );
        assert!(out.undo().is_some(), "{}", r.name);
    }
}

#[test]
fn a_clash_writes_nothing_and_gives_the_rebased_line_back() {
    let disk = "(A) 2026-09-20 Call the client +work id:c1\n";
    let dir = folder(disk, None);
    let tr = tracked(&dir, &format!("{BASE}\n"), 0, Settle::LineOrBlank);
    let got = edit_pinned(&dir.env(), &tr, RING.as_bytes(), &[]).unwrap();
    let Edited::Conflict(c) = got else {
        panic!("wanted a conflict");
    };
    assert_eq!(c.merged(), RING.as_bytes());
    assert_eq!(c.disk(), disk.trim_end().as_bytes());
    let e = c.error();
    assert_eq!(e.code(), Code::EditConflict);
    assert_eq!(
        e.message(),
        "changed on disk meanwhile: text (\"Call the client\")"
    );
    assert_eq!(e.hint(), Some("<CR> writes your version"));
    assert_eq!(dir.todo(), disk);
}

#[test]
fn a_priority_change_of_an_task_that_is_done_meanwhile_is_refused() {
    let dir = folder(format!("{DONE_C1}\n"), None);
    let tr = tracked(&dir, &format!("{BASE}\n"), 0, Settle::LineOrBlank);
    let draft = "(B) 2026-09-20 Call client +work id:c1";
    let err = edit_pinned(&dir.env(), &tr, draft.as_bytes(), &[]).unwrap_err();
    assert_eq!(err.code(), Code::WrongState);
    assert_eq!(dir.todo(), format!("{DONE_C1}\n"));
}

#[test]
fn a_merge_that_would_turn_text_into_a_stamp_is_refused() {
    let dir = folder("foo +p\n", None);
    let tr = tracked(&dir, "+p a\n", 0, Settle::LineOrBlank);
    let err = edit_pinned(&dir.env(), &tr, b"+p x marks", &[]).unwrap_err();
    assert_eq!(err.code(), Code::WrongState);
    assert_eq!(
        err.hint(),
        Some("the text would start with a stamp-like word; reword it first")
    );
    assert_eq!(dir.todo(), "foo +p\n");
}

#[test]
fn a_copy_that_cannot_be_made_is_refused() {
    let dir = folder("Top\n", None);
    let tr = tracked(&dir, &format!("Top\n{BASE}\n"), 1, Settle::LineOrBlank);
    let err =
        edit_pinned(&dir.env(), &tr, b"2026-10-01 id:c1", &[]).unwrap_err();
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(dir.todo(), "Top\n");
}

/// Name, the lines seen, the file now, the anchor, below it, the file
/// after.
type Beside = (
    &'static str,
    &'static str,
    &'static str,
    usize,
    bool,
    &'static str,
);

const BESIDE: &[Beside] = &[
    (
        "40.28 after the neighbour of a deleted anchor",
        "Top\nA\nEnd\n",
        "Top\nEnd\n",
        1,
        true,
        "Top\n2026-09-29 N\nEnd\n",
    ),
    (
        "40.28 at the top",
        "A\nEnd\n",
        "End\n",
        0,
        false,
        "2026-09-29 N\nEnd\n",
    ),
    (
        "above a live anchor",
        "Top\nA\n",
        "X\nTop\nA\n",
        1,
        false,
        "X\nTop\n2026-09-29 N\nA\n",
    ),
    (
        "below a live anchor",
        "Top\nA\n",
        "X\nTop\nA\nZ\n",
        1,
        true,
        "X\nTop\nA\n2026-09-29 N\nZ\n",
    ),
];

#[test]
fn a_new_task_goes_next_to_its_anchor_wherever_that_is() {
    for (name, seen, todo, at, below, want) in BESIDE {
        let dir = folder(todo, None);
        let params = AddParams {
            texts: vec!["N".to_owned()],
            place: Place::Beside {
                anchor: tracked(&dir, seen, *at, Settle::Line),
                below: *below,
            },
            ..AddParams::default()
        };
        add(&dir.env(), &params).unwrap();
        assert_eq!(dir.todo(), *want, "{name}");
    }
}

/// What the record of the edit of row `name` holds: the kind, the text
/// before and the text after of its one change, and how many files and
/// changes it has.
fn recorded(name: &str) -> (ChangeKind, String, String) {
    let r = rows()
        .into_iter()
        .find(|r| r.name.starts_with(name))
        .unwrap();
    let dir = folder(&r.todo, r.done);
    let tr = tracked(&dir, &r.seen, r.at, Settle::LineOrBlank);
    let Ok(Edited::Written(out)) =
        edit_pinned(&dir.env(), &tr, r.draft.as_bytes(), &[])
    else {
        panic!("{name}: not written");
    };
    let rec = out.undo().unwrap();
    assert_eq!(rec.files.len(), 1, "{name}");
    let [change] = rec.files[0].changes.as_slice() else {
        panic!("{name}: {} changes", rec.files[0].changes.len());
    };
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    (change.kind, text(&change.before), text(&change.after))
}

#[test]
fn a_merged_edit_is_one_step_from_the_disk_line_to_the_merged_line() {
    let (kind, before, after) = recorded("40.16");
    assert_eq!(kind, ChangeKind::Changed);
    assert_eq!(before, format!("{BASE} owner:agent1"));
    assert_eq!(after, format!("{RING} owner:agent1"));
}

#[test]
fn a_copy_is_one_step_that_removes_the_copy() {
    let (kind, before, after) = recorded("40.24");
    assert_eq!(kind, ChangeKind::Changed);
    assert_eq!(before, "");
    assert_eq!(after, "(A) 2026-09-29 Ring client +work");
    let (kind, before, _) = recorded("40.25");
    assert_eq!(kind, ChangeKind::Added);
    assert_eq!(before, "");
}

/// The base, the line on disk and what is said about the task having
/// been completed or reopened.
type Life = (&'static str, &'static str, &'static [&'static str]);

const LIFE: &[Life] = &[
    (
        "Call client +work id:c1",
        "x 2026-09-29 Call client +work id:c1",
        &["the item was completed meanwhile"],
    ),
    (
        "x 2026-09-28 Call client +work id:c1",
        "Call client +work id:c1",
        &["the item was reopened meanwhile"],
    ),
    (
        "x 2026-09-28 Call client +work id:c1",
        "x 2026-09-29 Call client +work id:c1",
        &[],
    ),
    (
        "Call client +work id:c1",
        "Call client +work id:c1 @desk",
        &[],
    ),
];

#[test]
fn a_merge_says_when_the_task_was_completed_or_reopened_meanwhile() {
    for (base, disk, want) in LIFE {
        let dir = folder(format!("{disk}\n"), None);
        let tr = tracked(&dir, &format!("{base}\n"), 0, Settle::LineOrBlank);
        let draft = format!("{base} +new");
        let Ok(Edited::Written(out)) =
            edit_pinned(&dir.env(), &tr, draft.as_bytes(), &[])
        else {
            panic!("{base} / {disk}: not written");
        };
        let said: Vec<String> = out
            .messages()
            .iter()
            .map(|m| String::from_utf8_lossy(m).into_owned())
            .filter(|m| m.ends_with("meanwhile"))
            .collect();
        assert_eq!(said, *want, "{base} / {disk}");
    }
}
