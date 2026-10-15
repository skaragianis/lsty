use std::{fs, path::Path};

use lsty_core::{config::Config, keys::Mode};

use crate::{
    Deps,
    model::Model,
    msg::Cmd,
    test_util::{
        Board, alone, deps, held_lock, keys, press, reload, started, todo_of,
        workdir,
    },
};

fn model_with(dir: &Path, board: &Board) -> Model {
    model_of(dir, board, alone())
}

/// A model that shows the archive too.
fn model_with_archive(dir: &Path, board: &Board) -> Model {
    let config = Config {
        hidden_items: Vec::new(),
        ..alone()
    };
    model_of(dir, board, config)
}

fn model_of(dir: &Path, board: &Board, config: Config) -> Model {
    let deps = Deps {
        clipboard: Box::new(board.clone()),
        ..deps(dir, config)
    };
    started(deps)
}

const SAMPLE: &str = "(A) Call client +work\nBuy milk +home\nPlan trip\n";

fn copy(text: &str) -> Vec<Cmd> {
    vec![Cmd::Copy(text.as_bytes().to_vec())]
}

// 43.5
#[test]
fn a_yank_asks_the_terminal_to_copy_the_lines() {
    let rows: [(&str, &str, &str); 3] = [
        ("yy", "(A) Call client +work", "yanked 1 item"),
        (
            "2yy",
            "(A) Call client +work\nBuy milk +home",
            "yanked 2 items",
        ),
        ("9yy", SAMPLE.trim_end(), "yanked 3 items"),
    ];
    for (keys, text, message) in rows {
        let dir = workdir(SAMPLE);
        let mut model = model_with(dir.path(), &Board::default());
        assert_eq!(press(&mut model, keys), copy(text), "{keys}");
        assert_eq!(model.message.as_deref(), Some(message), "{keys}");
    }
}

// The change that `.` repeats copies too.
#[test]
fn repeating_a_delete_copies_the_next_task() {
    let dir = workdir(SAMPLE);
    let mut model = model_with(dir.path(), &Board::default());
    press(&mut model, "dd");
    assert_eq!(press(&mut model, "."), copy("Buy milk +home"));
}

// 43.1, 43.3
#[test]
fn a_yank_writes_nothing_and_is_not_an_undo_step() {
    let dir = workdir(SAMPLE);
    let mut model = model_with(dir.path(), &Board::default());
    press(&mut model, "vjy");
    assert_eq!(model.mode(), Mode::Normal);
    assert_eq!(todo_of(dir.path()), SAMPLE);
    assert!(model.hist.undo.is_empty());
    assert_eq!(model.message.as_deref(), Some("yanked 2 items"));
}

#[test]
fn an_archive_row_can_be_yanked() {
    let dir = workdir("A\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = model_with_archive(dir.path(), &Board::default());
    assert_eq!(press(&mut model, "jyy"), copy("x 2026-09-01 Old"));
}

// 34.x with 43: a delete yanks what it deletes
#[test]
fn a_delete_yanks_the_lines_before_they_are_blanked() {
    for (keys, text) in [
        ("dd", "(A) Call client +work"),
        ("vjd", "(A) Call client +work\nBuy milk +home"),
    ] {
        let dir = workdir(SAMPLE);
        let mut model = model_with(dir.path(), &Board::default());
        assert_eq!(press(&mut model, keys), copy(text), "{keys}");
    }
}

#[test]
fn a_refused_delete_yanks_nothing_and_keeps_the_last_yank() {
    let dir = workdir("A\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let board = Board::default();
    let mut model = model_with_archive(dir.path(), &board);
    board.take(&press(&mut model, "yy"));
    press(&mut model, "j");
    assert_eq!(press(&mut model, "dd"), Vec::new());
    keys(&mut model, "kp");
    assert_eq!(todo_of(dir.path()), "A\n2026-09-29 A\n");
}

// 43.21, 43.24
#[test]
fn nothing_to_paste_without_text_or_yank() {
    let dir = workdir("A\n");
    let board = Board::default();
    let mut model = model_with(dir.path(), &board);
    press(&mut model, "p");
    assert_eq!(model.message.as_deref(), Some("nothing to paste"));
    board.set(" \n\t ");
    press(&mut model, "p");
    assert_eq!(model.message.as_deref(), Some("nothing to paste"));
    assert_eq!(todo_of(dir.path()), "A\n");
}

// 43.23
#[test]
fn an_unreadable_clipboard_pastes_the_last_yank() {
    let dir = workdir("A\nB\n");
    let mut model = model_with(dir.path(), &Board::default());
    keys(&mut model, "2yyjp");
    let said = model.message.clone().unwrap_or_default();
    assert!(
        said.ends_with(
            "; clipboard not readable \u{2014} pasted the last yank"
        ),
        "{said}"
    );
    assert_eq!(todo_of(dir.path()), "A\nB\n2026-09-29 A\n2026-09-29 B\n");
}

// 43.37: the dependent is followed to its new line
#[test]
fn a_kept_dependent_follows_its_line_through_an_outside_change() {
    let dir = workdir("A id:a\nB dep:a\nC\n");
    let board = Board::default();
    let mut model = model_with(dir.path(), &board);
    board.take(&press(&mut model, "dd"));
    fs::write(dir.path().join("todo.txt"), "X\nY\n\nB\nC\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    keys(&mut model, "Gp");
    assert_eq!(todo_of(dir.path()), "X\nY\n\nB dep:a\nC\nA id:a\n");
    assert_eq!(
        model.message.as_deref(),
        Some("pasted 1 item (1 moved); relinked 4")
    );
}

// 43.41: what the cascade kept is used once
#[test]
fn the_kept_links_are_used_by_one_paste() {
    let dir = workdir("A id:a\nB dep:a\nC\n");
    let board = Board::default();
    let mut model = model_with(dir.path(), &board);
    board.take(&press(&mut model, "dd"));
    keys(&mut model, "jjpp");
    assert_eq!(todo_of(dir.path()), "\nB dep:a\nC\nA id:a\n2026-09-29 A\n");
    assert_eq!(model.message.as_deref(), Some("pasted 1 item"));
}

#[test]
fn a_busy_lock_pastes_nothing() {
    let dir = workdir("A\n");
    let board = Board::default();
    board.set("Buy milk");
    let mut model = model_with(dir.path(), &board);
    let want = crate::messages::busy("todo.txt");
    let mut held = held_lock(dir.path());
    model.deps.config.lock_timeout = std::time::Duration::from_millis(50);
    press(&mut model, "p");
    drop(held.release());
    assert_eq!(model.message.as_deref(), Some(want.as_str()));
    assert_eq!(todo_of(dir.path()), "A\n");
}

#[test]
fn a_yank_and_a_delete_write_the_clipboard_and_still_ask_the_terminal() {
    for (keys, text) in [
        ("yy", "(A) Call client +work"),
        ("dd", "(A) Call client +work"),
    ] {
        let dir = workdir(SAMPLE);
        let board = Board::default();
        let mut model = model_with(dir.path(), &board);
        assert_eq!(press(&mut model, keys), copy(text), "{keys}");
        assert_eq!(board.written(), vec![text.as_bytes().to_vec()], "{keys}");
    }
}

#[test]
fn a_failed_write_of_the_clipboard_is_noted_and_the_yank_stands() {
    let dir = workdir(SAMPLE);
    let board = Board::default();
    board.fail_writes("write the clipboard with mine: run mine: boom");
    let mut model = model_with(dir.path(), &board);
    assert_eq!(press(&mut model, "yy"), copy("(A) Call client +work"));
    assert_eq!(
        model.message.as_deref(),
        Some("yanked 1 item; write the clipboard with mine: run mine: boom")
    );
}
