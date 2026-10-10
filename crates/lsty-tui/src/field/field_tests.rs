use lsty_core::keys::{Action, Mods, Named, Stroke};

use super::Field;
use crate::{
    segments::{Segments, cleared, line_field},
    wrap::Cell,
};

const E: &str = "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01";

/// What a row of the editing table does to the field.
#[derive(Clone, Copy, Debug)]
enum Do {
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    ToStart,
    WordBack,
    Paste(&'static str),
    Type(char),
}

fn apply(field: &mut Field, what: Do) {
    match what {
        Do::Backspace => field.typed(Stroke::named(Named::Bs)),
        Do::Delete => field.typed(Stroke::named(Named::Del)),
        Do::Left => field.act(Action::CursorLeft),
        Do::Right => field.act(Action::CursorRight),
        Do::Home => field.act(Action::LineStart),
        Do::ToStart => field.act(Action::DeleteToStart),
        Do::WordBack => field.act(Action::DeleteWordBack),
        Do::Paste(text) => field.paste(text),
        Do::Type(c) => field.typed(Stroke::char(c)),
    }
}

/// Name, the line, where the cursor starts, what is done how many
/// times, the text after and where the cursor ends.
type Row = (
    &'static str,
    &'static str,
    usize,
    Do,
    usize,
    &'static str,
    usize,
);

const ROWS: &[Row] = &[
    (
        "38.10 backspace stops at the date",
        E,
        15,
        Do::Backspace,
        20,
        E,
        15,
    ),
    (
        "38.9 delete the priority field",
        E,
        0,
        Do::Delete,
        4,
        "2026-09-20 Call client +work id:c1 due:2026-10-01",
        0,
    ),
    (
        "delete stops at the date",
        E,
        0,
        Do::Delete,
        9,
        "2026-09-20 Call client +work id:c1 due:2026-10-01",
        0,
    ),
    (
        "38.11 delete to the start of the text after the id",
        E,
        53,
        Do::ToStart,
        1,
        "(A) 2026-09-20 Call client +work id:c1",
        38,
    ),
    (
        "delete word back stops at the id",
        E,
        38,
        Do::WordBack,
        3,
        E,
        38,
    ),
    ("38.12 right jumps over the id", E, 33, Do::Right, 1, E, 38),
    ("left jumps over the id", E, 38, Do::Left, 1, E, 33),
    (
        "backspace after the id keeps it",
        E,
        38,
        Do::Backspace,
        1,
        E,
        38,
    ),
    ("delete before the id keeps it", E, 33, Do::Delete, 1, E, 33),
    (
        "38.14 the done mark can't be reached",
        "x 2026-09-28 Fix",
        13,
        Do::Left,
        5,
        "x 2026-09-28 Fix",
        2,
    ),
    (
        "home goes to the first place",
        "x 2026-09-28 Fix",
        16,
        Do::Home,
        1,
        "x 2026-09-28 Fix",
        2,
    ),
    (
        "text typed before an id moves it on",
        "id:a Buy",
        0,
        Do::Paste("hi"),
        1,
        "hi id:a Buy",
        2,
    ),
    (
        "a letter after an id is set apart from it",
        "Buy id:c1",
        9,
        Do::Type('2'),
        1,
        "Buy id:c1 2",
        11,
    ),
    (
        "a letter before an id is set apart from it",
        "id:a Buy",
        0,
        Do::Type('x'),
        1,
        "x id:a Buy",
        1,
    ),
    (
        "a space next to an id is kept as typed",
        "Buy id:c1",
        9,
        Do::Type(' '),
        1,
        "Buy id:c1 ",
        10,
    ),
    (
        "backspace before an id keeps it apart from the text",
        "Call id:c1",
        5,
        Do::Backspace,
        1,
        "Call id:c1",
        5,
    ),
    (
        "delete before an id keeps it apart from the text",
        "Buy x id:c1",
        5,
        Do::Delete,
        1,
        "Buy x id:c1",
        5,
    ),
    (
        "delete after an id keeps it apart from the text",
        "Buy id:c1 more",
        9,
        Do::Delete,
        1,
        "Buy id:c1 more",
        9,
    ),
    (
        "delete to start after an id keeps a space",
        "Buy id:c1 more",
        10,
        Do::ToStart,
        1,
        "Buy id:c1 more",
        9,
    ),
    (
        "delete word back after an id keeps a space",
        "Buy id:c1 more",
        10,
        Do::WordBack,
        1,
        "Buy id:c1 more",
        9,
    ),
    (
        "right skips the place between two stamps",
        "x 2026-09-28 2026-09-20 Fix",
        2,
        Do::Right,
        1,
        "x 2026-09-28 2026-09-20 Fix",
        24,
    ),
    (
        "left skips the place between two stamps",
        "x 2026-09-28 2026-09-20 Fix",
        24,
        Do::Left,
        1,
        "x 2026-09-28 2026-09-20 Fix",
        2,
    ),
    (
        "38.48 a paste turns breaks into spaces",
        "",
        0,
        Do::Paste("one\r\ntwo"),
        1,
        "one two",
        7,
    ),
    (
        "a paste turns a tab into a space",
        "",
        0,
        Do::Paste("a\tb"),
        1,
        "a b",
        3,
    ),
    (
        "wide characters step by one",
        "日本語",
        3,
        Do::Left,
        2,
        "日本語",
        1,
    ),
];

#[test]
fn a_field_edits_around_its_read_only_parts() {
    for (name, raw, at, what, times, want, cursor) in ROWS {
        let mut field = line_field(raw, *at);
        for _ in 0..*times {
            apply(&mut field, *what);
        }
        assert_eq!(field.text(), *want, "{name}");
        assert_eq!(field.cursor, *cursor, "{name}");
    }
}

/// Name, the line and what is read-only, the floor and where the
/// description begins.
type Seg = (
    &'static str,
    &'static str,
    &'static [(usize, usize)],
    usize,
    usize,
);

const SEGMENTS: &[Seg] = &[
    ("38.1 open item", E, &[(4, 15), (33, 38)], 0, 15),
    (
        "38.14 done item",
        "x 2026-09-28 Fix",
        &[(0, 2), (2, 13)],
        2,
        13,
    ),
    (
        "done with two dates and a priority",
        "x (A) 2026-09-28 2026-09-20 Fix",
        &[(0, 2), (6, 17), (17, 28)],
        2,
        28,
    ),
    ("no stamps", "Buy milk", &[], 0, 0),
    ("id first", "id:a Buy", &[(0, 4)], 0, 0),
    ("only the first id", "Buy id:a id:b", &[(4, 8)], 0, 0),
    ("wide text before the id", "日本 id:a", &[(3, 7)], 0, 0),
];

#[test]
fn a_line_has_segments_that_cannot_be_changed() {
    for (name, raw, ro, floor, desc) in SEGMENTS {
        let want = Segments {
            ro: ro.to_vec(),
            floor: *floor,
            desc: *desc,
        };
        assert_eq!(Segments::of(raw), want, "{name}");
    }
}

/// Name, the line, the line without its text and where that began.
type Cleared = (&'static str, &'static str, &'static str, usize);

const CLEARED: &[Cleared] = &[
    (
        "38.4 the text goes, the tags stay",
        E,
        "(A) 2026-09-20  +work id:c1 due:2026-10-01",
        15,
    ),
    ("text after a tag", "+a Call", "+a ", 3),
    ("text between tags", "+a Call +b", "+a  +b", 3),
    ("nothing but text", "Buy milk", "", 0),
    (
        "a done item keeps its stamps",
        "x 2026-09-28 Fix +w",
        "x 2026-09-28  +w",
        13,
    ),
];

#[test]
fn clearing_a_line_keeps_what_is_not_text() {
    for (name, raw, want, at) in CLEARED {
        assert_eq!(cleared(raw), ((*want).to_owned(), *at), "{name}");
    }
}

/// Name, the text, the width of a row, the number of rows and the cell
/// after the last character.
type Wrap = (&'static str, &'static str, usize, usize, (usize, usize));

const WRAPS: &[Wrap] = &[
    ("fits", "abc", 10, 1, (0, 3)),
    ("empty", "", 10, 1, (0, 0)),
    ("wraps", "abcdefgh", 3, 3, (2, 2)),
    ("fills row", "abcdef", 3, 3, (2, 0)),
    ("wide characters count two cells", "日本語", 5, 2, (1, 2)),
];

#[test]
fn a_field_wraps_at_the_width_of_its_rows() {
    for (name, text, width, rows, (row, col)) in WRAPS {
        let layout = Field::new(text).layout(*width);
        let end = layout.cell(text.chars().count());
        assert_eq!(layout.rows, *rows, "{name}");
        assert_eq!(
            end,
            Cell {
                row: *row,
                col: *col
            },
            "{name}"
        );
    }
}

/// Name, the text, the cursor, the moves (true for up) and where the
/// cursor ends.
type Vertical = (&'static str, &'static str, usize, &'static [bool], usize);

const VERTICAL: &[Vertical] = &[
    ("38.43 up keeps the column", "abcdefghij", 9, &[true], 5),
    (
        "up on the first row does nothing",
        "abcdefghij",
        2,
        &[true],
        2,
    ),
    (
        "down on the last row does nothing",
        "abcdefghij",
        9,
        &[false],
        9,
    ),
    (
        "the column is kept over a short row",
        "abcdefghi",
        9,
        &[true, true, false, false],
        9,
    ),
];

#[test]
fn the_cursor_moves_between_wrapped_rows() {
    for (name, text, cursor, moves, want) in VERTICAL {
        let mut field = Field::new(text);
        field.cursor = *cursor;
        for up in *moves {
            field.vertical(4, *up);
        }
        assert_eq!(field.cursor, *want, "{name}");
    }
}

#[test]
fn a_paste_forgets_the_column() {
    let mut field = Field::new("abcdefghij");
    field.cursor = 9;
    field.vertical(4, true);
    field.paste("x");
    field.vertical(4, true);
    assert_eq!(field.cursor, 2);
}

#[test]
fn the_place_between_two_stamps_is_not_valid() {
    let field = line_field("x 2026-09-28 2026-09-20 Fix", 27);
    assert!(!field.valid(13));
}

#[test]
fn only_text_and_the_editing_keys_are_typed() {
    let mut field = Field::new("");
    let ctrl = Mods::NONE.with(Mods::CTRL);
    for stroke in [
        Stroke::char('a'),
        Stroke::char('b'),
        Stroke::named(Named::Bs),
        Stroke::named(Named::Tab),
        Stroke::char('c').with_mods(ctrl),
        Stroke::char('d'),
    ] {
        field.typed(stroke);
    }
    assert_eq!(field.text(), "ad");
}

#[test]
fn replacing_a_word_moves_the_read_only_part() {
    let mut field = Field::locked("A +w id:c1", vec![(5, 10)], 0, 0);
    field.replace(2, 4, "+work");
    assert_eq!(field.text(), "A +work id:c1");
    assert_eq!(field.ro, [(8, 13)]);
}
