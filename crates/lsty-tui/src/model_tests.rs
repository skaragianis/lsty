use std::{cell::Cell, fs, rc::Rc, time::Duration};

use lsty_core::{
    config::{Config, Warning},
    keys::Action,
};

use crate::{
    messages::Tone,
    model::Model,
    motion::Step,
    msg::{Cmd, Msg},
    test_util::{
        NOW, SAMPLE, alone, deps, instant, keys, press, screen, start,
        started, started_from, todo_of, workdir, workdir_model,
    },
    watch::Notice,
};

fn quits(cmds: &[Cmd]) -> bool {
    cmds.contains(&Cmd::Quit)
}

#[test]
fn quit() {
    const ROWS: [(&str, bool); 5] = [
        ("q", true),
        ("ZZ", true),
        ("ZQ", true),
        ("Z", false),
        ("!", false),
    ];
    for (keys, want) in ROWS {
        let (dir, mut model) = workdir_model(SAMPLE, Config::default());
        let cmds = press(&mut model, keys);
        assert_eq!(quits(&cmds), want, "keys {keys}");
        assert_eq!(todo_of(dir.path()), SAMPLE, "keys {keys}");
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
fn an_item_without_links_only_says_so() {
    let (dir, mut model) = workdir_model(SAMPLE, Config::default());
    let before = screen(&model, 80, 24);
    let cmds = press(&mut model, "gx");
    assert_eq!(cmds.len(), 0);
    assert_eq!(model.message.as_deref(), Some("no file: link on this item"));
    let after = screen(&model, 80, 24);
    assert_eq!(after.get(..23), before.get(..23));
    assert_eq!(todo_of(dir.path()), SAMPLE);
}

#[test]
fn no_file_is_created_for_a_missing_file_or_folder() {
    for folder_missing in [false, true] {
        let dir = workdir("");
        let d = deps(dir.path(), Config::default());
        let mut s = start(dir.path());
        if folder_missing {
            s.found.todo = dir.path().join("no").join("todo.txt");
            s.found.done = dir.path().join("no").join("done.txt");
            s.found.state = lsty_core::discover::FileState::DirMissing;
        }
        let model = started_from(s, d);
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
    let (_dir, mut model) = workdir_model(&long_list(10), Config::default());
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
    let (_dir, mut model) = workdir_model(todo, Config::default());
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
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    model.update(Msg::Resize(80, 12));
    // 12 rows: 1 status, 2 border, 9 items; half is 4.
    keys(&mut model, "<C-d>");
    assert_eq!(model.cursor, 4);
    assert_eq!(model.top, 0);
    for _ in 0..2 {
        keys(&mut model, "<C-d>");
    }
    assert_eq!(model.cursor, 12);
    assert_eq!(model.top, 4);
}

#[test]
fn an_action_has_a_step_exactly_when_the_table_calls_it_a_motion() {
    for &action in Action::ALL {
        let step = Step::of(action, 1, 4).is_some();
        assert_eq!(step, action.is_motion(), "{action}");
    }
}

#[test]
fn the_view_scrolls_by_the_least_that_shows_the_cursor() {
    let (_dir, mut model) = workdir_model(&long_list(30), alone());
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
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
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
    let (_dir, model) = workdir_model(SAMPLE, Config::default());
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
    let d = deps(dir.path(), Config::default());
    let mut s = start(dir.path());
    s.warnings = ["one", "two"]
        .map(|m| Warning {
            source: "config.toml".to_owned(),
            message: m.to_owned(),
        })
        .to_vec();
    let mut model = started_from(s, d);
    let status = |m: &Model| screen(m, 80, 24).pop().unwrap_or_default();
    assert!(status(&model).contains("config.toml: one (1 of 2)"));
    press(&mut model, "j");
    assert!(status(&model).contains("config.toml: two (2 of 2)"));
    press(&mut model, "j");
    assert!(!status(&model).contains("config.toml"));
}

/// The message timer among `cmds`.
fn expiry(cmds: &[Cmd]) -> Option<Msg> {
    cmds.iter().find_map(|c| match c {
        Cmd::After {
            delay,
            msg: Msg::Expire(n),
        } if *delay == Duration::from_secs(2) => Some(Msg::Expire(*n)),
        Cmd::Quit
        | Cmd::After { .. }
        | Cmd::Copy(_)
        | Cmd::Edit(_)
        | Cmd::Load(_) => None,
    })
}

/// Presses `gx` on an item without links, and gives back the timer of
/// its message.
fn no_link(model: &mut Model) -> Msg {
    let cmds = keys(model, "gx");
    let Some(timer) = expiry(&cmds) else {
        panic!("no timer in {cmds:?}")
    };
    timer
}

#[test]
fn a_message_is_cleared_two_seconds_after_it_is_put_up() {
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
    let before = screen(&model, 80, 24).pop();
    let timer = no_link(&mut model);
    model.tone = Tone::Alert;
    assert_ne!(screen(&model, 80, 24).pop(), before);
    let cmds = model.update(timer);
    assert_eq!(model.message, None);
    assert_eq!(model.tone, Tone::Notice);
    assert_eq!(expiry(&cmds), None);
    assert_eq!(screen(&model, 80, 24).pop(), before);
}

#[test]
fn a_timer_clears_only_the_message_it_was_set_for() {
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
    let first = no_link(&mut model);
    let second = no_link(&mut model);
    assert_ne!(first, second);
    model.update(first);
    assert!(model.message.is_some());
    model.update(second);
    assert_eq!(model.message, None);
}

#[test]
fn a_timer_leaves_a_later_message_that_input_did_not_cause() {
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
    let timer = no_link(&mut model);
    let cmds = model.update(Msg::Notice(Notice::Failed("gone".to_owned())));
    assert_eq!(expiry(&cmds), None);
    model.update(timer);
    let shown = model.message.as_deref().unwrap_or_default();
    assert!(shown.starts_with("can't watch"), "{shown}");
}

#[test]
fn an_error_stays_until_a_key() {
    let (_dir, mut model) = workdir_model("Read file:gone.md\n", alone());
    let cmds = keys(&mut model, "gx");
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(expiry(&cmds), None);
}

#[test]
fn a_key_without_a_message_sets_no_timer() {
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
    assert_eq!(expiry(&keys(&mut model, "j")), None);
}

#[test]
fn a_warning_shown_at_launch_stays_until_a_key() {
    let dir = workdir(SAMPLE);
    let d = deps(dir.path(), Config::default());
    let mut s = start(dir.path());
    s.warnings = vec![Warning {
        source: "config.toml".to_owned(),
        message: "one".to_owned(),
    }];
    let mut model =
        Model::new(s, d).unwrap_or_else(|e| panic!("{}", e.message()));
    assert_eq!(expiry(&model.init()), None);
    assert_eq!(expiry(&model.update(Msg::Resize(80, 24))), None);
    assert_eq!(model.message.as_deref(), Some("config.toml: one"));
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
    assert_eq!(model.rows[model.cursor].nr.get(), 3);
    clock.set(instant("2026-09-30T00:00:00Z"));
    model.update(Msg::Minute);
    assert_eq!(model.rows.len(), 4, "the later item is listed now");
    assert_eq!(model.rows[model.cursor].nr.get(), 3);
}

#[test]
fn a_tick_keeps_the_row_where_it_was_on_screen() {
    let (_dir, mut model) = workdir_model(&long_list(30), Config::default());
    model.update(Msg::Resize(80, 12));
    press(&mut model, "20j");
    let (nr, offset) =
        (model.rows[model.cursor].nr.get(), model.cursor - model.top);
    model.update(Msg::Minute);
    assert_eq!(model.rows[model.cursor].nr.get(), nr);
    assert_eq!(model.cursor - model.top, offset);
}

/// The terminal may be smaller than the last size the model was told.
#[test]
fn drawing_into_a_smaller_buffer_stays_inside_it() {
    let (_dir, mut model) = workdir_model(&long_list(50), Config::default());
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
    let (_dir, model) = workdir_model(todo, Config::default());
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
    model.draw(&mut buf);
    assert!(
        buf.content()
            .iter()
            .all(|c| !c.symbol().chars().any(char::is_control))
    );
}
