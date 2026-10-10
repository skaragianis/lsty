use std::{fs, path::Path, sync::Arc, time::Duration};

use lsty_core::{
    config::Config,
    errs::Nr,
    list::State,
    lock::{self, Cancel, Mode, Want},
    ops::FileRecord,
    todotxt::Priority,
};

use crate::{
    actions::stepped,
    messages::{self, Tone},
    model::Model,
    test_util::{alone, deps, keys, press, started, todo_of, workdir},
};

const LIST: &str = "A +p\nB +p\n(A) C +p\nD +p\nE +p\n";

fn model_of(dir: &Path, config: Config) -> Model {
    started(deps(dir, config))
}

fn letter(c: u8) -> Option<Priority> {
    Priority::new(c)
}

/// Current priority, last letter, forward: the priority after.
type Step = (&'static str, Option<u8>, u8, bool, Option<u8>);

const STEPS: [Step; 10] = [
    ("none forward", None, b'C', true, Some(b'A')),
    ("A forward", Some(b'A'), b'C', true, Some(b'B')),
    ("last forward", Some(b'C'), b'C', true, None),
    ("past the range forward", Some(b'E'), b'C', true, None),
    ("none back", None, b'C', false, Some(b'C')),
    ("A back", Some(b'A'), b'C', false, None),
    ("B back", Some(b'B'), b'C', false, Some(b'A')),
    ("past the range back", Some(b'E'), b'C', false, Some(b'C')),
    ("last back", Some(b'C'), b'C', false, Some(b'B')),
    ("A forward, range A", Some(b'A'), b'A', true, None),
];

#[test]
fn the_priority_cycle() {
    for (name, cur, last, forward, want) in STEPS {
        let got = stepped(
            cur.and_then(letter),
            letter(last).unwrap_or(Priority::A),
            forward,
        );
        assert_eq!(got, want.and_then(letter), "{name}");
    }
}

type Mixed = (&'static str, &'static [State], Option<&'static str>);

const MIXED: [Mixed; 4] = [
    ("one state", &[State::Todo, State::Todo], None),
    (
        "two",
        &[State::Todo, State::Doing, State::Todo],
        Some("mixed states (2 todo, 1 doing) — nothing changed"),
    ),
    (
        "three, in the order of the states",
        &[State::Done, State::Doing, State::Todo],
        Some("mixed states (1 todo, 1 doing, 1 done) — nothing changed"),
    ),
    ("one item", &[State::Done], None),
];

#[test]
fn the_mixed_states_message() {
    for (name, states, want) in MIXED {
        assert_eq!(messages::mixed_states(states).as_deref(), want, "{name}");
    }
}

#[test]
fn the_archived_messages() {
    assert_eq!(
        messages::archived_item("Old task"),
        "Old task is archived — x restores it"
    );
    assert_eq!(
        messages::ARCHIVED_ITEMS,
        "the selection has archived items — nothing changed"
    );
    assert_eq!(
        messages::busy("todo.txt"),
        "todo.txt is busy (locked by another writer) — nothing changed, try again"
    );
}

/// The lines of a file that the record does not report, before and
/// after: they must be the same lines in the same order.
type Lines<'a> = Vec<&'a Arc<[u8]>>;

fn unreported(record: &FileRecord) -> (Lines<'_>, Lines<'_>) {
    let before: Vec<usize> = record
        .changes
        .iter()
        .filter_map(|c| c.nr_before)
        .map(Nr::index)
        .collect();
    let after: Vec<usize> = record
        .changes
        .iter()
        .filter_map(|c| c.nr_after)
        .map(Nr::index)
        .collect();
    (keep(record.pre(), &before), keep(record.post(), &after))
}

fn keep<'a>(lines: &'a [Arc<[u8]>], skip: &[usize]) -> Lines<'a> {
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !skip.contains(i))
        .map(|(_, line)| line)
        .collect()
}

/// One write: the todo file, the done file, the config, the keys.
type Case = (
    &'static str,
    &'static str,
    &'static str,
    Config,
    &'static str,
);

fn cases() -> Vec<Case> {
    let archive = Config {
        hidden_items: Vec::new(),
        ..alone()
    };
    let auto = Config {
        auto_archive: true,
        ..alone()
    };
    let spawn = "A\nPay rec:1w due:2026-09-30\nB\nC\n";
    vec![
        ("t", LIST, "", alone(), "jt"),
        ("x", LIST, "", alone(), "jx"),
        ("T", "A started:2026-09-29T0800Z\nB\n", "", alone(), "T"),
        ("dd", LIST, "", alone(), "jdd"),
        ("2dd", LIST, "", alone(), "j2dd"),
        ("gp", LIST, "", alone(), "jgp"),
        ("gP", LIST, "", alone(), "jgP"),
        ("2t", LIST, "", alone(), "j2t"),
        ("3gp", LIST, "", alone(), "j3gp"),
        ("2x", LIST, "", alone(), "j2x"),
        ("restore", LIST, "x 2026-09-01 Old\n", archive, "jjjjjx"),
        ("spawn", spawn, "", alone(), "jx"),
        ("archive", LIST, "", auto, "jx"),
    ]
}

// 34.28
#[test]
fn a_write_changes_only_the_lines_it_reports() {
    for (name, todo, done, config, keys) in cases() {
        let dir = workdir(todo);
        if !done.is_empty() {
            fs::write(dir.path().join("done.txt"), done)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let mut model = model_of(dir.path(), config);
        press(&mut model, keys);
        let Some(record) = model.hist.undo.last().map(|s| &s.rec) else {
            panic!("{name}: nothing was written");
        };
        assert!(!record.files.is_empty(), "{name}");
        for file in &record.files {
            let (before, after) = unreported(file);
            assert_eq!(before, after, "{name}: {:?}", file.source);
        }
    }
}

// 34.28: the check itself notices a line that changed unreported.
#[test]
fn the_check_of_reported_lines_sees_an_extra_change() {
    let record = FileRecord::new(
        lsty_core::errs::Source::Todo,
        vec![b"A".as_slice().into(), b"B".as_slice().into()],
        vec![b"A".as_slice().into(), b"X".as_slice().into()],
        Vec::new(),
        Vec::new(),
    );
    let (before, after) = unreported(&record);
    assert_ne!(before, after);
}

#[test]
fn a_write_keeps_the_undo_record_and_the_cursor_on_its_item() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "jjgp");
    assert_eq!(model.hist.undo.len(), 1);
    assert_eq!(model.cursor, 2);
    assert_eq!(model.tone, Tone::Notice);
    press(&mut model, "dd");
    assert_eq!(model.hist.undo.len(), 2);
    assert_eq!(model.cursor, 2, "the next shown item");
    assert_eq!(todo_of(dir.path()), "A +p\nB +p\n\nD +p\nE +p\n");
}

// A line that is not UTF-8 is guarded like any other.
#[test]
fn a_stale_view_does_not_write_a_line_that_is_not_utf8() {
    let dir = workdir(b"caf\xe9 A\nB\n");
    let path = dir.path().join("todo.txt");
    let mut model = model_of(dir.path(), alone());
    let outside = b"X\ncaf\xe9 A\nB\n";
    fs::write(&path, outside).unwrap_or_else(|e| panic!("{e}"));
    press(&mut model, "x");
    assert_eq!(fs::read(&path).unwrap_or_default(), outside);
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(
        model.message.as_deref(),
        Some("item 1 is \"X\", expected \"caf\u{fffd} A\"")
    );
    assert_eq!(model.hist.undo.len(), 0);
}

#[test]
fn a_refused_write_changes_nothing_and_records_nothing() {
    let dir = workdir("A\nx 2026-09-29 B\nC id:c\nD dep:c\n");
    let before = todo_of(dir.path());
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "3x");
    assert_eq!(
        model.message.as_deref(),
        Some("mixed states (2 todo, 1 done) — nothing changed")
    );
    assert_eq!(model.tone, Tone::Alert);
    keys(&mut model, "jjjx");
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(todo_of(dir.path()), before);
    assert_eq!(model.hist.undo.len(), 0);
    press(&mut model, "k");
    assert_eq!(model.tone, Tone::Notice, "a key clears the tone");
}

#[test]
fn a_busy_lock_changes_nothing_and_records_nothing() {
    let dir = workdir("Buy\n");
    let config = Config {
        lock_timeout: Duration::from_millis(30),
        ..alone()
    };
    let mut model = model_of(dir.path(), config);
    let want = Want {
        file: dir.path().join("todo.txt"),
        mode: Mode::Exclusive,
    };
    let mut held =
        lock::acquire(&[want], Duration::from_secs(1), &Cancel::new())
            .unwrap_or_else(|e| panic!("{}", e.message()));
    press(&mut model, "x");
    assert_eq!(
        model.message.as_deref(),
        Some(
            "todo.txt is busy (locked by another writer) \
             — nothing changed, try again"
        )
    );
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(model.hist.undo.len(), 0);
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    press(&mut model, "x");
    assert_eq!(todo_of(dir.path()), "x 2026-09-29 Buy\n");
}

#[test]
fn nothing_is_written_to_a_list_that_has_no_file() {
    let dir = workdir("");
    let mut model = model_of(dir.path(), alone());
    keys(&mut model, "xdd");
    assert!(!dir.path().join("todo.txt").exists());
    assert_eq!(model.hist.undo.len(), 0);
}

#[test]
fn the_state_decides_what_t_does() {
    let dir = workdir("A\n");
    let mut model = model_of(dir.path(), alone());
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut model, "t");
        seen.push(todo_of(dir.path()));
    }
    assert_eq!(
        seen,
        [
            "A started:2026-09-29T0900Z\n",
            "x 2026-09-29 A started:2026-09-29T0900Z\n",
            "A started:2026-09-29T0900Z\n",
            "x 2026-09-29 A started:2026-09-29T0900Z\n",
        ]
    );
}
