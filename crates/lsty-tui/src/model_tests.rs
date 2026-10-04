use std::cell::Cell;
use std::fs;
use std::rc::Rc;
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::keys::Stroke;

use crate::test_util::{
    NOW, SAMPLE, alone, deps, instant, press, screen, started, workdir,
};
use crate::{Cmd, Model, Msg};

fn quits(cmds: &[Cmd]) -> bool {
    cmds.contains(&Cmd::Quit)
}

#[test]
fn quit() {
    const ROWS: [(&str, bool); 4] =
        [("q", true), ("ZZ", true), ("Z", false), ("!", false)];
    for (keys, want) in ROWS {
        let dir = workdir(SAMPLE);
        let mut model = started(deps(dir.path(), Config::default()));
        let cmds = press(&mut model, keys);
        assert_eq!(quits(&cmds), want, "keys {keys}");
        let data = fs::read_to_string(dir.path().join("todo.txt"));
        assert_eq!(data.as_deref().ok(), Some(SAMPLE), "keys {keys}");
    }
}

#[test]
fn a_rebound_quit_key_replaces_the_default() {
    let dir = workdir(SAMPLE);
    fs::write(dir.path().join("keys.toml"), "[normal]\nquit = [\"Q\"]\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = started(deps(dir.path(), Config::default()));
    assert!(!quits(&press(&mut model, "q")));
    assert!(quits(&press(&mut model, "Q")));
}

#[test]
fn actions_of_later_parts_change_nothing() {
    let dir = workdir(SAMPLE);
    let mut model = started(deps(dir.path(), Config::default()));
    let before = screen(&model, 80, 24);
    for keys in ["x", "dd", "o", "t", "u"] {
        assert!(press(&mut model, keys).is_empty(), "keys {keys}");
    }
    assert_eq!(screen(&model, 80, 24), before);
    let data = fs::read_to_string(dir.path().join("todo.txt"));
    assert_eq!(data.as_deref().ok(), Some(SAMPLE));
}

#[test]
fn no_file_is_created_for_a_missing_file_or_folder() {
    for folder_missing in [false, true] {
        let dir = workdir("");
        let mut d = deps(dir.path(), Config::default());
        if folder_missing {
            d.found.todo = dir.path().join("no").join("todo.txt");
            d.found.done = dir.path().join("no").join("done.txt");
            d.found.state = lsty_core::discover::FileState::DirMissing;
        }
        let model = started(d);
        let status = screen(&model, 80, 24).pop().unwrap_or_default();
        assert!(status.contains("[New] todo.txt"), "{status}");
        let entries = fs::read_dir(dir.path()).map(Iterator::count);
        assert_eq!(entries.ok(), Some(0), "folder missing: {folder_missing}");
    }
}

fn long_list(items: usize) -> String {
    let lines: Vec<String> =
        (1..=items).map(|n| format!("Item {n} +work\n")).collect();
    lines.concat()
}

#[test]
fn the_cursor_stops_at_the_ends_and_counts_move_it() {
    let dir = workdir(&long_list(10));
    let mut model = started(deps(dir.path(), Config::default()));
    press(&mut model, "k");
    assert_eq!(model.cursor, 0);
    press(&mut model, "3j");
    assert_eq!(model.cursor, 3);
    press(&mut model, "G");
    assert_eq!(model.cursor, 9);
    press(&mut model, "j");
    assert_eq!(model.cursor, 9);
    press(&mut model, "gg");
    assert_eq!(model.cursor, 0);
}

#[test]
fn a_count_on_go_top_is_an_nr_of_the_todo_file() {
    let todo = "A\n\nC\nD\n";
    let dir = workdir(todo);
    let mut model = started(deps(dir.path(), Config::default()));
    press(&mut model, "2G");
    assert_eq!(model.cursor, 1, "NR 2 is blank: the next one is NR 3");
    press(&mut model, "1gg");
    assert_eq!(model.cursor, 0);
    press(&mut model, "9G");
    assert_eq!(model.cursor, 2, "no NR from 9 up: the last row");
}

#[test]
fn only_todo_items_have_an_nr_to_go_to() {
    let dir = workdir("Low item\n(A) Top item\n");
    fs::write(
        dir.path().join("done.txt"),
        "x 2026-09-01 One\nx 2026-09-02 Two\nx 2026-09-03 Three\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_items: Vec::new(),
        ..Config::default()
    };
    let mut model = started(deps(dir.path(), config));
    assert_eq!(model.rows.len(), 5);
    press(&mut model, "4G");
    assert_eq!(model.cursor, 4, "no todo item has NR 4 or more");
    press(&mut model, "2G");
    assert_eq!(model.cursor, 1);
}

#[test]
fn half_pages_move_by_half_the_pane() {
    let dir = workdir(&long_list(100));
    let mut model = started(deps(dir.path(), alone()));
    model.update(Msg::Resize(80, 12));
    // 12 rows: 1 status, 2 border, 9 items; half is 4.
    model.update(Msg::Key(
        Stroke::char('d').with_mods(lsty_core::keys::Mods::CTRL),
    ));
    assert_eq!(model.cursor, 4);
    assert_eq!(model.top, 0);
    for _ in 0..2 {
        model.update(Msg::Key(
            Stroke::char('d').with_mods(lsty_core::keys::Mods::CTRL),
        ));
    }
    assert_eq!(model.cursor, 12);
    assert_eq!(model.top, 4);
}

#[test]
fn the_view_scrolls_by_the_least_that_shows_the_cursor() {
    let dir = workdir(&long_list(30));
    let mut model = started(deps(dir.path(), alone()));
    model.update(Msg::Resize(80, 8));
    // Five item rows.
    press(&mut model, "4j");
    assert_eq!(model.top, 0);
    press(&mut model, "j");
    assert_eq!(model.top, 1);
    press(&mut model, "5k");
    assert_eq!(model.top, 0);
}

#[test]
fn a_terminal_that_is_too_small_shows_one_line() {
    let dir = workdir(SAMPLE);
    let mut model = started(deps(dir.path(), Config::default()));
    model.update(Msg::Resize(19, 5));
    let lines = screen(&model, 19, 5);
    assert_eq!(
        lines.first().map(String::as_str),
        Some("terminal too small")
    );
    assert!(lines.iter().skip(1).all(String::is_empty));
}

#[test]
fn the_model_waits_for_the_next_minute() {
    let dir = workdir(SAMPLE);
    let model = started(deps(dir.path(), Config::default()));
    let cmds = model.init();
    assert_eq!(
        cmds,
        [Cmd::After {
            delay: Duration::from_secs(60),
            msg: Msg::Minute,
        }]
    );
}

#[test]
fn a_new_minute_derives_the_times_again() {
    let todo = "Task started:2026-09-29T0700Z\n";
    let dir = workdir(todo);
    let now = Rc::new(Cell::new(instant(NOW)));
    let mut d = deps(dir.path(), Config::default());
    let clock = Rc::clone(&now);
    d.now = Box::new(move || clock.get());
    let mut model = started(d);
    assert!(screen(&model, 80, 24)[1].contains("2h"));
    now.set(instant("2026-09-29T10:00:30Z"));
    let cmds = model.update(Msg::Minute);
    assert!(screen(&model, 80, 24)[1].contains("3h"));
    assert_eq!(
        cmds,
        [Cmd::After {
            delay: Duration::from_secs(30),
            msg: Msg::Minute,
        }]
    );
}

#[test]
fn warnings_are_shown_one_per_key_press() {
    let dir = workdir(SAMPLE);
    let mut d = deps(dir.path(), Config::default());
    d.warnings = ["one", "two"]
        .map(|m| lsty_core::config::Warning {
            source: "config.toml".to_owned(),
            message: m.to_owned(),
        })
        .to_vec();
    let mut model =
        Model::new(d).unwrap_or_else(|e| panic!("{}", e.message()));
    model.update(Msg::Resize(80, 24));
    let status = |m: &Model| screen(m, 80, 24).pop().unwrap_or_default();
    assert!(status(&model).contains("config.toml: one (1 of 2)"));
    press(&mut model, "j");
    assert!(status(&model).contains("config.toml: two (2 of 2)"));
    press(&mut model, "j");
    assert!(!status(&model).contains("config.toml"));
}

#[test]
fn drawing_ten_thousand_rows_is_fast() {
    let dir = workdir(&long_list(10_000));
    let started_at = std::time::Instant::now();
    let mut model = started(deps(dir.path(), alone()));
    let loaded = started_at.elapsed();
    press(&mut model, "G");
    let drawn_at = std::time::Instant::now();
    let lines = screen(&model, 80, 24);
    let drawn = drawn_at.elapsed();
    eprintln!("load and rows {loaded:?}, one frame {drawn:?}");
    assert!(lines[1].contains("Item 9980"), "{lines:?}");
    assert!(loaded < Duration::from_secs(5));
    assert!(drawn < Duration::from_secs(1));
}

/// The probe of a minute tick that brings a hidden item into the list:
/// the cursor stays on its item.
#[test]
fn a_tick_keeps_the_cursor_on_its_item() {
    let dir = workdir("b\na t:2026-09-30\nc\nd\n");
    let clock = Rc::new(Cell::new(instant("2026-09-29T23:59:00Z")));
    let mut d = deps(dir.path(), Config::default());
    let shared = Rc::clone(&clock);
    d.now = Box::new(move || shared.get());
    let mut model = started(d);
    press(&mut model, "j");
    assert_eq!(model.rows.len(), 3);
    assert_eq!(model.rows[model.cursor].nr, 3);
    clock.set(instant("2026-09-30T00:00:00Z"));
    model.update(Msg::Minute);
    assert_eq!(model.rows.len(), 4, "the later item is listed now");
    assert_eq!(model.rows[model.cursor].nr, 3);
}

#[test]
fn a_tick_keeps_the_row_where_it_was_on_screen() {
    let dir = workdir(&long_list(30));
    let mut model = started(deps(dir.path(), Config::default()));
    model.update(Msg::Resize(80, 12));
    press(&mut model, "20j");
    let (nr, offset) = (model.rows[model.cursor].nr, model.cursor - model.top);
    model.update(Msg::Minute);
    assert_eq!(model.rows[model.cursor].nr, nr);
    assert_eq!(model.cursor - model.top, offset);
}

/// The terminal may be smaller than the last size the model was told.
#[test]
fn drawing_into_a_smaller_buffer_stays_inside_it() {
    let dir = workdir(&long_list(50));
    let mut model = started(deps(dir.path(), Config::default()));
    model.update(Msg::Resize(80, 24));
    for (w, h) in [(80, 20), (60, 24), (30, 8), (10, 3), (0, 0), (80, 1)] {
        let lines = screen(&model, w, h);
        assert_eq!(lines.len(), usize::from(h), "{w}x{h}");
    }
    let lines = screen(&model, 80, 20);
    assert!(lines[19].contains("NORMAL"), "the status line is last");
}

/// Control characters in a line must not move the columns after them,
/// and none reaches the terminal.
#[test]
fn control_characters_keep_the_columns_aligned() {
    let todo = "a\u{1}\u{1}\u{1}\u{1}b +home due:2026-09-30\n\
                plain +work due:2026-09-30\n\
                \u{1b}[31mred\u{7f} +x\n\
                tab\there\n";
    let dir = workdir(todo);
    let model = started(deps(dir.path(), Config::default()));
    let lines = screen(&model, 80, 24);
    let column = |line: &str, what: &str| {
        let at = line.find(what).unwrap_or(0);
        line[..at].chars().count()
    };
    assert_eq!(
        column(&lines[1], "+home") + 1,
        column(&lines[2], "+work") + 1
    );
    assert_eq!(column(&lines[1], "due"), column(&lines[2], "due"));
    assert!(lines[1].contains("a\u{2401}\u{2401}\u{2401}\u{2401}b"));
    assert!(lines[3].contains("\u{241b}[31mred\u{2421}"));
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(
        0, 0, 80, 24,
    ));
    model.view(&mut buf);
    assert!(
        buf.content()
            .iter()
            .all(|c| !c.symbol().chars().any(char::is_control))
    );
}
