use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use lsty_core::config::Config;
use lsty_core::keys::{Mods, Named, Stroke};
use lsty_core::lock::{self, Cancel, Mode, Want};

use crate::model::Model;
use crate::msg::Msg;
use crate::test_util::{alone, deps, press, started, workdir};

const LIST: &str = "A\nB\nC\nD\nE\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn todo_of(dir: &Path) -> String {
    fs::read_to_string(dir.join("todo.txt")).unwrap_or_default()
}

fn enter(model: &mut Model) {
    model.update(Msg::Key(Stroke::named(Named::Cr)));
}

fn kept(model: &Model) -> (usize, usize) {
    (model.hist.undo.len(), model.hist.redo.len())
}

// 36.39: nothing to repeat, and a sort is not a change.
#[test]
fn repeat_with_nothing_to_repeat_does_nothing() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "j.");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.message, None);
    press(&mut model, "s.");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(kept(&model), (0, 0));
}

// 36.35, 36.38: the next item, and a count replaces the stored one.
#[test]
fn repeat_runs_the_last_change_on_the_item_under_the_cursor() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "xj.");
    let day = "x 2026-09-29";
    assert_eq!(todo_of(dir.path()), format!("{day} A\n{day} B\nC\nD\nE\n"));
    press(&mut model, "j3.");
    let want = format!("{day} A\n{day} B\n{day} C\n{day} D\n{day} E\n");
    assert_eq!(todo_of(dir.path()), want);
    assert_eq!(kept(&model).0, 3, "each repeat is an action to undo");
}

// 36.36: the count it had.
#[test]
fn repeat_keeps_the_count_of_the_change() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "2ddj.");
    assert_eq!(todo_of(dir.path()), "\n\nC\n\n\n");
}

// 36.37: the text of a prompt, without opening it.
#[test]
fn repeat_runs_a_prompt_again_with_its_text() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "gd");
    press(&mut model, "2026-10-02");
    enter(&mut model);
    assert_eq!(todo_of(dir.path()), "A due:2026-10-02\nB\nC\nD\nE\n");
    press(&mut model, "j.");
    assert!(model.input.is_none(), "no prompt opens");
    assert_eq!(
        todo_of(dir.path()),
        "A due:2026-10-02\nB due:2026-10-02\nC\nD\nE\n"
    );
}

// 36.40: a repeat is an action of its own.
#[test]
fn undo_and_redo_are_never_repeated() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "xjx");
    press(&mut model, "u.");
    let day = "x 2026-09-29";
    assert_eq!(todo_of(dir.path()), format!("{day} A\n{day} B\nC\nD\nE\n"));
    press(&mut model, "uu");
    assert_eq!(todo_of(dir.path()), LIST);
}

// 36.3, 36.4: empty stacks, and a new action empties the redo stack.
#[test]
fn nothing_to_undo_or_redo() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("nothing to undo"));
    press(&mut model, "xu");
    assert_eq!(kept(&model), (0, 1));
    press(&mut model, "gp");
    assert_eq!(kept(&model), (1, 0));
    model.update(Msg::Key(Stroke::char('r').with_mods(Mods::CTRL)));
    assert_eq!(todo_of(dir.path()), "(A) A\nB\nC\nD\nE\n");
}

// 36.33: a busy lock keeps the record; the refusal of a changed line
// drops it.
#[test]
fn a_busy_lock_keeps_the_record_and_a_changed_line_drops_it() {
    let dir = workdir(LIST);
    let config = Config {
        lock_timeout: Duration::from_millis(30),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "x");
    let want = Want {
        file: dir.path().join("todo.txt"),
        mode: Mode::Exclusive,
    };
    let mut held =
        lock::acquire(&[want], Duration::from_secs(1), &Cancel::new())
            .unwrap_or_else(|e| panic!("{}", e.message()));
    press(&mut model, "u");
    assert_eq!(kept(&model), (1, 0));
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    fs::write(dir.path().join("todo.txt"), "x 2026-09-29 A +t\nB\n")
        .unwrap_or_else(|e| panic!("{e}"));
    press(&mut model, "u");
    assert_eq!(kept(&model), (0, 0));
    assert_eq!(
        model.message.as_deref(),
        Some("can't undo: \"x 2026-09-29 A\" was changed since")
    );
}

// 36.19: lines are followed across a reload that shifts them.
#[test]
fn the_lines_of_a_record_are_followed_across_a_reload() {
    let dir = workdir("A\nB\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "jx");
    fs::write(dir.path().join("todo.txt"), "Z\nA\nx 2026-09-29 B\nC\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let cmds = model.update(Msg::Reload);
    for cmd in cmds {
        if let crate::msg::Cmd::Load(load) = cmd {
            model.update(load.read());
        }
    }
    press(&mut model, "u");
    assert_eq!(todo_of(dir.path()), "Z\nA\nB\nC\n");
}

// An undo on a big file is a check and one write.
#[test]
#[ignore = "timing, run by hand"]
fn undo_of_a_big_file_is_quick() {
    let lines = "item\n".repeat(10_000);
    let dir = workdir(&lines);
    let mut model = model_of(dir.path());
    press(&mut model, "x");
    let start = Instant::now();
    press(&mut model, "u");
    eprintln!("undo of 10000 lines: {:?}", start.elapsed());
    assert_eq!(todo_of(dir.path()), lines);
}
