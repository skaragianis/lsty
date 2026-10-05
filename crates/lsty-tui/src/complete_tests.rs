use std::fs;
use std::path::Path;

use lsty_core::errs;
use lsty_core::keys::{Named, Stroke};
use lsty_core::ops::IdSource;

use crate::complete::POPUP_ROWS;
use crate::field::Field;
use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::test_util::{alone, deps, press, started, workdir};

const LIST: &str = "Write parser +spec01 id:parser\n\
    Add CLI ls +spec01 dep:parser\n\
    Write docs +spec01\n\
    Write docs +spec02 due:2026-10-02\n\
    Call client\n";

const DONE: &str = "x 2026-09-01 Fix tap +home @phone\n\
    x 2026-09-02 Send report +work @email\n\
    x 2026-09-03 Pitch +work.acme\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

/// A folder with the list and the archive of the specification.
fn listed() -> tempfile::TempDir {
    let dir = workdir(LIST);
    fs::write(dir.path().join("done.txt"), DONE).unwrap();
    dir
}

fn named(model: &mut Model, key: Named) {
    model.update(Msg::Key(Stroke::named(key)));
}

/// The labels of the popup; none when it is closed.
fn labels(model: &Model) -> Vec<String> {
    model
        .edit
        .as_ref()
        .and_then(|d| d.comp.popup.as_ref())
        .map(|p| p.cands.iter().map(|c| c.label.clone()).collect())
        .unwrap_or_default()
}

fn text_of(model: &Model) -> String {
    model
        .edit
        .as_ref()
        .map(|d| d.field.text())
        .unwrap_or_default()
}

/// The model with the field open on the last item and `typed` in it.
fn typing(dir: &Path, typed: &str) -> Model {
    let mut model = model_of(dir);
    press(&mut model, "Ga");
    press(&mut model, typed);
    model
}

#[test]
fn a_word_is_the_text_from_the_last_space_to_the_cursor() {
    let at = |text: &str, cursor: usize| {
        let mut field = Field::new(text);
        field.cursor = cursor;
        field.word_before()
    };
    assert_eq!(at("Call +wo", 8), Some((5, "+wo".to_owned())));
    assert_eq!(at("+wo", 3), Some((0, "+wo".to_owned())));
    assert_eq!(at("Call ", 5), None);
    assert_eq!(at("", 0), None);
    assert_eq!(at("client", 3), None);
    assert_eq!(at("client", 6), Some((0, "client".to_owned())));
}

#[test]
fn a_token_is_swapped_wherever_it_stands() {
    let mut field = Field::new("a dep:x b dep:x dep:xy");
    field.cursor = 20;
    field.swap_token("dep:x", "dep:zzz");
    assert_eq!(field.text(), "a dep:zzz b dep:zzz dep:xy");
    assert_eq!(field.cursor, 24);
}

// 39.1, 39.2, 39.6, 39.13: names of both files, by prefix.
#[test]
fn names_come_from_both_files_in_byte_order() {
    let dir = listed();
    let model = typing(dir.path(), " +");
    let all = ["+home", "+spec01", "+spec02", "+work", "+work.acme"];
    assert_eq!(labels(&model), all);
    let model = typing(dir.path(), " +W");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    let model = typing(dir.path(), " @");
    assert_eq!(labels(&model), ["@email", "@phone"]);
    let model = typing(dir.path(), " +zzz");
    assert_eq!(labels(&model), Vec::<String>::new());
}

// 39.14 to 39.16: keys need two characters and no colon.
#[test]
fn keys_are_offered_for_two_characters_without_a_colon() {
    let dir = listed();
    let rows: [(&str, &[&str]); 8] = [
        (" du", &["due:"]),
        (" DU", &["due:"]),
        (" d", &[]),
        (" de", &["dep:"]),
        (" t:", &[]),
        (" st", &["started:"]),
        (" ow", &["owner:"]),
        (" zz", &[]),
    ];
    for (typed, want) in rows {
        let model = typing(dir.path(), typed);
        assert_eq!(labels(&model), want, "{typed:?}");
    }
}

// 39.9 to 39.11: a closed popup stays closed until the word changes.
#[test]
fn a_closed_popup_opens_again_when_the_word_changes() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    named(&mut model, Named::Esc);
    assert_eq!(labels(&model), Vec::<String>::new());
    assert!(model.edit.is_some());
    press(&mut model, "o");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
}

// 39.4: accepting keeps INSERT mode and adds no space.
#[test]
fn accepting_puts_the_candidate_in_place_of_the_word() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    named(&mut model, Named::Tab);
    assert_eq!(text_of(&model), "Call client +work");
    assert_eq!(labels(&model), Vec::<String>::new());
    assert!(model.edit.is_some());
}

// 39.8: the selection wraps.
#[test]
fn the_selection_wraps_round_the_list() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    let at = |m: &Model| {
        m.edit
            .as_ref()
            .and_then(|d| d.comp.popup.as_ref())
            .map(|p| p.at)
    };
    named(&mut model, Named::Down);
    assert_eq!(at(&model), Some(1));
    named(&mut model, Named::Down);
    assert_eq!(at(&model), Some(0));
    named(&mut model, Named::Up);
    assert_eq!(at(&model), Some(1));
}

// 39.37: eight rows are shown, and the window follows the selection.
#[test]
fn at_most_eight_candidates_are_shown_and_the_window_scrolls() {
    let todo = (0..20)
        .fold(String::new(), |all, i| all + &format!("Item +p{i:02}\n"));
    let dir = workdir(&todo);
    let mut model = typing(dir.path(), " +p");
    let shown = |m: &Model, rows: usize| {
        let popup = m.edit.as_ref().and_then(|d| d.comp.popup.as_ref());
        popup.map(|p| {
            let (top, cands) = p.shown(rows);
            (top, cands.len())
        })
    };
    assert_eq!(labels(&model).len(), 20);
    assert_eq!(shown(&model, POPUP_ROWS), Some((0, POPUP_ROWS)));
    for _ in 0..POPUP_ROWS {
        named(&mut model, Named::Down);
    }
    assert_eq!(shown(&model, POPUP_ROWS), Some((1, POPUP_ROWS)));
    assert_eq!(shown(&model, 3), Some((6, 3)));
}

// 39.31 to 39.36: files of the folder of the list.
#[test]
fn files_are_the_entries_of_the_folder_the_path_names() {
    let dir = listed();
    let root = dir.path();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/a.md"), "").unwrap();
    fs::write(root.join("docs/b c.pdf"), "").unwrap();
    fs::write(root.join("notes.md"), "").unwrap();
    fs::write(root.join(".hidden"), "").unwrap();
    let rows: [(&str, &[&str]); 7] = [
        (" file:", &["docs/", "done.txt", "notes.md", "todo.txt"]),
        (" file:d", &["docs/", "done.txt"]),
        (" file:D", &[]),
        (" file:.", &[".hidden"]),
        (" file:docs/", &["a.md", "b c.pdf"]),
        (" file:nope/", &[]),
        (" file:~/docs/a", &["a.md"]),
    ];
    for (typed, want) in rows {
        let model = typing(root, typed);
        assert_eq!(labels(&model), want, "{typed:?}");
    }
}

// 39.33: the path is written encoded, and a folder opens its content.
#[test]
fn a_file_is_inserted_encoded_and_a_folder_opens_again() {
    let dir = listed();
    let root = dir.path();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/b c.pdf"), "").unwrap();
    let mut model = typing(root, " file:d");
    named(&mut model, Named::Tab);
    assert_eq!(text_of(&model), "Call client file:docs/");
    assert_eq!(labels(&model), ["b c.pdf"]);
    named(&mut model, Named::Tab);
    assert_eq!(text_of(&model), "Call client file:docs/b%20c.pdf");
}

// 39.19 to 39.21: open items by text, smartcase, in file order.
#[test]
fn prerequisites_are_the_open_items_with_a_matching_text() {
    let dir = listed();
    let rows: [(&str, &[&str]); 3] = [
        (
            " dep:",
            &[
                "1  Write parser  +spec01",
                "2  Add CLI ls  +spec01",
                "3  Write docs  +spec01",
                "4  Write docs  +spec02  fri",
            ],
        ),
        (
            " dep:docs",
            &["3  Write docs  +spec01", "4  Write docs  +spec02  fri"],
        ),
        (" dep:Docs", &[]),
    ];
    for (typed, want) in rows {
        let model = typing(dir.path(), typed);
        assert_eq!(labels(&model), want, "{typed:?}");
    }
}

// 39.28, 39.29: not the item itself, what it waits for, or a cycle.
#[test]
fn a_prerequisite_may_not_close_a_cycle() {
    let dir = listed();
    let mut model = model_of(dir.path());
    press(&mut model, "gg");
    press(&mut model, "a dep:");
    let want = [
        "3  Write docs  +spec01",
        "4  Write docs  +spec02  fri",
        "5  Call client",
    ];
    assert_eq!(labels(&model), want);
    let mut model = model_of(dir.path());
    press(&mut model, "jja");
    press(&mut model, "x dep:");
    assert_eq!(labels(&model).len(), 4);
    let mut model = model_of(dir.path());
    press(&mut model, "ja");
    press(&mut model, " dep:");
    assert_eq!(labels(&model).len(), 3);
    assert!(!labels(&model).iter().any(|l| l.starts_with("1  ")));
}

// 39.23, 39.24: an item without an id gets one when the line is
// written, in the same write.
#[test]
fn a_chosen_item_gets_its_id_on_commit_only() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    named(&mut model, Named::Tab);
    let text = text_of(&model);
    let id = text.rsplit("dep:").next().unwrap().to_owned();
    assert_eq!(id.len(), 4, "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("todo.txt")).unwrap(),
        LIST
    );
    named(&mut model, Named::Cr);
    let lines: Vec<String> = fs::read_to_string(dir.path().join("todo.txt"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        lines.get(2).map(String::as_str),
        Some(format!("Write docs +spec01 id:{id}").as_str())
    );
    assert_eq!(
        lines.get(4).map(String::as_str),
        Some(format!("Call client dep:{id}").as_str())
    );
}

// 39.27: no id is written when the token is gone from the text.
#[test]
fn a_token_that_is_deleted_gives_no_id() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    named(&mut model, Named::Tab);
    press(&mut model, " more");
    for _ in 0..16 {
        named(&mut model, Named::Bs);
    }
    named(&mut model, Named::Cr);
    let todo = fs::read_to_string(dir.path().join("todo.txt")).unwrap();
    assert!(todo.contains("\nWrite docs +spec01\n"), "{todo}");
}

// 39.23: a chosen item is offered with its new id, and once the text
// waits for it, not again.
#[test]
fn a_prerequisite_with_a_new_id_is_offered_with_it() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    named(&mut model, Named::Tab);
    press(&mut model, " dep:docs");
    assert_eq!(labels(&model), ["4  Write docs  +spec02  fri"]);
}

/// How long the keys of a word take over a list this long.
fn keys_over(items: usize, typed: &str) -> std::time::Duration {
    let todo = (0..items).fold(String::new(), |all, i| {
        all + &format!("Task number {i} +p{} @c{}\n", i % 50, i % 7)
    });
    let dir = workdir(&todo);
    let mut model = typing(dir.path(), " ");
    let start = std::time::Instant::now();
    press(&mut model, typed);
    start.elapsed()
}

// The sources are collected once for a reading, so each key after the
// first only filters.
#[test]
fn keys_over_a_long_list_do_not_collect_again() {
    let first = keys_over(10_000, "dep:t");
    let more = keys_over(10_000, "dep:task number 1");
    assert!(first < std::time::Duration::from_secs(10), "{first:?}");
    assert!(more < std::time::Duration::from_secs(10), "{more:?}");
}

/// A reading of the files, done at once.
fn reload(model: &mut Model) {
    for cmd in model.update(Msg::Reload) {
        if let Cmd::Load(load) = cmd {
            model.update(load.read());
        }
    }
}

fn selected(model: &Model) -> Option<usize> {
    model
        .edit
        .as_ref()
        .and_then(|d| d.comp.popup.as_ref())
        .map(|p| p.at)
}

// A reading made while the popup is open keeps the selection on the
// same candidate.
#[test]
fn a_new_reading_keeps_the_selected_candidate() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    named(&mut model, Named::Down);
    let more = format!("{LIST}Other +wab\n");
    fs::write(dir.path().join("todo.txt"), more).unwrap();
    reload(&mut model);
    assert_eq!(labels(&model), ["+wab", "+work", "+work.acme"]);
    assert_eq!(selected(&model), Some(2));
}

// When the selected candidate is gone the first one is selected.
#[test]
fn a_new_reading_selects_the_first_when_the_candidate_is_gone() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    named(&mut model, Named::Down);
    let fewer = DONE.replace("+work.acme", "+elsewhere");
    fs::write(dir.path().join("done.txt"), fewer).unwrap();
    reload(&mut model);
    assert_eq!(labels(&model), ["+work"]);
    assert_eq!(selected(&model), Some(0));
}

// The `dep:` word being typed is not a prerequisite already.
#[test]
fn the_word_being_typed_does_not_hide_its_item() {
    let dir = listed();
    let model = typing(dir.path(), " dep:parser");
    assert_eq!(labels(&model), ["1  Write parser  +spec01"]);
}

// Smartcase asks for a capital letter (category Lu), and the typed
// text is never lowered.
#[test]
fn only_a_letter_of_category_lu_makes_dep_exact() {
    let dir =
        workdir("Task \u{2160} one\nTask \u{1c5}a\nTask \u{c9}t\u{e9}\n");
    let rows: [(&str, usize); 4] = [
        (" dep:\u{2160}", 0),
        (" dep:\u{1c5}", 0),
        (" dep:\u{c9}", 1),
        (" dep:\u{e9}t", 1),
    ];
    for (typed, want) in rows {
        let mut model = model_of(dir.path());
        press(&mut model, "gga");
        press(&mut model, typed);
        assert_eq!(labels(&model).len(), want, "{typed:?}");
    }
}

#[test]
fn up_and_down_move_the_selection_of_the_popup() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    assert_eq!(labels(&model).len(), 2, "{:?}", labels(&model));
    assert_eq!(selected(&model), Some(0));
    named(&mut model, Named::Down);
    assert_eq!(selected(&model), Some(1));
    named(&mut model, Named::Up);
    assert_eq!(selected(&model), Some(0));
}

/// Ids that never come.
struct NoIds;

impl IdSource for NoIds {
    fn next(&self, _: usize) -> errs::Result<String> {
        Err(errs::Error::new(errs::Code::Io, "no randomness"))
    }
}

#[test]
fn an_id_that_cannot_be_made_leaves_the_word_and_says_why() {
    let dir = listed();
    let mut with = deps(dir.path(), alone());
    with.ids = Box::new(NoIds);
    let mut model = started(with);
    press(&mut model, "Ga");
    press(&mut model, " dep:docs");
    named(&mut model, Named::Tab);
    assert_eq!(text_of(&model), "Call client dep:docs");
    assert_eq!(labels(&model), Vec::<String>::new());
    assert_eq!(model.message.as_deref(), Some("no randomness"));
}
