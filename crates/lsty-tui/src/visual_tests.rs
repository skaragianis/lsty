use std::fs;
use std::path::Path;

use lsty_core::keys::{Mode, Named, Stroke};

use crate::model::Model;
use crate::msg::Msg;
use crate::test_util::{alone, deps, press, reload, started, workdir};

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn todo_of(dir: &Path) -> String {
    fs::read_to_string(dir.join("todo.txt")).unwrap_or_default()
}

fn escape(model: &mut Model) {
    model.update(Msg::Key(Stroke::named(Named::Esc)));
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
    press(&mut model, "vjj");
    escape(&mut model);
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
    let lines = crate::test_util::screen(&model, 80, 24);
    let status = lines.last().map_or("", String::as_str);
    assert!(
        status.starts_with(" VISUAL  2 selected  todo.txt"),
        "{status}"
    );
}

#[test]
fn a_selection_is_one_write_and_ends_on_its_first_item() {
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
    press(&mut model, "vjx");
    assert_eq!(model.mode(), Mode::Visual);
    assert_eq!(model.selected_rows(), Some((0, 1)));
    assert_eq!(
        model.message.as_deref(),
        Some("mixed states (1 todo, 1 done) — nothing changed")
    );
}

#[test]
fn a_counted_action_never_shows_visual() {
    let dir = workdir("A\nx 2026-01-01 B\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "2x");
    assert_eq!(model.mode(), Mode::Normal);
    assert_eq!(
        model.message.as_deref(),
        Some("mixed states (1 todo, 1 done) — nothing changed")
    );
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
    press(&mut model, "vjJ");
    escape(&mut model);
    press(&mut model, "gg.");
    assert_eq!(todo_of(dir.path()), "B\nC\nA\nD\nE\n");
}

#[test]
fn dot_after_a_selection_repeats_over_as_many_items() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vjx");
    press(&mut model, "jjj.");
    let want =
        "x 2026-09-29 A\nx 2026-09-29 B\nC\nx 2026-09-29 D\nx 2026-09-29 E\n";
    assert_eq!(todo_of(dir.path()), want);
}

#[test]
fn a_reload_that_removes_the_only_row_ends_the_selection() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "jv");
    fs::write(dir.path().join("todo.txt"), "A\nC\nD\nE\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(model.mode(), Mode::Normal);
    assert_eq!(model.visual, None);
}

#[test]
fn a_reload_keeps_the_rows_that_are_left_selected() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "jvjj");
    fs::write(dir.path().join("todo.txt"), "A\nB\nD\nE\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(model.selected_rows(), Some((1, 2)));
}

#[test]
fn a_reload_without_the_anchor_moves_it_to_the_next_selected_row() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "jvj");
    fs::write(dir.path().join("todo.txt"), "A\nC\nD\nE\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(model.selected_rows(), Some((1, 1)));
    assert_eq!(
        model.visual,
        Some(crate::test_util::at(lsty_core::errs::Source::Todo, 2))
    );
}

#[test]
fn a_prompt_cancelled_keeps_the_selection_and_a_written_one_ends_it() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vjgt");
    assert_eq!(model.mode(), Mode::Command);
    escape(&mut model);
    assert_eq!(
        (model.mode(), model.selected_rows()),
        (Mode::Visual, Some((0, 1)))
    );
    press(&mut model, "gt+x");
    model.update(Msg::Key(Stroke::named(Named::Cr)));
    assert_eq!(todo_of(dir.path()), "A +x\nB +x\nC\nD\nE\n");
    assert_eq!(model.mode(), Mode::Normal);
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
fn dot_after_a_prompt_over_a_selection_repeats_over_as_many_items() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "vjgt+x");
    model.update(Msg::Key(Stroke::named(Named::Cr)));
    press(&mut model, "jj.");
    assert_eq!(todo_of(dir.path()), "A +x\nB +x\nC +x\nD +x\nE\n");
}
