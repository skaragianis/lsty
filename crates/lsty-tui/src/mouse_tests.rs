use std::{cell::Cell, fs, path::Path, rc::Rc};

use jiff::{SignedDuration, Timestamp};
use lsty_core::{
    config::{Config, Hidden},
    keys::Mode,
};
use ratatui::{
    crossterm::event::{
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    layout::Position,
};
use tempfile::TempDir;

use crate::{
    detail::detail_rows,
    model::Model,
    msg::{Button, Gesture, Mouse, Msg},
    segments::line_field,
    terminal::mouse_of,
    test_util::{
        NOW, SAMPLE, alone, cursor_text, deps, expiry, instant, keys, press,
        reload, started, todo_of, workdir, workdir_model,
    },
};

/// Lines of the document, lines of the panel, the first line shown, and
/// which line each panel line shows.
type View = (usize, usize, usize, &'static [Option<usize>]);

const VIEWS: [(&str, View); 6] = [
    // 42.35
    (
        "top of long content",
        (
            20,
            8,
            0,
            &[
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                None,
            ],
        ),
    ),
    (
        "after one notch",
        (
            20,
            8,
            3,
            &[
                None,
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                None,
            ],
        ),
    ),
    // 42.36
    (
        "end of content",
        (
            20,
            8,
            99,
            &[
                None,
                Some(13),
                Some(14),
                Some(15),
                Some(16),
                Some(17),
                Some(18),
                Some(19),
            ],
        ),
    ),
    (
        "top again",
        (
            20,
            8,
            0,
            &[
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                None,
            ],
        ),
    ),
    // 42.38
    ("content that fits", (3, 8, 5, &[Some(0), Some(1), Some(2)])),
    (
        "content as long as the panel",
        (
            8,
            8,
            1,
            &[
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
            ],
        ),
    ),
];

#[test]
fn the_detail_panel_shows_the_lines_its_view_starts_at() {
    for (name, (n, h, top, want)) in VIEWS {
        assert_eq!(detail_rows(n, h, top), want, "{name}");
    }
}

/// The line, the row and column pressed, and the position the text
/// cursor goes to.
type Place = (&'static str, usize, usize, usize);

const PLACES: [(&str, Place); 4] = [
    // 42.29
    ("inside the text", ("A +work", 0, 3, 3)),
    // 42.30
    ("inside a date", ("2026-09-01 A", 0, 7, 7)),
    ("past the end of the text", ("A +work", 0, 40, 7)),
    (
        "before the floor of a done task",
        ("x 2026-09-28 Fix", 0, 0, 2),
    ),
];

#[test]
fn a_press_in_the_field_rests_on_the_nearest_editable_place() {
    for (name, (raw, row, col, want)) in PLACES {
        let field = line_field(raw, 0);
        assert_eq!(field.place_at(40, row, col), want, "{name}");
    }
}

fn event(kind: MouseEventKind, mods: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        column: 7,
        row: 3,
        modifiers: mods,
    }
}

#[test]
fn the_terminal_mouse_is_the_loops_mouse() {
    let none = KeyModifiers::NONE;
    let left = MouseButton::Left;
    let cases = [
        (
            event(MouseEventKind::Down(left), none),
            Some(Gesture::Down(Button::Left)),
        ),
        (
            event(MouseEventKind::Up(MouseButton::Right), none),
            Some(Gesture::Up),
        ),
        (
            event(MouseEventKind::Drag(MouseButton::Middle), none),
            Some(Gesture::Drag(Button::Other)),
        ),
        (
            event(MouseEventKind::ScrollUp, none),
            Some(Gesture::Wheel { up: true }),
        ),
        (
            event(MouseEventKind::ScrollDown, none),
            Some(Gesture::Wheel { up: false }),
        ),
        (event(MouseEventKind::Moved, none), None),
        (event(MouseEventKind::ScrollLeft, none), None),
    ];
    for (raw, gesture) in cases {
        let got = mouse_of(raw);
        assert_eq!(got.map(|m| m.gesture), gesture, "{raw:?}");
        assert!(got.is_none_or(|m| m.at == Position::new(7, 3) && !m.shift));
    }
    let shifted = event(MouseEventKind::Down(left), KeyModifiers::SHIFT);
    assert!(mouse_of(shifted).is_some_and(|m| m.shift));
}

/// A model of `todo` whose clock the test sets.
fn timed(dir: &Path) -> (Model, Rc<Cell<Timestamp>>) {
    let clock = Rc::new(Cell::new(instant(NOW)));
    let mut d = deps(dir, alone());
    let read = Rc::clone(&clock);
    d.now = Box::new(move || read.get());
    (started(d), clock)
}

fn click(model: &mut Model, x: u16, y: u16) {
    for gesture in [Gesture::Down(Button::Left), Gesture::Up] {
        let shift = false;
        model.update(Msg::Mouse(Mouse {
            gesture,
            at: Position::new(x, y),
            shift,
        }));
    }
}

fn after(clock: &Cell<Timestamp>, millis: i64) {
    let step = SignedDuration::from_millis(millis);
    if let Ok(at) = clock.get().checked_add(step) {
        clock.set(at);
    }
}

/// How long after a click on a row a second click on it is still a
/// double-click: the answer is whether INSERT mode opened.
const GAPS: [(i64, bool); 4] =
    [(0, true), (299, true), (300, true), (301, false)];

#[test]
fn a_double_click_is_two_clicks_within_300_ms() {
    for (millis, doubled) in GAPS {
        let dir = workdir("A\nB\nC\n");
        let (mut model, clock) = timed(dir.path());
        click(&mut model, 5, 2);
        after(&clock, millis);
        click(&mut model, 5, 2);
        assert_eq!(model.mode() == Mode::Insert, doubled, "{millis} ms");
    }
}

#[test]
fn a_click_on_another_row_is_no_double_click() {
    let dir = workdir("A\nB\nC\n");
    let (mut model, clock) = timed(dir.path());
    click(&mut model, 5, 1);
    after(&clock, 10);
    click(&mut model, 5, 2);
    assert_eq!(model.mode(), Mode::Normal);
    assert_eq!(model.cursor, 1);
}

fn long_list(n: usize) -> String {
    let lines: Vec<String> = (1..=n).map(|i| format!("Task {i:03}")).collect();
    lines.join("\n") + "\n"
}

fn wheel(model: &mut Model, up: bool, x: u16, y: u16) {
    let gesture = Gesture::Wheel { up };
    let shift = false;
    model.update(Msg::Mouse(Mouse {
        gesture,
        at: Position::new(x, y),
        shift,
    }));
}

#[test]
fn the_wheel_moves_three_rows_and_leaves_the_cursor_on_its_task() {
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    wheel(&mut model, false, 10, 5);
    assert_eq!((model.top, model.cursor), (3, 0));
    assert!(model.cursor_hidden());
    wheel(&mut model, true, 10, 5);
    assert_eq!((model.top, model.cursor), (0, 0));
    assert!(!model.cursor_hidden());
    wheel(&mut model, true, 10, 5);
    assert_eq!((model.top, model.cursor), (0, 0));
    for _ in 0..100 {
        wheel(&mut model, false, 10, 5);
    }
    assert_eq!(model.top, model.rows.len() - model.pane_rows());
    assert_eq!(model.cursor, 0);
}

#[test]
fn a_key_brings_the_hidden_cursor_back_before_it_acts() {
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    for _ in 0..4 {
        wheel(&mut model, false, 10, 5);
    }
    press(&mut model, "j");
    assert_eq!(model.cursor, 1);
    assert!(!model.cursor_hidden());
    assert_eq!(model.top, 0);
}

#[test]
fn changes_while_the_cursor_is_out_of_view_keep_the_view() {
    let (dir, mut model) = workdir_model(&long_list(100), alone());
    for _ in 0..4 {
        wheel(&mut model, false, 10, 5);
    }
    model.rebuild();
    assert_eq!((model.top, model.cursor), (12, 0));
    model.update(Msg::Resize(80, 30));
    assert_eq!((model.top, model.cursor), (12, 0));
    for _ in 0..100 {
        wheel(&mut model, false, 10, 5);
    }
    fs::write(dir.path().join("todo.txt"), long_list(101))
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(model.rows.len(), 101);
    assert_eq!(model.top, 101 - model.pane_rows());
    assert_eq!(model.cursor, 0);
}

#[test]
fn the_wheel_keeps_the_selection_in_visual_mode() {
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    press(&mut model, "vj");
    wheel(&mut model, false, 10, 5);
    assert_eq!(model.mode(), Mode::Visual);
    assert_eq!(model.selected_rows(), Some((0, 1)));
}

#[test]
fn the_wheel_does_nothing_over_the_status_line_or_a_closed_input() {
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    wheel(&mut model, false, 10, 23);
    assert_eq!(model.top, 0);
    press(&mut model, ":");
    wheel(&mut model, false, 10, 5);
    assert_eq!(model.top, 0);
}

#[test]
fn a_drag_below_the_pane_scrolls_and_grows_the_selection() {
    let (_dir, mut model) = workdir_model(&long_list(100), alone());
    let down = Gesture::Down(Button::Left);
    let drag = Gesture::Drag(Button::Left);
    let mut send = |gesture, y| {
        let shift = false;
        model.update(Msg::Mouse(Mouse {
            gesture,
            at: Position::new(5, y),
            shift,
        }));
    };
    send(down, 20);
    for _ in 0..10 {
        send(drag, 23);
    }
    assert_eq!(model.mode(), Mode::Visual);
    assert_eq!(model.top, 10);
    assert_eq!(model.selected_rows(), Some((19, 30)));
}

#[test]
fn a_cut_wide_chip_has_no_hit_on_the_blank_after_its_ellipsis() {
    let (_dir, mut model) = workdir_model(SAMPLE, Config::default());
    model.search = "日本語".to_owned();
    // "[1]─" and "/日" and "…" leave one blank cell of the nine.
    assert_eq!(model.chip_cells(13), vec![(6, 4)]);
    assert_eq!(model.chip_cells(30), vec![(6, 7)]);
}

/// A model of A to E after `typed`, which holds a row, and the timer its
/// last key set.
fn held_after(typed: &str) -> (TempDir, Model, Option<Msg>) {
    let config = Config {
        hidden_tasks: vec![Hidden::Done],
        ..Config::default()
    };
    let (dir, mut m) = workdir_model("A\nB\nC\nD\nE\n", config);
    m.update(Msg::Resize(100, 30));
    let timer = expiry(&keys(&mut m, typed));
    (dir, m, timer)
}

fn send(m: &mut Model, gesture: Gesture, y: u16) {
    let at = Position::new(20, y);
    let shift = false;
    m.update(Msg::Mouse(Mouse { gesture, at, shift }));
}

type Events<'a> = &'a [(Gesture, u16)];

// Rows held in view stay under a pressed button, whatever else happens
// meanwhile, and leave with its release: a click hits what was under
// the pointer when it went down.
#[test]
fn a_gesture_after_x_hits_the_rows_it_pressed() {
    let down = Gesture::Down(Button::Left);
    let drag = Gesture::Drag(Button::Left);
    let wheel = Gesture::Wheel { up: false };
    let up = Gesture::Up;
    let cases: [(Events<'_>, &str, Mode); 4] = [
        (&[(down, 4), (up, 4)], "D", Mode::Normal),
        (&[(down, 5), (up, 5)], "E", Mode::Normal),
        (&[(down, 5), (drag, 4), (up, 4)], "D", Mode::Visual),
        (&[(down, 4), (wheel, 4), (up, 4)], "D", Mode::Normal),
    ];
    for (events, at, mode) in cases {
        let (_dir, mut m, _) = held_after("jjx");
        for &(gesture, y) in events {
            send(&mut m, gesture, y);
        }
        assert_eq!(cursor_text(&m), at, "{events:?}");
        assert_eq!(m.mode(), mode, "{events:?}");
        assert!(m.rows.iter().all(|r| !r.sticky), "{events:?}");
    }
}

#[test]
fn the_timer_waits_for_the_button_and_a_key_does_not() {
    let (_dir, mut m, timer) = held_after("jjx");
    send(&mut m, Gesture::Down(Button::Left), 4);
    m.update(timer.expect("a timer was set"));
    assert_eq!(m.rows.iter().filter(|r| r.sticky).count(), 1);
    send(&mut m, Gesture::Up, 4);
    assert_eq!(cursor_text(&m), "D");
    assert!(m.rows.iter().all(|r| !r.sticky));
    // A press whose release never comes is let go at the next key, and
    // so is the row the cursor was on.
    let (_dir, mut m, timer) = held_after("Gx");
    send(&mut m, Gesture::Down(Button::Left), 2);
    m.update(timer.expect("a timer was set"));
    keys(&mut m, "<Esc>");
    assert!(m.rows.iter().all(|r| !r.sticky));
}

// A key ends the press, so its release clicks nothing.
#[test]
fn a_key_ends_the_press_and_its_release_clicks_nothing() {
    let (_dir, mut m, _) = held_after("jjx");
    send(&mut m, Gesture::Down(Button::Left), 4);
    keys(&mut m, "<Esc>");
    send(&mut m, Gesture::Up, 4);
    assert_eq!(cursor_text(&m), "D");
}

// An up that the command line swallows leaves no press set.
#[test]
fn a_release_the_command_line_swallows_leaves_no_press() {
    let (_dir, mut m, _) = held_after("jjx");
    send(&mut m, Gesture::Down(Button::Left), 4);
    keys(&mut m, ":");
    send(&mut m, Gesture::Up, 4);
    keys(&mut m, "<Esc>");
    assert!(!m.pointer.pressed());
}

#[test]
fn a_click_on_a_name_steps_it_like_x() {
    let todo = "A +work @phone @email\nB +work @phone\nC +home @email\n\
        D +work.acme @phone @email\nE +home +work\n";
    let config = Config {
        show_sidebar: true,
        show_detail: false,
        ..alone()
    };
    let (_dir, mut m) = workdir_model(todo, config);
    m.update(Msg::Resize(100, 30));
    let chips = |m: &Model| -> Vec<String> {
        m.chips().into_iter().map(|c| c.text).collect()
    };
    click(&mut m, 3, 9);
    assert_eq!(chips(&m), ["@email"]);
    click(&mut m, 3, 10);
    assert_eq!(chips(&m), ["@email|@phone"]);
    click(&mut m, 3, 10);
    assert_eq!(chips(&m), ["@email", "@phone"]);
    click(&mut m, 3, 10);
    click(&mut m, 3, 10);
    assert_eq!(chips(&m), ["@email"]);
}

// A gesture on the list acts on the list of the last panel step.
#[test]
fn a_gesture_outside_the_entries_refilters_first() {
    let config = Config {
        show_sidebar: true,
        show_detail: false,
        ..alone()
    };
    let gestures = [
        Gesture::Down(Button::Left),
        Gesture::Drag(Button::Left),
        Gesture::Wheel { up: false },
    ];
    for gesture in gestures {
        let (_dir, mut m) =
            workdir_model("A @a\nB @b\nC @a\n", config.clone());
        m.update(Msg::Resize(100, 30));
        keys(&mut m, "<C-w>4x");
        assert_eq!((m.rows.len(), m.refilter.owed), (3, true));
        let at = Position::new(40, 2);
        m.update(Msg::Mouse(Mouse {
            gesture,
            at,
            shift: false,
        }));
        assert_eq!((m.rows.len(), m.refilter.owed), (2, false), "{gesture:?}");
    }
}

/// The cursor line after the button went down at row `y`, the file was
/// read again as `after` and the button came up there.
fn clicked_across_a_reload(
    before: &str,
    typed: &str,
    y: u16,
    after: &str,
) -> String {
    let (dir, mut m) = workdir_model(before, alone());
    m.update(Msg::Resize(100, 12));
    keys(&mut m, typed);
    send(&mut m, Gesture::Down(Button::Left), y);
    fs::write(dir.path().join("todo.txt"), after).unwrap();
    reload(&mut m);
    send(&mut m, Gesture::Up, y);
    cursor_text(&m)
}

// A release clicks only when the line under the pointer is the line
// that was pressed.
#[test]
fn a_reload_between_press_and_release_clicks_what_is_still_pressed() {
    const SHORT: &str = "A\nB\nC\nD\nE\n";
    let long = long_list(40);
    let shifted = format!("Task 000\n{long}");
    let rows = [
        ("added above", SHORT, "gg", 3, "A\nNEW\nB\nC\nD\nE\n", "A"),
        ("removed above", SHORT, "gg", 3, "A\nC\nD\nE\n", "A"),
        ("pressed line removed", SHORT, "gg", 3, "A\nB\nD\nE\n", "A"),
        ("added below", SHORT, "gg", 3, "A\nB\nC\nD\nE\nF\n", "C"),
        // The view follows the cursor, so the pressed line stays put.
        ("scrolled with it", &long, "20j", 5, &shifted, "Task 017"),
    ];
    for (name, before, typed, y, after, want) in rows {
        let got = clicked_across_a_reload(before, typed, y, after);
        assert_eq!(got, want, "{name}");
    }
}

// A name of a panel is no place of the files: a reload drops the press.
#[test]
fn a_reload_drops_a_press_on_a_name() {
    let config = Config {
        show_sidebar: true,
        show_detail: false,
        ..alone()
    };
    let (dir, mut m) = workdir_model("A @phone\nB @email\n", config);
    m.update(Msg::Resize(100, 30));
    let at = Position::new(3, 9);
    let shift = false;
    m.update(Msg::Mouse(Mouse {
        gesture: Gesture::Down(Button::Left),
        at,
        shift,
    }));
    fs::write(dir.path().join("todo.txt"), "A @phone\nB @email\nC @aaa\n")
        .unwrap();
    reload(&mut m);
    m.update(Msg::Mouse(Mouse {
        gesture: Gesture::Up,
        at,
        shift,
    }));
    let chips: Vec<String> = m.chips().into_iter().map(|c| c.text).collect();
    assert_eq!(chips, Vec::<String>::new());
}

/// A wrapping list, 40 by 12: a task of three rows, then B and C.
fn wrapped_list(dir: &Path, hidden: Vec<Hidden>) -> Model {
    let config = Config {
        wrap: true,
        hidden_tasks: hidden,
        ..alone()
    };
    let mut model = started(deps(dir, config));
    model.update(Msg::Resize(40, 12));
    model
}

const LONG_FIRST: &str =
    "aaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbb ccc\nB\nC\n";

#[test]
fn a_double_click_on_a_later_row_opens_the_task() {
    let dir = workdir(LONG_FIRST);
    let clock = Rc::new(Cell::new(instant(NOW)));
    let mut d = deps(
        dir.path(),
        Config {
            wrap: true,
            ..alone()
        },
    );
    let read = Rc::clone(&clock);
    d.now = Box::new(move || read.get());
    let mut model = started(d);
    model.update(Msg::Resize(40, 12));
    click(&mut model, 5, 1);
    after(&clock, 10);
    click(&mut model, 5, 2);
    assert_eq!((model.mode(), model.cursor), (Mode::Insert, 0));
}

#[test]
fn a_drag_past_the_edge_of_a_wrapping_list_scrolls_by_tasks() {
    let dir = workdir(format!("{}{}", LONG_FIRST, "D\n".repeat(20)));
    let mut model = wrapped_list(dir.path(), Vec::new());
    let (down, drag) =
        (Gesture::Down(Button::Left), Gesture::Drag(Button::Left));
    send(&mut model, down, 1);
    send(&mut model, drag, 11);
    assert_eq!((model.mode(), model.top), (Mode::Visual, 1));
    assert_eq!(model.cursor, 9);
    send(&mut model, drag, 0);
    assert_eq!(model.top, 0);
}

#[test]
fn a_row_held_in_view_stays_wrapped() {
    let dir = workdir(LONG_FIRST);
    let mut model = wrapped_list(dir.path(), vec![Hidden::Done]);
    keys(&mut model, "x");
    assert_eq!(model.rows.iter().filter(|r| r.sticky).count(), 1);
    let lines = crate::test_util::screen(&model, 40, 12);
    assert!(lines[1].contains("aaaa"), "{lines:?}");
    assert!(lines[3].contains("ccc"), "{lines:?}");
    assert!(lines[4].contains('B'), "{lines:?}");
}

// A drag that starts a selection drops the keys typed before it.
#[test]
fn a_drag_forgets_the_keys_typed_before_it() {
    let (dir, mut m) = workdir_model("A\nB\nC\n", alone());
    m.update(Msg::Resize(100, 12));
    keys(&mut m, "vg");
    for (gesture, y) in [
        (Gesture::Down(Button::Left), 1),
        (Gesture::Drag(Button::Left), 2),
        (Gesture::Up, 2),
    ] {
        send(&mut m, gesture, y);
    }
    keys(&mut m, "p");
    assert_eq!(todo_of(dir.path()), "A\nB\nC\n");
}
