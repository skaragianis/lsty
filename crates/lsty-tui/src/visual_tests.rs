use std::{fs, path::Path};

use lsty_core::{
    config::{Config, Hidden},
    keys::Mode,
    ops::LIST_CHANGED,
};

use crate::{
    messages,
    model::Model,
    test_util::{
        alone, deps, keys, press, reload, screen, settled, started, todo_of,
        workdir,
    },
};

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

const LIST: &str = "A\nB\nC\nD\nE\n";

/// The keys, the rows selected, as `(first, last)`.
type Case = (&'static str, &'static str, Option<(usize, usize)>);

const SELECTIONS: [Case; 7] = [
    // 37.1
    ("jv", "start", Some((1, 1))),
    // 37.2
    ("jvjj", "extend down", Some((1, 3))),
    // 37.3
    ("jvkk", "extend up, the anchor stays", Some((0, 1))),
    // 37.4
    ("jVG", "to the last row", Some((1, 4))),
    // 37.5
    ("jv2j", "count motion", Some((1, 3))),
    // 37.6
    ("vv", "v leaves", None),
    ("vV", "V leaves", None),
];

#[test]
fn a_selection_runs_from_the_anchor_to_the_cursor() {
    for (keys, name, want) in SELECTIONS {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        press(&mut model, keys);
        assert_eq!(model.selected_rows(), want, "{name}");
        let mode = if want.is_some() {
            Mode::Visual
        } else {
            Mode::Normal
        };
        assert_eq!(model.mode(), mode, "{name}");
        assert_eq!(todo_of(dir.path()), LIST, "{name}");
    }
}

#[test]
fn escape_leaves_and_the_cursor_stays() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "vjj<Esc>");
    assert_eq!((model.mode(), model.cursor), (Mode::Normal, 2));
}

#[test]
fn v_does_nothing_on_an_empty_list() {
    let dir = workdir("");
    let mut model = model_of(dir.path());
    press(&mut model, "v");
    assert_eq!(model.mode(), Mode::Normal);
}

#[test]
fn only_the_visual_keys_act() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "v:/sjuo");
    assert_eq!(model.mode(), Mode::Visual);
    assert!(model.line.is_none());
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn the_status_line_counts_the_selection() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "jvj");
    let lines = screen(&model, 80, 24);
    let status = lines.last().map_or("", String::as_str);
    assert!(
        status.starts_with(" VISUAL  2 selected  todo.txt"),
        "{status}"
    );
}

#[test]
fn a_selection_is_one_write_and_ends_on_its_first_task() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "jvjjd");
    assert_eq!(todo_of(dir.path()), "A\n\n\n\nE\n");
    assert_eq!(model.mode(), Mode::Normal);
    // The cursor is on E, the next shown row.
    assert_eq!(model.cursor, 1);
    press(&mut model, "u");
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn a_refused_action_keeps_the_selection() {
    let dir = workdir("A\nx 2026-01-01 B\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "vjt");
    assert_eq!(model.mode(), Mode::Visual);
    assert_eq!(model.selected_rows(), Some((0, 1)));
    assert_eq!(model.message.as_deref(), Some(messages::SOME_DONE));
}

#[test]
fn a_delete_or_a_priority_takes_tasks_of_any_state() {
    const DOING: &str = "B started:2026-09-28T0700Z owner:me";
    for (keys, want) in [
        ("vjd", "\n\nC\n".to_owned()),
        ("vj-", format!("(A) A\n(A) {DOING}\nC\n")),
    ] {
        let dir = workdir(format!("A\n{DOING}\nC\n"));
        let mut model = model_of(dir.path());
        press(&mut model, keys);
        assert_eq!(todo_of(dir.path()), want, "{keys}");
    }
}

#[test]
fn a_counted_action_never_shows_visual() {
    let dir = workdir("A\nx 2026-01-01 B\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "2t");
    assert_eq!(model.mode(), Mode::Normal);
    assert_eq!(model.message.as_deref(), Some(messages::SOME_DONE));
}

#[test]
fn a_block_moves_and_the_selection_stays() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vjJ");
    assert_eq!(todo_of(dir.path()), "C\nA\nB\nD\nE\n");
    assert_eq!(model.mode(), Mode::Visual);
    assert_eq!(model.selected_rows(), Some((1, 2)));
    press(&mut model, "JKK");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.selected_rows(), Some((0, 1)));
}

#[test]
fn a_block_at_the_end_stays_put() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "GkvjJ");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.selected_rows(), Some((3, 4)));
}

#[test]
fn dot_after_a_block_move_moves_that_many_rows() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "vjJ<Esc>gg.");
    assert_eq!(todo_of(dir.path()), "B\nC\nA\nD\nE\n");
}

#[test]
fn dot_after_a_selection_repeats_over_as_many_tasks() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "vjxj.");
    let want =
        "x 2026-09-29 A\nx 2026-09-29 B\nC\nx 2026-09-29 D\nx 2026-09-29 E\n";
    assert_eq!(todo_of(dir.path()), want);
}

#[test]
fn a_reload_that_empties_the_list_ends_the_selection() {
    for (then, written) in [("", ""), ("P\nQ\n", "x 2026-09-29 P\nQ\n")] {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        press(&mut model, "jv");
        fs::write(dir.path().join("todo.txt"), "")
            .unwrap_or_else(|e| panic!("{e}"));
        reload(&mut model);
        assert_eq!(model.mode(), Mode::Normal);
        assert_eq!(model.visual, None);
        // The next key answers for no line that was selected.
        fs::write(dir.path().join("todo.txt"), then)
            .unwrap_or_else(|e| panic!("{e}"));
        reload(&mut model);
        keys(&mut model, "x");
        assert_eq!(todo_of(dir.path()), written);
        assert_ne!(model.message.as_deref(), LOOK);
    }
}

#[test]
fn a_reload_that_swaps_selected_lines_keeps_the_rows_lit() {
    let cases = [
        ("jvj", "A\nE\nC\nD\nB\n", (1, 2)),
        ("Gvkk", "A\nB\nC\nD\n", (2, 3)),
    ];
    for (typed, now, lit) in cases {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        press(&mut model, typed);
        fs::write(dir.path().join("todo.txt"), now)
            .unwrap_or_else(|e| panic!("{e}"));
        reload(&mut model);
        assert_eq!(model.selected_rows(), Some(lit), "{typed}");
    }
}

#[test]
fn a_prompt_cancelled_keeps_the_selection_and_a_written_one_ends_it() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vjgt");
    assert_eq!(model.mode(), Mode::Command);
    keys(&mut model, "<Esc>");
    assert_eq!(
        (model.mode(), model.selected_rows()),
        (Mode::Visual, Some((0, 1)))
    );
    keys(&mut model, "gt+x<CR>");
    assert_eq!(todo_of(dir.path()), "A +x\nB +x\nC\nD\nE\n");
    assert_eq!(model.mode(), Mode::Normal);
}

#[test]
fn a_prompt_that_changes_nothing_keeps_the_lines_selected_through_an_insert() {
    let dir = workdir("A +x\nB +x\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "vjgt+x");
    fs::write(dir.path().join("todo.txt"), "N\nA +x\nB +x\nC\n")
        .unwrap_or_else(|e| panic!("{e}"));
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), "N\nA +x\nB +x\nC\n");
    assert_eq!(model.selected_rows(), Some((1, 2)));
    assert_eq!(model.mode(), Mode::Visual);
}

#[test]
fn a_count_before_an_action_in_visual_mode_is_dropped() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vj3x");
    let todo = todo_of(dir.path());
    let done = todo.lines().filter(|l| l.starts_with("x ")).count();
    assert_eq!(done, 2, "{todo}");
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vj3J");
    assert_eq!(todo_of(dir.path()), "C\nA\nB\nD\nE\n");
}

#[test]
fn dot_after_a_prompt_over_a_selection_repeats_over_as_many_tasks() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "vjgt+x<CR>jj.");
    assert_eq!(todo_of(dir.path()), "A +x\nB +x\nC +x\nD +x\nE\n");
}

#[test]
fn a_pane_key_digit_is_no_count_in_visual_mode() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "V<C-w>2j");
    assert_eq!(model.selected_rows(), Some((0, 1)));
}

// A reading leaves what is typed on an open line, even one that empties the
// list and so ends the selection, or in a field.
#[test]
fn a_reload_keeps_the_keys_typed_on_an_open_line_or_in_a_field() {
    for (mode, typed, read) in [
        ("command", "jvgdj", ""),
        ("insert", "jccj", "A\nB\nC\nD\nE\nF\n"),
    ] {
        let dir = workdir(LIST);
        fs::write(
            dir.path().join("keys.toml"),
            format!("[{mode}]\ncancel = [\"jk\"]\n"),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let mut model = model_of(dir.path());
        press(&mut model, typed);
        fs::write(dir.path().join("todo.txt"), read)
            .unwrap_or_else(|e| panic!("{e}"));
        reload(&mut model);
        press(&mut model, "k");
        let closed = model.line.is_none() && model.edit.is_none();
        assert!(closed, "{mode}: j then k closes it");
    }
}

const ABC: &str = "A\nB\nC\n";
const ABCD: &str = "A\nB\nC\nD\n";
const LOOK: Option<&str> = Some(LIST_CHANGED);
const SAME: &str = "A\nB +k\nC\n";
const LINKED: &str = "A https://a.test +k\nB\n";
const CLOSED: &str = "x 2026-09-01 A\nx 2026-09-01 B\n";
const CLOSED_K: &str = "x 2026-09-01 A +k\nx 2026-09-01 B\n";

/// The file, the done file after a `|`, keys to where they are typed, those
/// typed before the reading, what it reads, each reading after a `|`, those
/// typed after, the file left, and the word said.
type Under = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);

const UNDER_WAY: &[Under] = &[
    (ABC, "j", "d", "A\nC\n", "d", "A\nC\n", LOOK),
    (ABC, "j", "d", "A\nC\n", "ddd", "A\n", None),
    (ABC, "j", "d", "A\nB2\nC\n", "d", "A\nB2\nC\n", LOOK),
    (ABC, "j", "d", "A\nB +k\nC\n", "d", "A\nB +k\nC\n", LOOK),
    (ABC, "j", "d", "C\nB\nA\n", "d", "C\nB\nA\n", LOOK),
    (ABC, "j", "d", "A\nB\nB\nC\n", "d", "A\nB\nB\nC\n", LOOK),
    (ABC, "j", "g", "A\nB +k\nC\n", "d", "A\nB +k\nC\n", LOOK),
    (ABC, "j", "c", "A\nB +k\nC\n", "c", "A\nB +k\nC\n", LOOK),
    (ABC, "j", "g", "A\nB +k\nC\n", "g", "A\nB +k\nC\n", None),
    (ABC, "j", "2", "A\nB +k\nC\n", "j", "A\nB +k\nC\n", None),
    (ABC, "j", "2", "A\nB +k\nC\n", "J", "A\nB +k\nC\n", LOOK),
    (CLOSED, "gg", "2", CLOSED_K, "-", CLOSED_K, None),
    (CLOSED, "gg", "vj", CLOSED_K, "-", CLOSED_K, None),
    (
        "A due:2030-01-01\nB\nC\n",
        "Ggd2030-01-01<CR>|gg",
        "2",
        "A due:2030-01-01 +k\nB\nC due:2030-01-01\n",
        ".",
        "A due:2030-01-01 +k\nB\nC due:2030-01-01\n",
        LOOK,
    ),
    (ABC, "j", "g", "A\nB +k\nC\n", "o", "A\nB +k\nC\n", LOOK),
    (
        ABC,
        "+j",
        "2",
        "(A) A\nB +k\nC\n",
        ".",
        "(A) A\nB +k\nC\n",
        LOOK,
    ),
    (ABC, "j", "2", "A\nB\n", "x", "A\nB\n", LOOK),
    (
        ABC,
        "jyyp",
        "2",
        "A\nB\nB\nC +k\n",
        ".",
        "A\nB\nB\n2026-09-29 B id:t002\nC +k\n",
        None,
    ),
    (
        "(A) B\n|x 2026-09-01 Done\n",
        "s",
        "2",
        "(B) N\n(A) B\n",
        "x",
        "(B) N\n(A) B\n",
        LOOK,
    ),
    (
        ABC,
        "j",
        "d",
        "A\nB +k\nC\n|N\nA\nB +k\nC\n",
        "d",
        "N\nA\nB +k\nC\n",
        LOOK,
    ),
    (
        ABC,
        "j",
        "?g",
        SAME,
        "gqx",
        "A\nx 2026-09-29 B +k\nC\n",
        None,
    ),
    (ABC, "j", "2", SAME, "o", SAME, None),
    (ABC, "j", "2", SAME, "O", SAME, None),
    (ABC, "j", "2", SAME, "yy", SAME, None),
    (
        ABC,
        "yyj",
        "2",
        SAME,
        "p",
        "A\nB +k\n2026-09-29 A id:t001\nC\n",
        None,
    ),
    (
        ABC,
        "yyj",
        "2",
        SAME,
        "P",
        "A\n2026-09-29 A id:t001\nB +k\nC\n",
        None,
    ),
    (
        "A https://a.test\nB\n",
        "gg",
        "g",
        LINKED,
        "x",
        LINKED,
        None,
    ),
    (ABC, "j", "d", "A\nB +k\nC\n", "<Esc>dd", "A\nC\n", None),
    (ABC, "j", "d", "Z\nA\nB\nC\n", "d", "Z\nA\nC\n", None),
    (ABC, "j", "d", "B\nC\n", "d", "C\n", None),
    (ABC, "j", "d", "A\nB\nC +k\n", "d", "A\nC +k\n", None),
    (ABCD, "j", "2", "A\nC\nD\n", "dd", "A\nC\nD\n", LOOK),
    (
        ABCD,
        "j",
        "2",
        "A\nB\nN\nC\nD\n",
        "dd",
        "A\nB\nN\nC\nD\n",
        LOOK,
    ),
    (ABCD, "j", "2", "Z\nA\nB\nC\nD\n", "dd", "Z\nA\nD\n", None),
    (
        ABC,
        "j",
        "gd",
        "A2\nB2\nC\n",
        "fri<CR>",
        "A2\nB2\nC\n",
        LOOK,
    ),
    (ABC, "j", "gd", SAME, "fri<CR>", SAME, LOOK),
    (ABC, "j", "gd", "C\nB\nA\n", "fri<CR>", "C\nB\nA\n", LOOK),
    (ABC, "gg", "2gd", "B\nC\n", "fri<CR>", "B\nC\n", LOOK),
    ("A\nA\n", "gg", "gd", "A\n", "fri<CR>", "A\n", LOOK),
    (
        ABC,
        "gg",
        "vjgd",
        "A\nB2\nC\n",
        "fri<CR>",
        "A\nB2\nC\n",
        LOOK,
    ),
    (ABC, "gg", "vj", "A\nB2\nC\n", "x", "A\nB2\nC\n", LOOK),
    (ABC, "gg", "vj", "A\nC\n", "x", "A\nC\n", LOOK),
    (ABC, "gg", "vj", "A\nN\nB\nC\n", "x", "A\nN\nB\nC\n", LOOK),
    (ABC, "gg", "vj", "A2\nB\nC\n", "jd", "A2\nB\nC\n", LOOK),
    (ABC, "gg", "vj", "A\nB2\nC\n", "gd", "A\nB2\nC\n", LOOK),
    (ABC, "gg", "vjgd", SAME, "<Esc>x", SAME, LOOK),
    (
        "A\nB\nC\nD\nE\n",
        "j",
        "vj",
        "A\nE\nC\nD\nB\n",
        "d",
        "A\nE\nC\nD\nB\n",
        LOOK,
    ),
    (
        ABC,
        "gg",
        "vj",
        "A\nB2\nC\n",
        "<Esc>x",
        "A\nx 2026-09-29 B2\nC\n",
        None,
    ),
    (ABC, "gg", "vj", "Z\nA\nB\nC\n", "d", "Z\nC\n", None),
    (ABC, "gg", "vj", "A\nB\nC2\n", "d", "C2\n", None),
    (
        "A +k\nB +k\nC +k\n",
        "<C-w>3x|<C-w>1j",
        "d",
        "A +k\nB\nC +k\n",
        "d",
        "A +k\nB\nC +k\n",
        LOOK,
    ),
];

// Keys typed on a task refuse at the last of them when a reading took the
// line from under them, or cannot tell it did not, and read the files again.
// A line that only shifted goes on, and so does a key that writes nothing.
#[test]
fn a_reading_taking_the_line_from_under_the_keys_typed_refuses_the_last() {
    for &(todo, to, typed, outside, then, want, says) in UNDER_WAY {
        let (todo, done) = todo.split_once('|').unwrap_or((todo, ""));
        let dir = workdir(todo);
        fs::write(dir.path().join("done.txt"), done)
            .unwrap_or_else(|e| panic!("{e}"));
        let config = Config {
            hidden_tasks: vec![Hidden::Later],
            ..alone()
        };
        let mut model = started(deps(dir.path(), config));
        to.split('|').for_each(|part| settled(&mut model, part));
        keys(&mut model, typed);
        for reading in outside.split('|') {
            fs::write(dir.path().join("todo.txt"), reading)
                .unwrap_or_else(|e| panic!("{e}"));
            reload(&mut model);
        }
        keys(&mut model, then);
        let file = todo_of(dir.path());
        let lines: Vec<&str> =
            file.lines().filter(|l| !l.is_empty()).collect();
        let name = format!("{typed} {outside} {then}");
        assert_eq!(lines.join("\n") + "\n", want, "{name}");
        let said = model.message.as_deref();
        if says.is_some() {
            assert_eq!(said, says, "{name}");
            let open = model.line.is_some() || model.visual.is_some();
            assert!(!open, "{name}: it ends");
        } else {
            assert_ne!(said, LOOK, "{name}");
        }
    }
}
