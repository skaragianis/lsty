use std::{fs, path::Path, time::SystemTime};

use lsty_core::{
    config::{Config, Filter, Hidden},
    errs::Source,
    list::SortField,
};

use crate::{
    messages::{self, Tone},
    model::Model,
    test_util::{alone, deps, keys, press, reload, started, todo_of, workdir},
};

fn config(hidden: &[Hidden]) -> Config {
    Config {
        hidden_items: hidden.to_vec(),
        ..alone()
    }
}

fn model_of(dir: &Path, config: Config) -> Model {
    started(deps(dir, config))
}

/// The text of each row of the list, with `*` after a sticky one.
fn rows_of(model: &Model) -> Vec<String> {
    model
        .rows
        .iter()
        .filter_map(|row| {
            let item = model.snap.item(row.id)?;
            let raw = model.snap.line(item).raw();
            let text = String::from_utf8_lossy(raw).into_owned();
            Some(if row.sticky { format!("{text}*") } else { text })
        })
        .collect()
}

fn modified(dir: &Path) -> Option<SystemTime> {
    fs::metadata(dir.join("todo.txt")).ok()?.modified().ok()
}

/// The list, the keys, the file after them and the row of the cursor.
type Move = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    usize,
);

const MOVES: [Move; 8] = [
    // 34a.1, 34a.2
    ("down", "A\nB\nC\n", "J", "B\nA\nC\n", 1),
    ("up", "A\nB\nC\n", "jjK", "A\nC\nB\n", 1),
    // 34a.3: one write for a count
    ("count", "A\nB\nC\nD\n", "2J", "B\nC\nA\nD\n", 2),
    // 34a.4: as far as it goes
    ("past the end", "A\nB\nC\n", "9J", "B\nC\nA\n", 2),
    ("past the top", "A\nB\nC\n", "jj9K", "C\nA\nB\n", 0),
    // 34a.5: the ends
    ("last row", "A\nB\nC\n", "jjJ", "A\nB\nC\n", 2),
    ("first row", "A\nB\nC\n", "K", "A\nB\nC\n", 0),
    // 34a.7: a blank line stays
    ("blank line", "A\n\nB\n", "J", "\nB\nA\n", 1),
];

#[test]
fn j_and_k_move_the_item_and_the_cursor_follows() {
    for (name, todo, keys, want, cursor) in MOVES {
        let dir = workdir(todo);
        let mut model = model_of(dir.path(), alone());
        press(&mut model, keys);
        assert_eq!(todo_of(dir.path()), want, "{name}");
        assert_eq!(model.cursor, cursor, "{name}");
    }
}

// 34a.3: one write is one undo step.
#[test]
fn a_count_move_is_one_write() {
    let dir = workdir("A\nB\nC\nD\n");
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "2J");
    assert_eq!(model.hist.undo.len(), 1);
}

// 34a.5, 34a.10: nothing happens, nothing is written.
#[test]
fn a_move_that_cannot_go_writes_nothing() {
    let dir = workdir("A\nB\n");
    let mut model = model_of(dir.path(), alone());
    let before = modified(dir.path());
    press(&mut model, "K");
    press(&mut model, "jJ");
    assert_eq!(modified(dir.path()), before);
    assert_eq!(model.hist.undo.len(), 0);
    assert!(model.message.is_none());
}

#[test]
fn moving_in_a_list_without_a_file_creates_none() {
    let dir = workdir("");
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "J");
    press(&mut model, "K");
    assert!(!dir.path().join("todo.txt").exists());
}

// 34a.6: items that are not shown keep their place.
#[test]
fn hidden_items_are_not_passed() {
    let dir = workdir("A +x\nH1\nH2\nB +x\nC +x\n");
    let config = Config {
        start_filter: Some("X".to_owned()),
        filters: vec![Filter {
            name: "X".to_owned(),
            query: "+x".to_owned(),
        }],
        ..alone()
    };
    let mut model = model_of(dir.path(), config);
    press(&mut model, "J");
    assert_eq!(todo_of(dir.path()), "H1\nH2\nB +x\nA +x\nC +x\n");
}

// 34a.8: the warning of a claimed item.
#[test]
fn moving_a_claimed_item_warns() {
    let todo = "A started:2026-09-29T0800Z owner:agent1\nB\n";
    let dir = workdir(todo);
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "J");
    assert_eq!(
        todo_of(dir.path()),
        "B\nA started:2026-09-29T0800Z owner:agent1\n"
    );
    let said = model.message.clone().unwrap_or_default();
    assert!(said.contains("is claimed by agent1"), "{said}");
}

// 34a.9: archive rows are not passed and cannot be moved.
#[test]
fn archive_rows_are_not_moved_or_passed() {
    let dir = workdir("A\nB\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = model_of(dir.path(), config(&[]));
    press(&mut model, "jJ");
    assert_eq!(todo_of(dir.path()), "A\nB\n");
    assert!(model.message.is_none());
    press(&mut model, "j");
    assert_eq!(model.rows[model.cursor].source, Source::Done);
    press(&mut model, "J");
    assert_eq!(
        model.message.as_deref(),
        Some(messages::archived_item("Old").as_str())
    );
    assert_eq!(model.tone, Tone::Alert);
}

// 34a.12: a sorted view does not move, and does not write.
#[test]
fn a_sorted_view_refuses_to_move() {
    let dir = workdir("A\n(A) B\n");
    let mut model = model_of(dir.path(), alone());
    let before = modified(dir.path());
    press(&mut model, "sJ");
    assert_eq!(
        model.message.as_deref(),
        Some("sorted by priority — press s for manual order")
    );
    press(&mut model, "K");
    assert_eq!(todo_of(dir.path()), "A\n(A) B\n");
    assert_eq!(modified(dir.path()), before);
}

const CYCLE: [(SortField, &str); 4] = [
    (SortField::Priority, "priority"),
    (SortField::Due, "due"),
    (SortField::Created, "created"),
    (SortField::File, "manual"),
];

// 34a.13: the order of the cycle, and the file is never written.
#[test]
fn s_cycles_the_orders_without_writing() {
    let dir = workdir("A\n(A) B\n");
    let mut model = model_of(dir.path(), alone());
    let before = modified(dir.path());
    assert_eq!(model.sort.name(), "manual");
    for (field, name) in CYCLE {
        press(&mut model, "s");
        assert_eq!(model.sort, field);
        assert_eq!(model.sort.name(), name);
    }
    assert_eq!(modified(dir.path()), before);
}

// 34a.14: the cursor stays on its item.
#[test]
fn the_cursor_stays_on_its_item_when_the_sort_changes() {
    let dir = workdir("A\nB\n(A) C\n");
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "jj");
    assert_eq!(rows_of(&model)[model.cursor], "(A) C");
    press(&mut model, "s");
    assert_eq!(rows_of(&model), ["(A) C", "A", "B"]);
    assert_eq!(rows_of(&model)[model.cursor], "(A) C");
}

// 34a.15: archive rows come after the todo rows in every sort.
#[test]
fn archive_rows_stay_last_in_every_sort() {
    let dir = workdir("B\n(A) A\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 (A) Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = model_of(dir.path(), config(&[]));
    for _ in CYCLE {
        press(&mut model, "s");
        let last = model.rows.last().map(|r| r.source);
        assert_eq!(last, Some(Source::Done), "{}", model.sort.name());
    }
}

// 34a.16, 34a.17: sticky until the cursor moves off it.
#[test]
fn a_completed_item_stays_until_the_cursor_leaves() {
    let dir = workdir("A\nB\nC\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "x");
    assert_eq!(rows_of(&model), ["x 2026-09-29 A*", "B", "C"]);
    let said = model.message.clone().unwrap_or_default();
    assert!(said.ends_with("; hidden by current filters once you move"));
    press(&mut model, "j");
    assert_eq!(rows_of(&model), ["B", "C"]);
    assert!(model.sticky.is_empty());
}

// 34a.18: acting on a sticky row that makes it match ends the stickiness.
#[test]
fn an_action_that_matches_again_ends_the_stickiness() {
    let dir = workdir("A\nB\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "xx");
    assert_eq!(rows_of(&model), ["A", "B"]);
    assert!(model.sticky.is_empty());
    assert_eq!(todo_of(dir.path()), "A\nB\n");
}

// 34a.19: a toggle, a sort, a search or a filter drops sticky items.
#[test]
fn a_change_of_the_view_drops_the_sticky_items() {
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    for (name, keys) in [("sort", "s"), ("toggle", ""), ("search", "/B\r")] {
        let dir = workdir("A\nB\nC\n");
        let mut model = model_of(dir.path(), config(&hidden));
        press(&mut model, "x");
        assert!(!model.sticky.is_empty(), "{name}: not sticky first");
        if name == "toggle" {
            model.shows.flip(Hidden::Later);
            model.refresh();
        } else {
            press(&mut model, keys);
        }
        assert!(model.sticky.is_empty(), "{name}");
        assert!(!rows_of(&model).iter().any(|r| r.ends_with('*')), "{name}");
        assert!(
            !rows_of(&model).iter().any(|r| r.contains("2026")),
            "{name}"
        );
    }
}

#[test]
fn moving_names_inline_keeps_the_sticky_items() {
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let dir = workdir("A @c\nB\nC\n");
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "x");
    let before = rows_of(&model);
    assert!(model.rows.iter().any(|row| row.tags_width > 0));
    press(&mut model, ":set inline_contexts");
    keys(&mut model, "<CR>");
    assert!(model.rows.iter().all(|row| row.tags_width == 0));
    assert_eq!(rows_of(&model), before);
    assert!(!model.sticky.is_empty());
}

// 34a.20: a changed priority under a filter.
#[test]
fn a_priority_that_leaves_the_filter_stays() {
    let dir = workdir("(A) X\nY\n");
    let config = Config {
        start_filter: Some("Top".to_owned()),
        filters: vec![Filter {
            name: "Top".to_owned(),
            query: "pri:A".to_owned(),
        }],
        ..alone()
    };
    let mut model = model_of(dir.path(), config);
    press(&mut model, "gp");
    assert_eq!(rows_of(&model), ["(B) X*"]);
}

// 34a.21: an archived item stays among the todo rows.
#[test]
fn an_archived_item_keeps_its_place_in_manual_order() {
    let dir = workdir("A\nB\nC\n");
    let config = Config {
        auto_archive: true,
        hidden_items: vec![Hidden::Later, Hidden::Archive],
        ..alone()
    };
    let mut model = model_of(dir.path(), config);
    press(&mut model, "jx");
    assert_eq!(rows_of(&model), ["A", "x 2026-09-29 B*", "C"]);
    assert_eq!(todo_of(dir.path()), "A\n\nC\n");
    press(&mut model, "s");
    assert_eq!(rows_of(&model), ["A", "C"]);
}

// 34a.22: each item of one action is sticky.
#[test]
fn several_items_are_sticky_together() {
    let dir = workdir("A\nB\nC\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "2x");
    assert_eq!(rows_of(&model), ["x 2026-09-29 A*", "x 2026-09-29 B*", "C"]);
    press(&mut model, "j");
    assert_eq!(model.rows.len(), 3);
    press(&mut model, "j");
    assert_eq!(rows_of(&model), ["C"]);
}

// 34a.23: deleted items are gone at once.
#[test]
fn a_deleted_item_is_not_sticky() {
    let dir = workdir("A\nB\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "dd");
    assert_eq!(rows_of(&model), ["B"]);
    assert!(model.sticky.is_empty());
    assert_eq!(todo_of(dir.path()), "\nB\n");
}

// 34a.24: an outside change to a sticky item keeps it sticky.
#[test]
fn an_outside_change_keeps_the_item_sticky_with_its_new_text() {
    let dir = workdir("A\nB\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    press(&mut model, "x");
    fs::write(dir.path().join("todo.txt"), "x 2026-09-29 A2\nB\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(rows_of(&model), ["x 2026-09-29 A2*", "B"]);
}

// 34a.24: an outside change hides at once what the view does not match.
#[test]
fn an_outside_change_is_never_sticky() {
    let dir = workdir("A\nB\n");
    let hidden = [Hidden::Later, Hidden::Archive, Hidden::Done];
    let mut model = model_of(dir.path(), config(&hidden));
    fs::write(dir.path().join("todo.txt"), "x 2026-09-29 A\nB\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(rows_of(&model), ["B"]);
}

// 34a.21: a sticky archived item keeps its place across outside writes.
#[test]
fn an_archived_item_keeps_its_place_across_an_outside_write() {
    let dir = workdir("A\nB\nC\n");
    let config = Config {
        auto_archive: true,
        hidden_items: vec![Hidden::Later, Hidden::Archive],
        ..alone()
    };
    let mut model = model_of(dir.path(), config);
    press(&mut model, "x");
    fs::write(dir.path().join("done.txt"), "x 2026-09-29 A2\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    assert_eq!(rows_of(&model), ["x 2026-09-29 A2*", "B", "C"]);
}

// Duplicate lines: the cursor follows the line that was moved, by number.
#[test]
fn the_cursor_follows_the_moved_line_among_twins() {
    let dir = workdir("A\nB\nA\nC\n");
    let mut model = model_of(dir.path(), alone());
    press(&mut model, "2J");
    assert_eq!(todo_of(dir.path()), "B\nA\nA\nC\n");
    assert_eq!(model.cursor, 2);
}
