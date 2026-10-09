use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use lsty_core::config::Config;
use lsty_core::keys::{Named, Stroke};

use crate::model::Model;
use crate::msg::Msg;
use crate::test_util::{alone, deps, press, screen, started, workdir};

const LIST: &str = "Buy\nCall\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn todo_of(dir: &Path) -> String {
    fs::read_to_string(dir.join("todo.txt")).unwrap_or_default()
}

fn named(model: &mut Model, key: Named) {
    model.update(Msg::Key(Stroke::named(key)));
}

fn mode_of(model: &Model) -> String {
    let rows = screen(model, 80, 24);
    rows.last().cloned().unwrap_or_default()
}

// 38.7: nothing is written when nothing changed.
#[test]
fn a_draft_that_was_not_changed_writes_nothing() {
    for key in [Named::Cr, Named::Esc] {
        let dir = workdir(LIST);
        let path = dir.path().join("todo.txt");
        let old = SystemTime::UNIX_EPOCH + Duration::from_hours(438_288);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(old).unwrap();
        let mut model = model_of(dir.path());
        press(&mut model, "a");
        assert!(model.cursor_at().is_some());
        named(&mut model, key);
        assert!(model.cursor_at().is_none(), "{key:?}: still in INSERT");
        let seen = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(seen, old, "{key:?}");
        assert_eq!(todo_of(dir.path()), LIST);
    }
}

// 38.7 with a line that is not UTF-8: the field shows it lossily, and
// nothing is written while it is not changed.
#[test]
fn a_line_that_is_not_utf8_is_left_alone_when_unchanged() {
    let dir = workdir("");
    let raw = b"caf\xe9\n";
    fs::write(dir.path().join("todo.txt"), raw).unwrap();
    let mut model = model_of(dir.path());
    press(&mut model, "i");
    named(&mut model, Named::Cr);
    assert_eq!(fs::read(dir.path().join("todo.txt")).unwrap(), raw);
}

// 38.24 in an empty list: the first item.
#[test]
fn o_creates_the_first_item_of_an_empty_list() {
    let dir = workdir("");
    fs::write(dir.path().join("todo.txt"), "").unwrap();
    let config = Config {
        keep_adding: false,
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "oFirst");
    named(&mut model, Named::Cr);
    assert_eq!(todo_of(dir.path()), "2026-09-29 First\n");
    assert!(model.cursor_at().is_none());
    assert_eq!(model.cursor, 0);
}

#[test]
fn keep_adding_opens_another_new_item_below_the_one_written() {
    for (keys, want) in [
        ("o", "Buy\n2026-09-29 One\n2026-09-29 Two\nCall\n"),
        ("O", "2026-09-29 One\n2026-09-29 Two\nBuy\nCall\n"),
    ] {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        press(&mut model, keys);
        press(&mut model, "One");
        named(&mut model, Named::Cr);
        assert!(model.cursor_at().is_some(), "{keys}: a field is open");
        press(&mut model, "Two");
        named(&mut model, Named::Cr);
        named(&mut model, Named::Cr);
        assert_eq!(todo_of(dir.path()), want, "{keys}");
        assert!(model.cursor_at().is_none(), "{keys}: an empty one ends");
    }
}

#[test]
fn cancel_on_a_new_item_with_nothing_typed_keeps_no_draft() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "oOne");
    named(&mut model, Named::Cr);
    named(&mut model, Named::Esc);
    assert!(model.cursor_at().is_none());
    assert_eq!(model.message, None);
    press(&mut model, "u");
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn keep_adding_leaves_an_edited_item_alone() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "A!");
    named(&mut model, Named::Cr);
    assert_eq!(todo_of(dir.path()), "Buy!\nCall\n");
    assert!(model.cursor_at().is_none());
}

#[test]
fn keep_adding_off_ends_insert_mode_after_a_new_item() {
    let dir = workdir(LIST);
    let config = Config {
        keep_adding: false,
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "oOne");
    named(&mut model, Named::Cr);
    assert_eq!(todo_of(dir.path()), "Buy\n2026-09-29 One\nCall\n");
    assert!(model.cursor_at().is_none());
}

// 38.26: nothing typed, nothing created.
#[test]
fn a_new_item_with_only_spaces_is_not_created() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "o");
    named(&mut model, Named::Cr);
    press(&mut model, "o   ");
    named(&mut model, Named::Esc);
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.message, None);
}

// 38.34 and 38.35: the draft comes back with `u`, once.
#[test]
fn u_brings_a_discarded_draft_back_once() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "izzz");
    model.update(Msg::Key(
        Stroke::char('c').with_mods(lsty_core::keys::Mods::CTRL),
    ));
    assert_eq!(
        model.message.as_deref(),
        Some("edit discarded — u restores")
    );
    press(&mut model, "u");
    assert!(model.cursor_at().is_some());
    assert!(mode_of(&model).contains("INSERT"));
    named(&mut model, Named::Esc);
    assert_eq!(todo_of(dir.path()), "zzzBuy\nCall\n");
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("undid: 1 zzzBuy"));
    assert_eq!(todo_of(dir.path()), LIST);
}

// 38.35: the item is gone when `u` brings the draft back.
#[test]
fn a_kept_draft_of_a_deleted_item_says_so() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "izzz");
    model.update(Msg::Key(
        Stroke::char('c').with_mods(lsty_core::keys::Mods::CTRL),
    ));
    fs::write(dir.path().join("todo.txt"), "Call\n").unwrap();
    let load =
        model
            .update(Msg::Reload)
            .into_iter()
            .find_map(|cmd| match cmd {
                crate::Cmd::Load(load) => Some(load),
                crate::Cmd::Quit
                | crate::Cmd::After { .. }
                | crate::Cmd::Copy(_)
                | crate::Cmd::Edit(_) => None,
            });
    if let Some(load) = load {
        model.update(load.read());
    }
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("the item is gone"));
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("nothing to undo"));
}

// 38.33: the edit actions do not open on an archive row.
#[test]
fn an_archived_item_cannot_be_edited() {
    let dir = workdir("");
    fs::write(dir.path().join("todo.txt"), "Open\n").unwrap();
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n").unwrap();
    let config = Config {
        hidden_items: Vec::new(),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "Gi");
    assert!(model.cursor_at().is_none());
    assert_eq!(
        model.message.as_deref(),
        Some("Old is archived — x restores it")
    );
}

// 38.48: a paste is one event, and breaks in it are spaces.
#[test]
fn a_paste_goes_into_the_field_only() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    model.update(Msg::Paste("ignored".to_owned()));
    assert_eq!(todo_of(dir.path()), LIST);
    press(&mut model, "a");
    model.update(Msg::Paste(" one\ntwo".to_owned()));
    named(&mut model, Named::Cr);
    assert_eq!(todo_of(dir.path()), "Buy one two\nCall\n");
}

// 38.44: a lock timeout keeps the draft open; commit again retries.
#[test]
fn a_busy_lock_keeps_the_draft() {
    use lsty_core::lock::{self, Cancel, Mode, Want};
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "ax");
    let want = Want {
        file: dir.path().join("todo.txt"),
        mode: Mode::Exclusive,
    };
    let mut held =
        lock::acquire(&[want], Duration::from_secs(1), &Cancel::new())
            .unwrap();
    named(&mut model, Named::Cr);
    assert!(model.cursor_at().is_some());
    assert!(model.message.as_deref().unwrap_or("").contains("is busy"));
    held.release().unwrap();
    named(&mut model, Named::Cr);
    assert_eq!(todo_of(dir.path()), "Buyx\nCall\n");
}
