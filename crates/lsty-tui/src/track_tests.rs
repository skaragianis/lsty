use std::sync::Arc;

use lsty_core::{
    errs::{Nr, Source},
    ops::{Change, ChangeKind, FileRecord, UndoRecord},
};

use crate::{
    test_util::{at, snapshot},
    track::{Place, Remap},
};

const NOW: &str = "2026-09-29T0900Z";

/// (old todo, new todo, place before, place after)
type Row = (&'static str, &'static str, u32, u32);

const TODO_ROWS: [Row; 12] = [
    // Lines that stay, whatever comes above or below.
    ("A\nB\nC\n", "A\nB\nC\nD\n", 3, 3),
    ("A\nB\nC\n", "A\nB\nX\nY\nC\n", 3, 5),
    ("A\nB\nC\n", "B\nC\n", 3, 2),
    // A unique line that moved is followed.
    ("A\nB\nC\n", "C\nA\nB\n", 3, 1),
    // A line that is gone: the place after the nearest earlier line.
    ("A\nB\nC\nD\n", "A\nB\nD\n", 3, 3),
    ("A\nB\nC\nD\n", "A\nD\n", 3, 2),
    ("A\nB\nC\n", "A\nB\n", 3, 3),
    // Nothing earlier is left: the first line.
    ("A\nB\nC\n", "D\nE\n", 2, 1),
    ("A\nB\nC\n", "", 2, 1),
    // A line changed in place stays at its place.
    ("A\nB\nC\nD\n", "A\nB\nC x:1\nD\n", 3, 3),
    // A duplicate that moves counts as gone.
    ("A\nB\nC\nC\nD\n", "C\nA\nB\nC\nD\n", 4, 4),
    // A place beyond the old lines.
    ("A\n", "A\nB\n", 5, 2),
];

#[test]
fn a_todo_line_goes_where_it_went() {
    for (old, new, before, after) in TODO_ROWS {
        let map = Remap::new(
            &snapshot(old, "", NOW),
            &snapshot(new, "", NOW),
            None,
            &[],
        );
        assert_eq!(
            map.place(at(Source::Todo, before)),
            at(Source::Todo, after),
            "{old:?} to {new:?}, line {before}"
        );
    }
}

#[test]
fn the_two_files_are_followed_apart() {
    let old = snapshot("A\nB\n", "x C\nx D\n", NOW);
    let new = snapshot("B\n", "x D\n", NOW);
    let map = Remap::new(&old, &new, None, &[]);
    assert_eq!(map.place(at(Source::Todo, 2)), at(Source::Todo, 1));
    assert_eq!(map.place(at(Source::Done, 2)), at(Source::Done, 1));
    assert_eq!(map.place(at(Source::Done, 1)), at(Source::Done, 1));
}

fn lines(text: &str) -> Vec<Arc<[u8]>> {
    text.lines().map(|l| l.as_bytes().into()).collect()
}

fn change(
    kind: ChangeKind,
    before: Option<u32>,
    after: Option<u32>,
) -> Change {
    Change {
        kind,
        before: Vec::new(),
        after: Vec::new(),
        nr_before: before.and_then(Nr::new),
        nr_after: after.and_then(Nr::new),
        own: true,
    }
}

fn record(pre: &str, post: &str, changes: Vec<Change>) -> UndoRecord {
    UndoRecord {
        action: "test".to_owned(),
        files: vec![FileRecord::new(
            Source::Todo,
            lines(pre),
            lines(post),
            Vec::new(),
            changes,
        )],
    }
}

/// A line that the operation moved is followed to where it went, which
/// a diff cannot tell when the line has a twin.
#[test]
fn a_moved_line_is_followed_by_its_number() {
    let (old, new) = ("A\nA\nB\n", "A\nB\nA\n");
    let rec =
        record(old, new, vec![change(ChangeKind::Moved, Some(1), Some(3))]);
    let map = Remap::new(
        &snapshot(old, "", NOW),
        &snapshot(new, "", NOW),
        Some(&rec),
        &[],
    );
    assert_eq!(map.place(at(Source::Todo, 1)), at(Source::Todo, 3));
    let plain = Remap::new(
        &snapshot(old, "", NOW),
        &snapshot(new, "", NOW),
        None,
        &[],
    );
    assert_ne!(plain.place(at(Source::Todo, 1)), at(Source::Todo, 3));
}

/// Another writer changed the file before the operation read it: the
/// lines go from what was shown, through what was read, to what was
/// written.
#[test]
fn lines_are_followed_through_a_file_that_changed_before_the_read() {
    let (shown, read, new) = ("A\nB\n", "X\nA\nB\n", "X\nB\nA\n");
    let rec =
        record(read, new, vec![change(ChangeKind::Moved, Some(2), Some(3))]);
    let map = Remap::new(
        &snapshot(shown, "", NOW),
        &snapshot(new, "", NOW),
        Some(&rec),
        &[],
    );
    assert_eq!(map.place(at(Source::Todo, 1)), at(Source::Todo, 3));
}

#[test]
fn a_line_the_operation_blanked_or_removed_is_gone() {
    let (old, new) = ("A\nB\nC\n", "A\n\nC\n");
    let rec = record(
        old,
        new,
        vec![change(ChangeKind::Blanked, Some(2), Some(2))],
    );
    let map = Remap::new(
        &snapshot(old, "", NOW),
        &snapshot(new, "", NOW),
        Some(&rec),
        &[],
    );
    assert_eq!(map.track(at(Source::Todo, 1)), Some(at(Source::Todo, 1)));
    assert_eq!(map.track(at(Source::Todo, 2)), None);
    assert_eq!(map.track(at(Source::Todo, 3)), Some(at(Source::Todo, 3)));
    let (old, new) = ("A\nB\nC\n", "A\nC\n");
    let rec =
        record(old, new, vec![change(ChangeKind::Removed, Some(2), None)]);
    let map = Remap::new(
        &snapshot(old, "", NOW),
        &snapshot(new, "", NOW),
        Some(&rec),
        &[],
    );
    assert_eq!(map.track(at(Source::Todo, 2)), None);
    assert_eq!(map.track(at(Source::Todo, 3)), Some(at(Source::Todo, 2)));
}

/// A line that changed where it stands stays; one that another line
/// took the place of does not.
#[test]
fn a_changed_line_is_tracked_only_while_it_stands_alone() {
    let map = Remap::new(
        &snapshot("A\nB\nC\n", "", NOW),
        &snapshot("A\nB2\nC\n", "", NOW),
        None,
        &[],
    );
    assert_eq!(map.track(at(Source::Todo, 2)), Some(at(Source::Todo, 2)));
    assert_eq!(map.track(at(Source::Todo, 4)), None);
}

/// Where lines 1 to 4 of the todo file go through a move, with `follow`
/// the lines followed.
fn through_move(follow: &[Place]) -> Vec<Place> {
    let (old, new) = ("A\nX\nX\nB\n", "A\nX\nB\nX\n");
    let rec =
        record(old, new, vec![change(ChangeKind::Moved, Some(2), Some(4))]);
    let map = Remap::new(
        &snapshot(old, "", NOW),
        &snapshot(new, "", NOW),
        Some(&rec),
        follow,
    );
    (1..=4).map(|n| map.place(at(Source::Todo, n))).collect()
}

#[test]
fn a_followed_twin_is_paired_by_the_full_diff() {
    let nrs =
        |at: Vec<Place>| at.iter().map(|p| p.1.get()).collect::<Vec<_>>();
    // Lines of unique text go the same way by either diff, and following
    // one keeps the quick diff: the twin pairs with the one that stayed.
    assert_eq!(nrs(through_move(&[at(Source::Todo, 1)])), [1, 4, 2, 3]);
    assert_eq!(nrs(through_move(&[])), [1, 4, 2, 3]);
    // Following the twin pairs it as the full diff does.
    assert_eq!(nrs(through_move(&[at(Source::Todo, 3)]))[2], 4);
    // A moved line is followed exactly, so its twin does not count.
    assert_eq!(nrs(through_move(&[at(Source::Todo, 2)])), [1, 4, 2, 3]);
}
