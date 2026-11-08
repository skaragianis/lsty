//! A dim key against what it does: on an empty list, over every action, and
//! when the files have moved on from the list shown.

use std::{
    cell::Cell,
    fs, io,
    path::Path,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use lsty_core::{
    config::Config,
    keys::{Action, Mode},
};
use lsty_scripttest::holds;
use tempfile::TempDir;

use crate::{
    deps::Deps,
    focus::Focus,
    fuzzing::hints_property,
    hints::dim,
    messages::Tone,
    model::Model,
    msg::{Cmd, Msg},
    press::Answer,
    test_util::{
        Board, alone, cursor_text, deps, files, held_lock, hint_is, keys,
        reload, same_as, screen, started, workdir,
    },
};

const LOOK_AGAIN: &str = "the list changed — look again";

const BUSY: &str =
    "todo.txt is busy (locked by another writer) — nothing changed, try again";

/// `stop` has no default key.
const STOP_BOUND: &str = "[normal]\nstop = [\"<F9>\"]\n\
    [visual]\nstop = [\"<F9>\"]\n";

const BLOCKED: &str = "Blocker id:b\nA dep:b\nB dep:b\n";
const FREED: &str = "Blocker id:b\nA\nB\n";
const NO_BLOCKER: &str = "A dep:b\nB dep:b\n";
const CLOSED: &str = "x 2026-09-01 A\nx 2026-09-01 B\n";
const REOPENED: &str = "A\nB\n";

/// A list, the file that makes a key write over its first task, and the file
/// that leaves the key refused.
type Files = (&'static str, &'static str, &'static str);

const BLOCKS: Files =
    (BLOCKED, NO_BLOCKER, "New\nBlocker id:b\nA dep:b\nB dep:b\n");
const CLOSES: Files =
    (CLOSED, REOPENED, "New\nx 2026-09-01 A\nx 2026-09-01 B\n");
const CLAIMS: Files = (
    REOPENED,
    "A started:2026-09-29T0900Z owner:agent1\n\
     B started:2026-09-29T0900Z owner:agent1\n",
    "New\nA\nB\n",
);

/// A dim key with the keys before it, whichever way it is refused.
type Key = (Action, &'static str, &'static str, Files);

const KEYS: [Key; 7] = [
    (Action::ToggleComplete, "x", "j", BLOCKS),
    (Action::ToggleStart, "t", "j", BLOCKS),
    (Action::ToggleStart, "t", "gg", CLOSES),
    (Action::Wontdo, "T", "gg", CLOSES),
    (Action::CyclePriority, "-", "gg", CLOSES),
    (Action::CyclePriorityBack, "+", "gg", CLOSES),
    (Action::Stop, "<F9>", "gg", CLAIMS),
];

/// The keys of a selection, over an archived task, that no other row presses.
const OVER_ARCHIVED: [&str; 5] = ["J", "K", "gd", "gt", "gT"];

/// A folder with `todo` and the key bindings `toml`.
fn bound(todo: &str, toml: &str) -> TempDir {
    let dir = workdir(todo);
    fs::write(dir.path().join("keys.toml"), toml)
        .unwrap_or_else(|e| panic!("{e}"));
    dir
}

/// A list of `shown` after the keys `first`.
fn opened(shown: &str, first: &str) -> (TempDir, Model) {
    let dir = bound(shown, STOP_BOUND);
    let mut model = started(deps(dir.path(), alone()));
    keys(&mut model, first);
    (dir, model)
}

/// A list of `shown` with an archived task as its last row.
fn with_archived(shown: &str) -> (TempDir, Model) {
    let dir = bound(shown, STOP_BOUND);
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_tasks: Vec::new(),
        ..alone()
    };
    let model = started(deps(dir.path(), config));
    (dir, model)
}

/// Replaces `from`, the first lines of the file, behind the list's back.
fn outside(dir: &Path, from: &str, to: &str) {
    let path = dir.join("todo.txt");
    let now = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(now.contains(from), "{now:?} has no {from:?}");
    fs::write(&path, now.replace(from, to)).unwrap_or_else(|e| panic!("{e}"));
}

/// One press on a list whose lines `from` the outside made `to`, against
/// the same press on a list that has read the same files: `writes` tells
/// which of the two outcomes it is. The first asks to look again and writes
/// nothing, and the next press does what the up to date one did. The second
/// is the same press as the up to date one.
fn stale_against_fresh(
    (action, key, first): (Action, &str, &str),
    (list, from, to): (&str, &str, &str),
    writes: bool,
) {
    let name = format!("{key} after {first} on {list:?} to {to:?}");
    let (stale_dir, mut stale) = opened(list, first);
    let (fresh_dir, mut fresh) = opened(list, first);
    assert!(dim(stale.answers()(action)), "{name}: dim to start with");
    outside(stale_dir.path(), from, to);
    outside(fresh_dir.path(), from, to);
    let moved_on = files(stale_dir.path());
    reload(&mut fresh);
    let hint = fresh.answers()(action);
    keys(&mut fresh, key);
    assert_eq!(files(fresh_dir.path()) != moved_on, writes, "{name}");
    keys(&mut stale, key);
    assert_eq!(files(stale_dir.path()), moved_on, "{name}: wrote");
    if writes {
        looks_again(&stale, action, Some(hint), &name);
        keys(&mut stale, key);
    }
    same_as(
        (stale_dir.path(), &stale),
        (fresh_dir.path(), &fresh),
        &name,
    );
}

/// The list said to look again; the hint of `action` now is as `hint_is`.
fn looks_again(
    model: &Model,
    action: Action,
    fresh: Option<Answer>,
    name: &str,
) {
    assert_eq!(model.message.as_deref(), Some(LOOK_AGAIN), "{name}");
    hint_is(model, action, fresh, name);
}

/// A dim key does nothing, whichever it is: after it the selection is as it
/// was, and `next` repeats `real` just as if the key had not been pressed.
#[test]
fn a_dim_press_leaves_the_selection_and_the_repeat_alone() {
    let check = |list: &str, real: &str, (first, key): (&str, &str), next| {
        let name = format!("{real}, {first}, {key} on {list:?}");
        let (dir, mut model) = with_archived(list);
        let (twin_dir, mut twin) = with_archived(list);
        keys(&mut model, real);
        keys(&mut twin, real);
        keys(&mut model, first);
        let (mode, selected) = (model.key_mode(), model.selected_rows());
        let before = files(dir.path());
        keys(&mut model, key);
        assert_eq!(files(dir.path()), before, "{name}");
        assert_eq!(model.tone, Tone::Alert, "{name}");
        assert_eq!(model.key_mode(), mode, "{name}");
        assert_eq!(model.selected_rows(), selected, "{name}");
        keys(&mut model, next);
        keys(&mut twin, next);
        assert_ne!(files(dir.path()), before, "{name}: repeated nothing");
        assert_eq!(files(dir.path()), files(twin_dir.path()), "{name}");
    };
    for (_, key, first, (shown, ..)) in KEYS {
        let selection = format!("gg{first}Vj");
        let list = format!("{shown}Mine\n");
        check(&list, "Gk-", (&selection, key), "<Esc>Gk.");
    }
    for key in OVER_ARCHIVED {
        check("A\nMine\n", "gg-", ("jVj", key), "<Esc>gg.");
    }
    for real in ["gg-", "ggVjJ<Esc>", "gggd2030-01-01<CR>"] {
        check("A\nB\nC\nD\n", real, ("G", "3."), "ggj.");
    }
}

/// Each way of pressing a key: alone, over a selection, with a count.
#[test]
fn a_dim_press_on_a_list_that_moved_on_never_writes() {
    for (action, key, first, (shown, freed, other)) in KEYS {
        let selected = format!("{first}Vj");
        let counted = format!("2{key}");
        for way in [(key, first), (key, &selected), (&counted, first)] {
            // A selection refuses unless its lines only shifted.
            let writes = way.1 != selected || freed == NO_BLOCKER;
            for (to, writes) in [(freed, writes), (other, false)] {
                stale_against_fresh(
                    (action, way.0, way.1),
                    (shown, shown, to),
                    writes,
                );
            }
        }
    }
}

/// `.` stores the key from a press on the last task, which the outside
/// leaves alone. A selection has no `.`.
#[test]
fn a_dim_repeat_on_a_list_that_moved_on_never_writes() {
    for (action, key, first, (shown, freed, other)) in KEYS {
        let mine = if action == Action::Stop {
            "Mine started:2026-09-29T0800Z\n"
        } else {
            "Mine\n"
        };
        let before = format!("G{key}gg{first}");
        let list = format!("{shown}{mine}");
        for press in [".", "2."] {
            for (to, writes) in [(freed, true), (other, false)] {
                stale_against_fresh(
                    (Action::Repeat, press, &before),
                    (&list, shown, to),
                    writes,
                );
            }
        }
    }
}

/// Where the cursor ends up, and that the next press is on the task it is
/// on. A selection stays through the look again and follows its lines. Lines
/// the outside changed drop it, as any reload does.
#[test]
fn the_press_after_a_look_again_is_on_the_task_under_the_cursor() {
    let rows = [
        // It changed: the row stays, on the task now there.
        (
            "j",
            "New\nBlocker id:b\nA\nB dep:b\n",
            "Blocker id:b",
            "New\nx 2026-09-29 Blocker id:b\nA\nB dep:b\n",
        ),
        // It stayed where it was.
        ("j", FREED, "A", "Blocker id:b\nx 2026-09-29 A\nB\n"),
        // It went away: the next task has the cursor.
        (
            "j",
            "Blocker id:b\nB\n",
            "B",
            "Blocker id:b\nx 2026-09-29 B\n",
        ),
        // The lines of the selection changed: it ended, the cursor stayed.
        ("jVj", FREED, "B", "Blocker id:b\nA\nx 2026-09-29 B\n"),
        // They stayed, and the outside took away what blocked them.
        (
            "jVj",
            NO_BLOCKER,
            "B dep:b",
            "x 2026-09-29 A dep:b\nx 2026-09-29 B dep:b\n",
        ),
        // All that was selected went away.
        (
            "jVj",
            "Blocker id:b\n",
            "Blocker id:b",
            "x 2026-09-29 Blocker id:b\n",
        ),
    ];
    for (first, to, under, want) in rows {
        let name = format!("{first} to {to:?}");
        let (dir, mut model) = opened(BLOCKED, first);
        outside(dir.path(), BLOCKED, to);
        let before = files(dir.path());
        keys(&mut model, "x");
        looks_again(&model, Action::ToggleComplete, None, &name);
        assert_eq!(files(dir.path()), before, "{name}");
        assert_eq!(cursor_text(&model), under, "{name}");
        keys(&mut model, "x");
        assert_eq!(files(dir.path())[0], want.as_bytes(), "{name}");
    }
}

/// A count is for the press it was typed for, and the next one reads its
/// own, as the status line and the hints say. When the count's other tasks
/// are still refused, so is the press.
#[test]
fn a_count_does_not_outlive_a_look_again() {
    for (second, want) in [("x", 1), ("2x", 2)] {
        let (dir, mut model) = opened(BLOCKED, "j");
        outside(dir.path(), BLOCKED, FREED);
        keys(&mut model, "2x");
        looks_again(&model, Action::ToggleComplete, None, second);
        assert_eq!(model.typed_count(), 0);
        keys(&mut model, second);
        let todo = files(dir.path())[0].clone();
        let done =
            todo.split(|&b| b == b'\n').filter(|l| l.starts_with(b"x "));
        assert_eq!(done.count(), want);
    }

    let (dir, mut model) = opened(BLOCKED, "j");
    outside(dir.path(), BLOCKED, "Blocker id:b\nA\nB dep:b\n");
    let before = files(dir.path());
    keys(&mut model, "2x");
    assert_eq!(
        model.message.as_deref(),
        Some("task 3 is blocked by 1 (Blocker)")
    );
    assert_eq!(files(dir.path()), before);
}

/// The task the cursor was on went away and the next one is refused too:
/// the reason is about the task now shown.
#[test]
fn a_refused_press_names_the_task_now_under_the_cursor() {
    let rows = [
        (
            BLOCKED,
            "j",
            "Blocker id:b\nB dep:b\n",
            "x",
            "B dep:b",
            "task 2 is blocked by 1 (Blocker)",
        ),
        (
            CLOSED,
            "gg",
            "x 2026-09-01 A\nB\n",
            "2t",
            "x 2026-09-01 A",
            "some are done — nothing changed",
        ),
    ];
    for (from, first, to, key, under, says) in rows {
        let (dir, mut model) = opened(from, first);
        outside(dir.path(), from, to);
        keys(&mut model, key);
        assert_eq!(cursor_text(&model), under);
        assert_eq!(model.message.as_deref(), Some(says));
    }
}

/// Only the done file moved: the archived task under the cursor is gone, and
/// the key is lit over the task that took its place.
#[test]
fn a_press_after_a_change_to_the_done_file_alone_looks_again() {
    let dir = workdir("A\n");
    let done = dir.path().join("done.txt");
    fs::write(&done, "x 2026-09-01 Old\n").unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_tasks: Vec::new(),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    keys(&mut model, "G");
    assert!(dim(model.answers()(Action::CyclePriority)));
    fs::write(&done, "").unwrap_or_else(|e| panic!("{e}"));
    keys(&mut model, "-");
    looks_again(&model, Action::CyclePriority, None, "done file");
    assert_eq!(cursor_text(&model), "A");
}

/// Tasks came to an empty list: a key that said nothing says so, once.
#[test]
fn a_press_on_an_empty_list_that_has_tasks_now_looks_again() {
    let rows: [(_, _, &[u8]); 2] = [
        ("x", Action::ToggleComplete, b"x 2026-09-29 New\n"),
        ("dd", Action::Delete, b"\n"),
    ];
    for (key, action, then) in rows {
        let (dir, mut model) = opened("", "");
        fs::write(dir.path().join("todo.txt"), "New\n")
            .unwrap_or_else(|e| panic!("{e}"));
        keys(&mut model, key);
        looks_again(&model, action, None, key);
        assert_eq!(files(dir.path())[0], b"New\n");
        keys(&mut model, key);
        assert_eq!(files(dir.path())[0], then, "{key}");
    }
}

/// The file changed and the list did not: there is nothing to look at.
#[test]
fn a_press_on_a_list_that_stays_empty_says_nothing() {
    for (shown, first, to) in
        [("A\n", "/zzz<CR>", "A\nB\n"), ("", "", "A t:2099-01-01\n")]
    {
        let (dir, mut model) = opened(shown, first);
        fs::write(dir.path().join("todo.txt"), to)
            .unwrap_or_else(|e| panic!("{e}"));
        keys(&mut model, "x");
        assert!(model.rows.is_empty(), "{to}");
        assert_eq!(model.message, None, "{to}");
        assert_eq!(files(dir.path())[0], to.as_bytes());
    }
}

/// Emptied from outside, however it came about: the list is empty, the key
/// says why, and nothing is written.
#[test]
fn a_press_on_a_list_emptied_outside_looks_again() {
    let (later, gone) = ("A t:2030-01-01\nB t:2030-01-01\n", "gone");
    for first in ["j", "jVj"] {
        for to in ["", later, gone] {
            let (dir, mut model) = opened(BLOCKED, first);
            let todo = dir.path().join("todo.txt");
            if to == gone {
                fs::remove_file(&todo).unwrap_or_else(|e| panic!("{e}"));
            } else {
                fs::write(&todo, to).unwrap_or_else(|e| panic!("{e}"));
            }
            let before = files(dir.path());
            keys(&mut model, "x");
            let name = format!("{first} to {to:?}");
            assert!(model.rows.is_empty(), "{name}");
            assert_eq!(model.message.as_deref(), Some(LOOK_AGAIN), "{name}");
            assert_eq!(model.key_mode(), Mode::Normal, "{name}");
            assert_eq!(files(dir.path()), before, "{name}");
        }
    }
}

/// A read that fails ends the press with the message the core would give, the
/// list as it was. Once the lock is free, the next press reads.
#[test]
fn a_dim_press_that_cannot_read_says_the_file_is_busy() {
    let dir = workdir(BLOCKED);
    let config = Config {
        lock_timeout: Duration::from_millis(30),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    keys(&mut model, "j");
    outside(dir.path(), BLOCKED, FREED);
    let mut held = held_lock(dir.path());
    keys(&mut model, "x");
    assert_eq!(model.message.as_deref(), Some(BUSY));
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(cursor_text(&model), "A dep:b", "the list as it was");
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    assert_eq!(files(dir.path())[0], FREED.as_bytes());
    keys(&mut model, "x");
    looks_again(&model, Action::ToggleComplete, None, "lock let go");
    assert_eq!(files(dir.path())[0], FREED.as_bytes());
}

/// A key that cannot write does not wait for the lock, and what it says is
/// true of the list still shown. Once the lock is free, the next press reads.
#[test]
fn a_refused_press_does_not_wait_for_a_held_lock() {
    let done = Some("A is done — x reopens it");
    for (shown, first, to, key, says) in [
        (CLOSED, "gg", REOPENED, "T", done),
        ("", "", "New\n", "x", None),
        ("", "", "New\n", "dd", None),
    ] {
        let dir = workdir(shown);
        let config = Config {
            lock_timeout: Duration::from_secs(10),
            ..alone()
        };
        let mut model = started(deps(dir.path(), config));
        keys(&mut model, first);
        let rows = model.rows.len();
        fs::write(dir.path().join("todo.txt"), to)
            .unwrap_or_else(|e| panic!("{e}"));
        let mut held = held_lock(dir.path());
        let start = Instant::now();
        keys(&mut model, key);
        assert!(start.elapsed() < Duration::from_secs(5), "{key}: waited");
        assert_eq!(model.message.as_deref(), says, "{key}");
        assert_eq!(model.rows.len(), rows, "{key}: the list as it was");
        held.release().unwrap_or_else(|e| panic!("{}", e.message()));
        keys(&mut model, key);
        assert_eq!(model.message.as_deref(), Some(LOOK_AGAIN), "{key}");
        assert_eq!(files(dir.path())[0], to.as_bytes(), "{key}");
    }
}

/// The one who holds the lock frees the task and lets go, after the read gave
/// up or while it waits. Nothing follows the read that could still write.
#[test]
fn a_dim_press_that_cannot_read_writes_nothing_when_the_lock_comes_free() {
    for millis in [450, 100] {
        let dir = workdir(BLOCKED);
        let config = Config {
            lock_timeout: Duration::from_millis(300),
            ..alone()
        };
        let mut model = started(deps(dir.path(), config));
        keys(&mut model, "j");
        let (holding, held) = std::sync::mpsc::channel();
        let path = dir.path().to_owned();
        let holder = thread::spawn(move || {
            let mut lock = held_lock(&path);
            fs::write(path.join("todo.txt"), FREED)
                .unwrap_or_else(|e| panic!("{e}"));
            holding.send(()).unwrap_or_else(|e| panic!("{e}"));
            thread::sleep(Duration::from_millis(millis));
            lock.release().unwrap_or_else(|e| panic!("{}", e.message()));
        });
        held.recv().unwrap_or_else(|e| panic!("{e}"));
        keys(&mut model, "x");
        holder
            .join()
            .unwrap_or_else(|_| panic!("the holder panicked"));
        assert_eq!(files(dir.path())[0], FREED.as_bytes(), "{millis}");
    }
}

/// All a press can change, but for what it says, taken off before this.
type Seen = ([Vec<u8>; 2], Vec<String>, Focus, Mode, String, bool, usize);

fn seen(dir: &Path, model: &Model, size: (u16, u16), opened: usize) -> Seen {
    let field = model.edit.is_some() || model.line.is_some();
    (
        files(dir),
        screen(model, size.0, size.1),
        model.focus,
        model.key_mode(),
        format!("{:?}", model.clip),
        field,
        opened,
    )
}

/// What a walk presses every action over: the config, the width and the
/// list with the keys before it.
type Place = (Config, u16, &'static str, &'static str);

const TAGGED: &str = "A +p @c\nB +q @d\n";

fn with_panels() -> Config {
    Config {
        show_sidebar: true,
        show_detail: true,
        ..alone()
    }
}

fn no_sidebar() -> Config {
    Config {
        show_detail: true,
        ..alone()
    }
}

fn the_empty_lists() -> Vec<Place> {
    let mut places = Vec::new();
    for (config, width) in [
        (with_panels(), 120),
        (with_panels(), 60),
        (no_sidebar(), 120),
        (no_sidebar(), 60),
        (alone(), 120),
    ] {
        places.push((config.clone(), width, "", ""));
        places.push((config, width, TAGGED, "/zzz<CR>"));
    }
    places
}

fn the_panels() -> Vec<Place> {
    let at = |first| (with_panels(), 120, TAGGED, first);
    vec![
        at("/zzz<CR><C-w>3"),
        at("/zzz<CR><C-w>4"),
        at("/zzz<CR><C-w>5"),
        at("<C-w>2"),
        at("<C-w>3"),
        at("<C-w>4"),
        at("<C-w>5"),
    ]
}

/// A model of `place` with `action` bound to `<F9>` in every mode.
fn walked(place: &Place, action: Action) -> (TempDir, Model, Rc<Cell<usize>>) {
    let (config, width, todo, first) = place.clone();
    let toml = Mode::ALL.iter().fold(String::new(), |toml, mode| {
        format!("{toml}[{}]\n{action} = [\"<F9>\"]\n", mode.as_str())
    });
    let dir = bound(todo, &toml);
    let opened = Rc::new(Cell::new(0));
    let runs = Rc::clone(&opened);
    let deps = Deps {
        clipboard: Box::new(Board::default()),
        opener: Box::new(move |_| {
            runs.set(runs.get() + 1);
            Ok(())
        }),
        ..deps(dir.path(), config)
    };
    let mut model = started(deps);
    model.update(Msg::Resize(width, 24));
    keys(&mut model, first);
    (dir, model, opened)
}

/// Each action as its hint would show over `place`, pressed, with what it
/// did. `None` for an action that has no key where the place is.
fn walk(place: &Place, action: Action) -> Option<(bool, bool, bool)> {
    let (dir, mut model, opened) = walked(place, action);
    let mode = model.key_mode();
    if !model.deps.keys.keys(mode, action).contains(&"<F9>") {
        return None;
    }
    let size = (place.1, 24);
    let is_dim = dim(model.answers()(action));
    model.message = None;
    let before = seen(dir.path(), &model, size, opened.get());
    let cmds = keys(&mut model, "<F9>");
    let said = model.message.take().is_some();
    let after = seen(dir.path(), &model, size, opened.get());
    let ran = cmds.iter().any(|cmd| {
        !matches!(
            cmd,
            Cmd::After {
                msg: Msg::Expire(_),
                ..
            }
        )
    });
    Some((is_dim, before != after || ran, said))
}

/// Lit on an empty list, and pressing shows nothing: `gw` changes what the
/// next task looks like, and `<Esc>` has a hint only while it clears a search.
const LIT_AND_UNSEEN: [Action; 2] = [Action::ToggleWrap, Action::ClearSearch];

/// Lit on an empty list, and pressing only says why not.
const SAYS_WHY_NOT: [Action; 3] =
    [Action::PasteBelow, Action::PasteAbove, Action::ClearFilters];

#[test]
fn on_an_empty_list_a_key_is_dim_when_it_does_nothing() {
    let mut pressed = 0;
    let mut wrong = Vec::new();
    for place in the_empty_lists() {
        for &action in Action::ALL {
            let Some((is_dim, changed, said)) = walk(&place, action) else {
                continue;
            };
            pressed += 1;
            let at = (action.to_string(), place.1, place.2, place.3);
            if is_dim && changed {
                wrong.push(format!("dim and acts: {at:?}"));
            }
            let unseen = LIT_AND_UNSEEN.contains(&action);
            if is_dim && unseen {
                wrong.push(format!("dim and acts unseen: {at:?}"));
            }
            let says = said && SAYS_WHY_NOT.contains(&action);
            if !is_dim && !changed && !says && !unseen {
                wrong.push(format!("lit and does nothing: {at:?} {said}"));
            }
        }
    }
    assert!(pressed > 600, "{pressed}");
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn in_a_panel_no_key_that_works_is_dim() {
    let mut wrong = Vec::new();
    for place in the_panels() {
        for &action in Action::ALL {
            if let Some((true, true, _)) = walk(&place, action) {
                wrong.push(format!("{action}: {:?}", place.3));
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// A run of keys, the file sometimes changed behind the list, each key
/// against its hint, over the seeds of the fuzz target.
#[test]
fn every_key_of_a_seed_does_what_its_hint_says() -> io::Result<()> {
    holds("hints", 20, hints_property)
}
