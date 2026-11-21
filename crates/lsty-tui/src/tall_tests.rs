use std::rc::Rc;

use lsty_core::{
    config::{Config, Hidden, LineNumbers},
    theme::{Parts, Slot},
    todotxt::Line,
};
use ratatui::layout::Position;
use tempfile::TempDir;

use crate::{
    model::Model,
    msg::{Button, Gesture, Mouse, Msg},
    rows::{Inline, Shape},
    tall::{Entry, lay_out},
    test_util::{alone, deps, keys, screen, started, workdir, workdir_model},
    text::Span,
    wrap::wrap_words,
};

/// The rows the `lay_out` tests allow.
const MAX_ROWS: usize = 3;

/// Tasks of 1, 2, 3 and 1 rows at 40 cells, then more of each size.
const HEIGHTS: [usize; 12] = [1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 1, 1];

/// A task that takes `rows` rows at 40 cells wide, named `n`.
fn task(n: usize, rows: usize) -> String {
    let words = [0, 1, 7, 14][rows.min(3)];
    let text: Vec<String> =
        (0..words).map(|w| format!("w{n:02}{w:02}")).collect();
    text.join(" ")
}

fn todo() -> String {
    let lines: Vec<String> = HEIGHTS
        .iter()
        .enumerate()
        .map(|(n, &rows)| task(n, rows))
        .collect();
    lines.join("\n") + "\n"
}

/// A model of `todo()` with the list alone, `h` lines high, a task taking
/// up to `list_rows` rows.
fn tall(h: u16, list_rows: u64) -> (TempDir, Model) {
    let config = Config {
        list_rows,
        ..alone()
    };
    let (dir, mut model) = workdir_model(&todo(), config);
    model.update(Msg::Resize(40, h));
    (dir, model)
}

fn heights(model: &Model) -> Vec<usize> {
    model.sized(|s| (0..s.len).map(|i| s.height(i)).collect())
}

fn wheel(model: &mut Model, up: bool) {
    model.update(Msg::Mouse(Mouse {
        gesture: Gesture::Wheel { up },
        at: Position::new(10, 3),
        shift: false,
    }));
}

fn whole(model: &Model) -> bool {
    model.sized(|s| s.seen(model.top, model.cursor))
}

#[test]
fn the_tasks_take_the_rows_the_tests_assume() {
    let (_dir, model) = tall(12, 3);
    assert_eq!(heights(&model), HEIGHTS);
    assert_eq!(model.pane_rows(), 9);
    for list_rows in [1, 2, 5] {
        let (_dir, model) = tall(12, list_rows as u64);
        let want = HEIGHTS.map(|h| h.min(list_rows));
        assert_eq!(heights(&model), want, "{list_rows}");
    }
}

#[test]
fn a_task_takes_no_more_rows_than_the_limit() {
    let long = ["w"; 400].join(" ");
    for list_rows in [1, 2, 3, 5] {
        let (_dir, mut model) = workdir_model(
            &long,
            Config {
                list_rows: list_rows as u64,
                ..alone()
            },
        );
        model.update(Msg::Resize(40, 12));
        assert_eq!(heights(&model), [list_rows]);
    }
}

/// (cursor, keys, cursor after): a half page covers half the pane in
/// rows, a page the pane, and both move by the tasks that fill that.
const PAGING: [(usize, &str, usize); 13] = [
    (0, "<C-d>", 2),
    (2, "<C-d>", 4),
    (4, "<C-d>", 5),
    (5, "<C-d>", 7),
    (0, "<C-f>", 5),
    (5, "<C-b>", 0),
    (3, "<C-u>", 2),
    (11, "<C-d>", 11),
    (0, "3<C-d>", 3),
    (6, "2<C-f>", 11),
    (11, "3<C-u>", 8),
    (11, "<C-b>", 6),
    (11, "2<C-b>", 2),
];

#[test]
fn paging_moves_by_the_tasks_that_fill_the_distance() {
    for (from, notation, to) in PAGING {
        let (_dir, mut model) = tall(12, 3);
        if from > 0 {
            keys(&mut model, &format!("{from}j"));
        }
        assert_eq!(model.cursor, from, "{from} {notation}");
        keys(&mut model, notation);
        assert_eq!(model.cursor, to, "{from} {notation}");
        assert!(whole(&model), "{from} {notation}");
    }
}

#[test]
fn a_half_page_moves_one_task_at_least() {
    let (_dir, mut model) = tall(6, 3);
    assert_eq!(model.pane_rows(), 3);
    keys(&mut model, "j<C-d>");
    assert_eq!(model.cursor, 2);
    keys(&mut model, "<C-u>");
    assert_eq!(model.cursor, 1);
    assert!(whole(&model));
}

/// (wheel steps down, then up, top after each).
#[test]
fn the_wheel_moves_by_the_tasks_that_fill_three_rows() {
    let (_dir, mut model) = tall(12, 3);
    let mut tops = Vec::new();
    for _ in 0..5 {
        wheel(&mut model, false);
        tops.push(model.top);
    }
    assert_eq!(tops, [2, 3, 5, 6, 6]);
    assert_eq!(model.cursor, 0);
    let mut tops = Vec::new();
    for _ in 0..4 {
        wheel(&mut model, true);
        tops.push(model.top);
    }
    assert_eq!(tops, [5, 3, 2, 0]);
}

#[test]
fn the_wheel_moves_one_task_at_least() {
    let (_dir, mut model) = tall(6, 3);
    assert_eq!(model.pane_rows(), 3);
    let mut tops = Vec::new();
    for _ in 0..7 {
        wheel(&mut model, false);
        tops.push(model.top);
    }
    assert_eq!(tops, [2, 3, 5, 6, 8, 9, 9]);
}

#[test]
fn the_cursor_task_stays_whole_as_it_moves() {
    let (_dir, mut model) = tall(12, 3);
    for i in 0..HEIGHTS.len() {
        assert_eq!(model.cursor, i);
        assert!(whole(&model), "going down to {i}");
        keys(&mut model, "j");
    }
    for i in (0..HEIGHTS.len()).rev() {
        assert_eq!(model.cursor, i);
        assert!(whole(&model), "going up to {i}");
        keys(&mut model, "k");
    }
}

#[test]
fn the_list_does_not_scroll_past_its_last_task() {
    let (_dir, mut model) = tall(12, 3);
    keys(&mut model, "G");
    assert_eq!((model.cursor, model.top), (11, 6));
    for _ in 0..10 {
        keys(&mut model, "<C-e>");
        wheel(&mut model, false);
    }
    assert_eq!(model.top, 6);
    let lines = screen(&model, 40, 12);
    assert!(lines[1].contains("w0600"), "{lines:?}");
    assert!(lines[9].contains("w1100"), "{lines:?}");
}

#[test]
fn a_row_more_or_fewer_keeps_the_cursor_task_whole() {
    let (_dir, mut model) = tall(12, 1);
    keys(&mut model, "8j");
    assert_eq!((model.top, model.cursor), (0, 8));
    keys(&mut model, "++");
    assert_eq!((model.top, model.cursor), (5, 8));
    let lines = screen(&model, 40, 12);
    assert!(lines[7].contains("w0800"), "{lines:?}");
    assert!(lines[9].contains("w0813"), "{lines:?}");
    fewer_rows_cut_the_top(&mut model);
    cut_view_scrolls_and_opens_whole();
    for (from, typed, rows, lit) in STOPS {
        let (_dir, mut model) = tall(12, from);
        keys(&mut model, typed);
        assert_eq!(model.deps.config.list_rows, rows, "{from} {typed}");
        let back = model.can_default_rows();
        assert_eq!(back, rows != from, "{from} {typed} hint");
        assert_eq!(model.can_add_row(), lit, "{from} {typed}");
    }
}

/// With the top task cut and the cursor task at the bottom, a move down or
/// a field opened on it works on whole tasks: it ends in view.
fn cut_view_scrolls_and_opens_whole() {
    let (_dir, mut model) = tall(12, 1);
    keys(&mut model, "7j+");
    assert_eq!((model.top, model.cursor, model.cut), (2, 7, (2, 1)));
    keys(&mut model, "kL");
    assert_eq!(model.cursor, 7);
    keys(&mut model, "j");
    assert!(whole(&model), "{:?}", (model.top, model.cursor));
    // a drag inside the pane reads the rows as drawn, and keeps the cut
    let (_dir, mut model) = tall(12, 1);
    keys(&mut model, "7j+");
    for (gesture, y) in [
        (Gesture::Down(Button::Left), 2),
        (Gesture::Drag(Button::Left), 3),
    ] {
        let at = Position::new(5, y);
        model.update(Msg::Mouse(Mouse {
            gesture,
            at,
            shift: false,
        }));
    }
    assert_eq!((model.selected_rows(), model.cut), (Some((3, 4)), (2, 1)));
    let (_dir, mut model) = tall(12, 1);
    keys(&mut model, "7j+A");
    let end = model.field_line() + model.field_height();
    assert!(end <= model.pane_rows(), "{end}");
    keys(&mut model, "<Esc>");
    assert_eq!(model.cut.1, 0);
}

/// Line `line` of the screen holds `text`.
fn row_has(model: &Model, line: usize, text: &str) {
    let lines = screen(model, 40, 12);
    assert!(lines[line].contains(text), "{lines:?}");
}

/// `-` keeps the cursor task on its line, the task at the top cut, until
/// the next scroll. The cursor is on task 8, at the bottom of 3 rows each.
fn fewer_rows_cut_the_top(model: &mut Model) {
    // the cursor task keeps its line: the task at the top is cut
    keys(model, "-");
    assert_eq!((model.top, model.cursor, model.cut), (4, 8, (4, 1)));
    assert_eq!(model.task_at_line(0), Some(4));
    row_has(model, 1, "w0405 w0406");
    row_has(model, 7, "w0800");
    // a cut that is gone does not come back: onto the top task and off it
    keys(model, "4k4j");
    assert_eq!((model.top, model.cursor, model.cut.1), (4, 8, 0));
    row_has(model, 1, "w0400");
    // nor after the list is built again
    keys(model, "+-");
    assert_eq!(model.cut, (4, 1));
    model.rebuild();
    assert_eq!(model.cut.1, 0);
    row_has(model, 1, "w0400");
    keys(model, "-");
    assert_eq!((model.top, model.cursor), (1, 8));
    row_has(model, 8, "w0800");
    // the next scroll shows whole tasks again
    keys(model, "+<C-e>");
    assert_eq!(model.sized(|s| s.cut.1), 0);
}

type Stop = (u64, &'static str, u64, bool);

/// (`list_rows` at first, keys, `list_rows` after, whether `+` is lit): both
/// keys stop, `-` drops to one below the longest task, also after a filter;
/// `0` sets the configured rows, `from`, and is dim there.
const STOPS: [Stop; 13] = [
    (1, "++", 3, false),
    (1, "+++", 3, false),
    (3, "---", 1, true),
    (5, "+", 5, false),
    (5, "-", 2, true),
    (5, "+-", 2, true),
    (5, "+-+", 3, false),
    (5, "/w01<CR>-", 1, true),
    (5, "/w01<CR>--", 1, true),
    // `0` goes back to the `list_rows` of the config, and is dim there
    (1, "++0", 1, true),
    (5, "0", 5, false),
    (5, "-0", 5, false),
    (5, ":set list_rows=4<CR>0", 5, false),
];

#[test]
fn a_task_takes_no_more_rows_than_the_list_shows() {
    let (_dir, mut model) = tall(5, 3);
    assert_eq!(model.pane_rows(), 2);
    // a task takes no more rows than the list shows, and `+` stops there
    assert!(heights(&model).iter().all(|&h| h <= 2));
    assert!(!model.can_add_row());
    let set = ["+", "-", "0"].map(|k| {
        keys(&mut model, k);
        model.deps.config.list_rows
    });
    assert_eq!(set, [3, 1, 3]);
    keys(&mut model, "2j");
    assert_eq!((model.top, model.cursor), (2, 2));
    assert!(whole(&model));
    let lines = screen(&model, 40, 5);
    assert!(lines[1].contains("w0200"), "{lines:?}");
    assert!(lines[2].contains("w0208"), "{lines:?}");
    keys(&mut model, "j");
    assert_eq!((model.top, model.cursor), (3, 3));
}

#[test]
fn counts_and_relative_numbers_count_tasks() {
    let (_dir, mut model) = tall(12, 3);
    keys(&mut model, "3j");
    assert_eq!(model.cursor, 3);
    keys(&mut model, "2k");
    assert_eq!(model.cursor, 1);
    let lines = screen(&model, 40, 12);
    let number = |line: &str| {
        line.trim_start_matches('│')
            .trim_start()
            .split(' ')
            .next()
            .map(str::to_owned)
    };
    // task 0 (1 row), then the cursor task (2 rows), then task 2.
    assert_eq!(number(&lines[1]).as_deref(), Some("1"));
    assert_eq!(number(&lines[2]).as_deref(), Some("2"));
    assert_eq!(number(&lines[4]).as_deref(), Some("1"));
    assert_eq!(number(&lines[7]).as_deref(), Some("2"));
}

#[test]
fn a_cursor_task_cut_by_the_bottom_edge_counts_as_out_of_view() {
    let (_dir, mut model) = tall(8, 3);
    assert_eq!(model.pane_rows(), 5);
    keys(&mut model, "5j");
    assert_eq!((model.top, model.cursor), (4, 5));
    model.top = 3;
    assert!(model.cursor_hidden());
    model.reveal();
    assert_eq!(model.top, 4);
    assert!(!model.cursor_hidden());
}

#[test]
fn the_rows_of_a_task_are_worked_out_once_for_a_width() {
    let (_dir, mut model) = tall(12, 3);
    let lines =
        |m: &Model| m.stacked(|s, _| s.tall.as_ref().map(|t| t.lines(1)));
    let (first, again) = (lines(&model), lines(&model));
    assert!(first.zip(again).is_some_and(|(a, b)| Rc::ptr_eq(&a, &b)));
    model.update(Msg::Resize(30, 12));
    let narrow = lines(&model);
    assert_eq!(narrow.map(|l| l.len()), Some(3));
}

#[test]
fn the_rows_an_task_takes_follow_the_width_of_the_list() {
    let (_dir, mut model) = tall(12, 3);
    let wide = heights(&model);
    model.update(Msg::Resize(30, 12));
    let narrow = heights(&model);
    assert!(narrow.iter().sum::<usize>() > wide.iter().sum::<usize>());
    model.update(Msg::Resize(40, 12));
    assert_eq!(heights(&model), wide);
}

/// The rows the words of `text` fall on, the first `first` cells wide
/// and the others `later`.
fn wrapped(text: &str, first: usize, later: usize) -> Vec<String> {
    let mut spans = Vec::new();
    for (n, word) in text.split(' ').enumerate() {
        if n > 0 {
            spans.push(Span::new(" ", Some(Slot::Text)));
        }
        spans.push(Span::new(word, Some(Slot::Text)));
    }
    wrap_words(&spans, |r| if r == 0 { first } else { later })
        .iter()
        .map(|row| row.iter().map(|s| s.text.as_ref()).collect())
        .collect()
}

#[test]
fn a_row_holds_as_many_cells_as_its_width() {
    assert_eq!(wrapped("aaa bbb", 7, 7), ["aaa bbb"]);
    assert_eq!(wrapped(&"w".repeat(51), 52, 50), ["w".repeat(51)]);
    assert_eq!(wrapped(&"w".repeat(52), 52, 50), ["w".repeat(52)]);
    assert_eq!(wrapped("aaa bbb", 6, 6), ["aaa", "bbb"]);
}

#[test]
fn a_last_row_that_is_full_has_no_blank_row_after_it() {
    assert_eq!(wrapped("aaaa bbbb cccc", 4, 4), ["aaaa", "bbbb", "cccc"]);
    assert_eq!(wrapped("aaaa bbbb", 4, 4), ["aaaa", "bbbb"]);
}

#[test]
fn the_row_after_a_word_that_fills_or_overruns_a_row() {
    assert_eq!(wrapped("aaaa bb", 4, 4), ["aaaa", "bb"]);
    assert_eq!(wrapped("abcdefghij kl", 4, 4), ["abcd", "efgh", "ij", "kl"]);
    assert_eq!(wrapped("ab abcdefgh", 4, 4), ["ab", "abcd", "efgh"]);
}

/// The screen of a list of one task of two words, 30 and 26 cells wide,
/// wrapping, with a date column.
fn two_words(numbers: LineNumbers) -> Vec<String> {
    let (_dir, mut model) = workdir_model(
        &format!("{} {} due:2030-12-24\n", "a".repeat(30), "b".repeat(26)),
        Config {
            line_numbers: numbers,
            ..alone()
        },
    );
    model.update(Msg::Resize(60, 8));
    screen(&model, 60, 8)
}

#[test]
fn later_rows_start_under_the_text_beside_a_number_column() {
    for numbers in [LineNumbers::Absolute, LineNumbers::Relative] {
        let lines = two_words(numbers);
        let first = lines[1].find('a').unwrap_or_default();
        assert_eq!(lines[2].find('b'), Some(first), "{lines:?}");
    }
}

#[test]
fn later_rows_start_two_cells_in_without_a_number_column() {
    let lines = two_words(LineNumbers::Off);
    let first = lines[1].find('a').unwrap_or_default();
    assert_eq!(lines[2].find('b'), Some(first + 2), "{lines:?}");
}

#[test]
fn a_field_starts_after_the_number_column_only_when_there_is_one() {
    // The done file is 99 lines: the column is wider when archived rows
    // can be listed, by the switch or by a filter asking for them.
    let dir = workdir("one\n");
    std::fs::write(dir.path().join("done.txt"), "x old\n".repeat(99))
        .unwrap_or_else(|e| panic!("{e}"));
    for (numbers, archive, search, want) in [
        (LineNumbers::Off, false, "", (1, 26)),
        (LineNumbers::Absolute, false, "", (4, 23)),
        (LineNumbers::Absolute, true, "", (5, 22)),
        (LineNumbers::Absolute, false, "/is:archived<CR>", (5, 22)),
    ] {
        let hidden = if archive {
            vec![]
        } else {
            vec![Hidden::Archive]
        };
        let config = Config {
            line_numbers: numbers,
            hidden_tasks: hidden,
            ..alone()
        };
        let mut model = started(deps(dir.path(), config));
        model.update(Msg::Resize(30, 8));
        keys(&mut model, search);
        assert_eq!(model.field_box(), want, "{numbers:?} {archive} {search}");
    }
    // A row just archived with `:archive` stays drawn after it left the
    // file: 100 rows over 99 lines still fit their numbers.
    let todo = (0..100).map(|i| format!("task {i}\n")).collect::<Vec<_>>();
    let dir = workdir(todo.concat());
    let config = Config {
        line_numbers: LineNumbers::Absolute,
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    model.update(Msg::Resize(30, 8));
    keys(&mut model, "Gx:archive<CR>");
    assert_eq!((model.rows.len(), model.snap.todo().lines.len()), (100, 99));
    assert_eq!(model.field_box(), (5, 22), "archived row kept");
}

/// The rows each entry of the list takes, as (task or None, rows).
fn entries(model: &Model) -> Vec<(Option<usize>, usize)> {
    model.stacked(|stack, size| {
        (0..size.len)
            .map(|v| match stack.entry(v) {
                Entry::Row(i) => (Some(i), size.height(v)),
                Entry::Field => (None, size.height(v)),
            })
            .collect()
    })
}

#[test]
fn an_open_field_replaces_the_rows_of_its_own_task_only() {
    for notation in ["i", "a", "o", "O"] {
        let (_dir, mut model) = tall(12, 3);
        keys(&mut model, "4j");
        let before = screen(&model, 40, 12);
        keys(&mut model, notation);
        let replaced = matches!(notation, "i" | "a");
        for (task, rows) in entries(&model) {
            let own = replaced && task == Some(4);
            assert!(!own, "{notation}");
            if let Some(i) = task {
                assert_eq!(rows, HEIGHTS[i], "{notation} {i}");
            }
        }
        keys(&mut model, "<Esc>");
        if replaced {
            assert_eq!(screen(&model, 40, 12), before, "{notation}");
        }
    }
}

#[test]
fn committing_a_field_keeps_the_other_tasks_wrapped() {
    let (_dir, mut model) = tall(12, 3);
    keys(&mut model, "2jonew<CR>");
    assert_eq!(model.cursor, 3);
    let heights = heights(&model);
    assert_eq!(heights[..3], HEIGHTS[..3]);
    assert_eq!(heights[4..], HEIGHTS[3..]);
    assert_eq!(heights[3], 1);
    assert!(whole(&model));
}

#[test]
fn a_field_taller_than_the_list_shows_a_window_of_its_rows() {
    let (_dir, mut model) = tall(5, 3);
    keys(&mut model, "2ji");
    let rows = entries(&model);
    assert!(rows.iter().any(|&(i, n)| i.is_none() && n > 2), "{rows:?}");
    let top = screen(&model, 40, 5);
    assert!(top[1].contains("w0200"), "{top:?}");
    assert!(top[2].contains("w0205"), "{top:?}");
    keys(&mut model, "<End>");
    let end = screen(&model, 40, 5);
    assert!(end[1].contains("w0205"), "{end:?}");
    assert!(end[2].contains("w0210"), "{end:?}");
}

/// (cursor, keys, top and cursor after) on the list of `HEIGHTS` at 12
/// lines, 9 of them for tasks, which starts at its top for the first
/// cases and is as the key left it for the others. Tasks start on rows
/// 0, 1, 3, 6, 7, 9, 12, 13, 15, 18, 19 and 20.
type Placed = (usize, &'static str, (usize, usize));

const PLACING: [Placed; 19] = [
    (4, "zt", (4, 4)),
    (4, "zz", (3, 4)),
    (4, "zb", (0, 4)),
    (5, "zt", (5, 5)),
    (5, "zz", (3, 5)),
    (5, "zb", (2, 5)),
    (8, "zt", (6, 8)),
    (8, "zz", (6, 8)),
    (8, "zb", (5, 8)),
    (0, "M", (0, 2)),
    (0, "L", (0, 4)),
    (0, "H", (0, 0)),
    (11, "H", (6, 6)),
    (11, "M", (6, 8)),
    (11, "L", (6, 11)),
    (11, "<C-y>", (5, 8)),
    (11, "2<C-y>", (4, 7)),
    (3, "<C-y>", (0, 3)),
    (6, "<C-y>", (2, 5)),
];

#[test]
fn placing_the_view_and_the_cursor_counts_the_rows_of_the_tasks() {
    for (from, notation, want) in PLACING {
        let (_dir, mut model) = tall(12, 3);
        keys(&mut model, &format!("{from}j"));
        keys(&mut model, notation);
        assert_eq!((model.top, model.cursor), want, "{from} {notation}");
        assert!(whole(&model), "{from} {notation}");
    }
}

/// The rows of `text` laid out `first` cells wide on the first row,
/// `later` after, `rows` at most.
fn laid(text: &str, widths: (usize, usize), rows: usize) -> Vec<String> {
    let inline = Inline::new(&Config::default());
    lay_out(
        &Line::parse(text.as_bytes()),
        (None, Parts::default()),
        inline,
        Shape { widths, rows },
    )
    .iter()
    .map(|row| row.iter().map(|s| s.text.as_ref()).collect())
    .collect()
}

/// (text, rows at 14 then 12 cells): how projects and contexts the last row
/// would cut or leave out follow the ellipsis, also when the last tag or the
/// cut it needs fills the room exactly.
const TAGS: [(&str, &[&str]); 17] = [
    ("fix it +a @b", &["fix it +a @b"]),
    (
        "+a @b one two three four five six seven",
        &["+a @b one two", "three four", "five six se…"],
    ),
    (
        "aaaa bbbb cccc dddd eeee ff ggg +longtag more",
        &["aaaa bbbb cccc", "dddd eeee ff", "gg… +longtag"],
    ),
    (
        "aaaa bbbb cccc dddd eeee ffff gggg +long",
        &["aaaa bbbb cccc", "dddd eeee", "ffff… +long"],
    ),
    (
        "aaaa bbbb cccc dddd eeee @c1 @c2 +p1 +p2",
        &["aaaa bbbb cccc", "dddd ee… @c1", "@c2 +p1 +p2"],
    ),
    (
        "aaaa bbbb cccc dddd eeee @c1 @c2 @c3 +p1 +p2 +p3",
        &["aaaa bbbb ccc…", "@c1 @c2 @c3", "+p1 +p2 +p3"],
    ),
    (
        "aaaa bbbb cccc dddd @c1 @c2 @c3 @c4 +p1 +p2 +p3 +p4 +p5 +p6",
        &["… @c1 @c2 @c3", "@c4 +p1 +p2", "+p3 +p4 +p5…"],
    ),
    (
        "do @x then aaaa bbbb cccc dddd eeee ffff +y",
        &["do @x then", "aaaa bbbb", "cccc ddd… +y"],
    ),
    (
        "+p1 aaaa bbbb cccc dddd eeee ffff gggg +p2 @c2 @c1",
        &["+p1 aaaa bbbb", "cccc dddd e…", "@c2 @c1 +p2"],
    ),
    (
        "aaaa bbbb cccc dddd eeee ffff gggg k:v +p",
        &["aaaa bbbb cccc", "dddd eeee", "ffff ggg… +p"],
    ),
    (
        "aaaa bbbb cccc dddd eeeeeee ffff ggggggg h",
        &["aaaa bbbb cccc", "dddd eeeeeee", "ffff gggggg…"],
    ),
    (
        "+a aaaa bbbb cccc dddd eeee ffff gggg +a",
        &["+a aaaa bbbb", "cccc dddd", "eeee ffff g…"],
    ),
    (
        "+a aaaa bbbb cccc dddd eeee ffff gggg +a @c",
        &["+a aaaa bbbb", "cccc dddd", "eeee fff… @c"],
    ),
    (
        "aaaa bbbb cccc dddd eeee ffff gggg +a hhhh +a",
        &["aaaa bbbb cccc", "dddd eeee", "ffff ggg… +a"],
    ),
    (
        "+abcd aaaa bbbb cccc dddd eeee +abcd more",
        &["+abcd aaaa", "bbbb cccc", "dddd eeee…"],
    ),
    (
        "aaaa bbbb cccc dddd eeeeeee +abcdefghi hhhh",
        &["aaaa bbbb cccc", "dddd eeeeeee", "+abcdefghi …"],
    ),
    (
        "+a aaaa bbbbbb cccc ddddddd eeee ffffff +a",
        &["+a aaaa bbbbbb", "cccc ddddddd", "eeee ffffff…"],
    ),
];

#[test]
fn projects_and_contexts_the_last_row_would_cut_follow_the_ellipsis() {
    for (text, want) in TAGS {
        assert_eq!(laid(text, (14, 12), MAX_ROWS), want, "{text}");
    }
    // rows one cell wide: the fourth word still leaves an ellipsis
    assert_eq!(laid("a b c d", (1, 1), MAX_ROWS), ["a", "b", "…"]);
    for (text, widths, want) in ONE_ROW {
        assert_eq!(laid(text, widths, 1), [want], "{text} {widths:?}");
    }
}

/// (text, widths, the row): a row cut at its edge shows the projects and
/// contexts it cuts off by the rules of the last of several rows; its
/// width is the first, not the later.
type OneRow = (&'static str, (usize, usize), &'static str);

const ONE_ROW: [OneRow; 10] = [
    ("界", (1, 1), "…"),
    ("fix it +a @b", (20, 20), "fix it +a @b"),
    (
        "fix the parser quickly @agent +lsty",
        (26, 26),
        "fix the pars… @agent +lsty",
    ),
    (
        "fix the parser quickly @agent +lsty",
        (14, 14),
        "… @agent +lsty",
    ),
    ("aaaa bbbb @c +p", (8, 8), "a… @c +p"),
    ("aaaa bbbb @c +p", (5, 5), "… @c…"),
    (
        "do @x then aaaa bbbb cccc dddd +y",
        (20, 20),
        "do @x then aaaa… +y",
    ),
    ("aaaa bbbb +a cccc +a", (12, 12), "aaaa bbb… +a"),
    ("a b c d", (1, 1), "…"),
    ("aaaa bbbb cccc", (9, 3), "aaaa bbb…"),
];

/// (text, widths, the last row as text and slot): the ellipsis takes the
/// slot of the text it cuts, also when the cut ends a word, and the tags
/// after it keep theirs.
type Styled = (
    &'static str,
    (usize, usize),
    &'static [(&'static str, Slot)],
);

const STYLED: [Styled; 2] = [
    (
        "aaaa bbbb cccc dddd eeee ffff +p @c",
        (12, 12),
        &[
            ("eeee", Slot::Text),
            ("…", Slot::Text),
            (" ", Slot::Text),
            ("@c", Slot::Context),
            (" ", Slot::Text),
            ("+p", Slot::Project),
        ],
    ),
    (
        "aaaa bbbb cccc dddd eeee ffff k:v gggg +p",
        (14, 12),
        &[
            ("ffff", Slot::Text),
            (" ", Slot::Text),
            ("k:v", Slot::Muted),
            ("…", Slot::Muted),
            (" ", Slot::Text),
            ("+p", Slot::Project),
        ],
    ),
];

#[test]
fn the_tags_that_follow_the_ellipsis_keep_their_style() {
    let inline = Inline::new(&Config::default());
    for (text, widths, want) in STYLED {
        let rows = lay_out(
            &Line::parse(text.as_bytes()),
            (None, Parts::default()),
            inline,
            Shape {
                widths,
                rows: MAX_ROWS,
            },
        );
        let last: Vec<_> =
            rows[2].iter().map(|s| (s.text.as_ref(), s.slot)).collect();
        let want: Vec<_> = want.iter().map(|&(t, s)| (t, Some(s))).collect();
        assert_eq!(last, want, "{text}");
    }
}

#[test]
fn a_muted_task_keeps_its_slot_on_the_last_row() {
    let inline = Inline::new(&Config::default());
    for text in [
        "aaaa bbbb cccc dddd eeee ffff +p @c",
        "+p @c aaaa bbbb cccc dddd eeee ffff gggg",
    ] {
        let line = Line::parse(text.as_bytes());
        let rows = lay_out(
            &line,
            (Some(Slot::Muted), Parts::default()),
            inline,
            Shape {
                widths: (12, 12),
                rows: MAX_ROWS,
            },
        );
        assert!(
            rows[2].iter().all(|s| s.slot == Some(Slot::Muted)),
            "{text}"
        );
    }
}
