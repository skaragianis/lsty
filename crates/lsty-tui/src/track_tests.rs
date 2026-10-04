use lsty_core::errs::Source;

use crate::test_util::snapshot;
use crate::track::Remap;

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
        let map = Remap::new(&snapshot(old, "", NOW), &snapshot(new, "", NOW));
        assert_eq!(
            map.place((Source::Todo, before)),
            (Source::Todo, after),
            "{old:?} to {new:?}, line {before}"
        );
    }
}

#[test]
fn the_two_files_are_followed_apart() {
    let old = snapshot("A\nB\n", "x C\nx D\n", NOW);
    let new = snapshot("B\n", "x D\n", NOW);
    let map = Remap::new(&old, &new);
    assert_eq!(map.place((Source::Todo, 2)), (Source::Todo, 1));
    assert_eq!(map.place((Source::Done, 2)), (Source::Done, 1));
    assert_eq!(map.place((Source::Done, 1)), (Source::Done, 1));
}
