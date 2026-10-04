use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use lsty_core::keys::Mode;
use ratatui::crossterm::event::{
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::detail::detail_rows;
use crate::model::Model;
use crate::segments::line_field;
use crate::terminal::mouse_of;
use crate::test_util::{NOW, alone, deps, instant, started, workdir};
use crate::{Button, Gesture, Mouse, Msg};

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

const PLACES: [(&str, Place); 5] = [
    // 42.29
    ("inside the text", ("A +work", 0, 3, 3)),
    // 42.30
    ("nearer the end of a date", ("2026-09-01 A", 0, 7, 11)),
    ("nearer the start of a date", ("2026-09-01 A", 0, 3, 0)),
    ("past the end of the text", ("A +work", 0, 40, 7)),
    (
        "before the floor of a done item",
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
            Some(Gesture::Up(Button::Other)),
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
        assert!(got.is_none_or(|m| (m.x, m.y) == (7, 3) && !m.shift));
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
    for gesture in [Gesture::Down(Button::Left), Gesture::Up(Button::Left)] {
        let shift = false;
        model.update(Msg::Mouse(Mouse {
            gesture,
            x,
            y,
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
        assert_eq!(model.mode == Mode::Insert, doubled, "{millis} ms");
    }
}

#[test]
fn a_click_on_another_row_is_no_double_click() {
    let dir = workdir("A\nB\nC\n");
    let (mut model, clock) = timed(dir.path());
    click(&mut model, 5, 1);
    after(&clock, 10);
    click(&mut model, 5, 2);
    assert_eq!(model.mode, Mode::Normal);
    assert_eq!(model.cursor, 1);
}

fn long_list(n: usize) -> String {
    let lines: Vec<String> = (1..=n).map(|i| format!("Item {i:03}")).collect();
    lines.join("\n") + "\n"
}

fn wheel(model: &mut Model, up: bool, x: u16, y: u16) {
    let gesture = Gesture::Wheel { up };
    let shift = false;
    model.update(Msg::Mouse(Mouse {
        gesture,
        x,
        y,
        shift,
    }));
}

#[test]
fn the_wheel_moves_three_rows_and_keeps_the_cursor_on_screen() {
    let dir = workdir(&long_list(100));
    let mut model = started(deps(dir.path(), alone()));
    wheel(&mut model, false, 10, 5);
    assert_eq!((model.top, model.cursor), (3, 3));
    wheel(&mut model, true, 10, 5);
    assert_eq!((model.top, model.cursor), (0, 3));
    wheel(&mut model, true, 10, 5);
    assert_eq!((model.top, model.cursor), (0, 3));
    for _ in 0..100 {
        wheel(&mut model, false, 10, 5);
    }
    let last = model.rows.len() - model.pane_rows();
    assert_eq!(model.top, last);
    assert_eq!(model.cursor, model.top);
}

#[test]
fn the_wheel_does_nothing_over_the_status_line_or_a_closed_input() {
    let dir = workdir(&long_list(100));
    let mut model = started(deps(dir.path(), alone()));
    wheel(&mut model, false, 10, 23);
    assert_eq!(model.top, 0);
    model.update(Msg::Key(lsty_core::keys::Stroke::char(':')));
    wheel(&mut model, false, 10, 5);
    assert_eq!(model.top, 0);
}

#[test]
fn a_drag_below_the_pane_scrolls_and_grows_the_selection() {
    let dir = workdir(&long_list(100));
    let mut model = started(deps(dir.path(), alone()));
    let down = Gesture::Down(Button::Left);
    let drag = Gesture::Drag(Button::Left);
    let mut send = |gesture, y| {
        let shift = false;
        model.update(Msg::Mouse(Mouse {
            gesture,
            x: 5,
            y,
            shift,
        }));
    };
    send(down, 20);
    for _ in 0..10 {
        send(drag, 23);
    }
    assert_eq!(model.mode, Mode::Visual);
    assert_eq!(model.top, 10);
    assert_eq!(model.selection(), (19, 30));
}

/// Hit testing a click, a wheel notch and a drag over the rows of a
/// file of 10 000 lines stays far from the time a frame takes.
#[test]
fn the_mouse_is_quick_on_a_long_list() {
    let dir = workdir(&long_list(10_000));
    fs::write(dir.path().join("config.toml"), "").unwrap_or_default();
    let mut model = started(deps(dir.path(), alone()));
    let started_at = Instant::now();
    for i in 0..200 {
        click(&mut model, 5, 1 + (i % 20));
        wheel(&mut model, false, 10, 5);
    }
    let drag = Gesture::Drag(Button::Left);
    model.update(Msg::Mouse(Mouse {
        gesture: Gesture::Down(Button::Left),
        x: 5,
        y: 2,
        shift: false,
    }));
    for _ in 0..2000 {
        model.update(Msg::Mouse(Mouse {
            gesture: drag,
            x: 5,
            y: 23,
            shift: false,
        }));
    }
    assert!(started_at.elapsed() < Duration::from_secs(20));
}
