use std::{fs, path::Path};

use lsty_core::{errs, ops::IdSource};

use crate::{
    complete::{POPUP_ROWS, ring},
    field::Field,
    model::Model,
    test_util::{alone, deps, keys, press, reload, started, todo_of, workdir},
};

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

/// The labels of the popup; none when it is closed.
fn labels(model: &Model) -> Vec<String> {
    model
        .popup()
        .map(|p| p.cands.iter().map(|c| c.label.clone()).collect())
        .unwrap_or_default()
}

/// The selected candidate of the popup; none when it is closed.
fn selected(model: &Model) -> Option<usize> {
    model.popup().map(|p| p.at)
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
    press(&mut model, &format!("Ga{typed}"));
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
    assert_eq!(at("client", 3), Some((0, "cli".to_owned())));
    assert_eq!(at("client", 6), Some((0, "client".to_owned())));
    // Only a space or a tab ends a word, as in the line.
    assert_eq!(at("a\tb", 3), Some((2, "b".to_owned())));
    assert_eq!(at("a\u{a0}b", 3), Some((0, "a\u{a0}b".to_owned())));
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

// A sentence that ends in a name adds no name of its own, and typing the
// dot towards a child still offers the child.
#[test]
fn punctuation_after_a_name_is_not_a_candidate() {
    let dir = workdir("Ship it for +work.\nPlan +work.acme, @phone.\n");
    let model = typing(dir.path(), " +");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    let model = typing(dir.path(), " +work.");
    assert_eq!(labels(&model), ["+work.acme"]);
    let model = typing(dir.path(), " @");
    assert_eq!(labels(&model), ["@phone"]);
}

// A name that holds the typed text comes after the names that start with
// it and the names with a part that does; a context never answers `+`.
#[test]
fn a_name_matches_anywhere_after_the_others() {
    let dir = listed();
    let model = typing(dir.path(), " +ork");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    let model = typing(dir.path(), " +pec0");
    assert_eq!(labels(&model), ["+spec01", "+spec02"]);
    let model = typing(dir.path(), " +o");
    assert_eq!(labels(&model), ["+home", "+work", "+work.acme"]);
    let model = typing(dir.path(), " @ho");
    assert_eq!(labels(&model), ["@phone"]);
    let mut model = typing(dir.path(), " +ork");
    keys(&mut model, "<Tab>");
    assert!(text_of(&model).ends_with(" +work"), "{}", text_of(&model));
}

// A name may end in plus signs, and a sigil inside a word starts a name.
#[test]
fn a_name_with_plus_signs_and_a_split_word_are_candidates() {
    let dir = workdir("Port +c++ @c++\nCall +work@home\n");
    let model = typing(dir.path(), " +");
    assert_eq!(labels(&model), ["+c++", "+work"]);
    let model = typing(dir.path(), " @");
    assert_eq!(labels(&model), ["@c++", "@home"]);
}

// A name matches at a part after `.` or `/` too, after the names that
// start with the word.
#[test]
fn names_match_at_a_part() {
    let dir = listed();
    let model = typing(dir.path(), " +ac");
    assert_eq!(labels(&model), ["+work.acme"]);
    let model = typing(dir.path(), " @ac");
    assert_eq!(labels(&model), Vec::<String>::new());
}

// 39.14 to 39.16: keys need two characters and no colon.
#[test]
fn keys_are_offered_for_two_characters_without_a_colon() {
    let dir = listed();
    let rows: [(&str, &[&str]); 11] = [
        (" ue", &[]),
        (" art", &[]),
        (" du", &["due:"]),
        (" DU", &["due:"]),
        (" d", &[]),
        (" de", &["dep:"]),
        (" t:", &[]),
        (" st", &["started:"]),
        (" en", &["ended:"]),
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
    keys(&mut model, "<Esc>");
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
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "Call client +work");
    // The cursor ends after the name and its space, before the next word.
    let mut model = typing(dir.path(), " call bob");
    keys(&mut model, "<Left><Left><Left>+<Tab>");
    let cursor = model.edit.as_ref().map(|d| d.field.cursor);
    assert_eq!(cursor, Some("Call client call +home ".chars().count()));
    assert_eq!(labels(&model), Vec::<String>::new());
    assert!(model.edit.is_some());
}

/// A folder holding a list that spells a project and a context twice.
fn spelled() -> tempfile::TempDir {
    workdir("Plan +LSTY-rs @Phone\nPlan +lsty @phone\nCall client\n")
}

#[test]
fn the_word_typed_comes_first_and_enter_saves_on_it() {
    for (typed, first) in [(" +lsty", "+lsty"), (" @phone", "@phone")] {
        let dir = spelled();
        let mut model = typing(dir.path(), typed);
        assert_eq!(labels(&model).first().map(String::as_str), Some(first));
        keys(&mut model, "<CR>");
        assert!(model.edit.is_none(), "{typed}");
        let saved = todo_of(dir.path());
        assert!(saved.ends_with(&format!("Call client{typed}\n")), "{saved}");
    }
}

#[test]
fn enter_still_accepts_when_it_changes_the_text() {
    let dir = listed();
    let cases = [
        (" +work", "<C-n><CR>", "Call client +work.acme"),
        (" +wor", "<CR>", "Call client +work"),
    ];
    for (typed, pressed, text) in cases {
        let mut model = typing(dir.path(), typed);
        keys(&mut model, pressed);
        assert_eq!(text_of(&model), text, "{typed}{pressed}");
        assert_eq!(todo_of(dir.path()), LIST, "{typed}{pressed}");
    }
}

#[test]
fn the_word_typed_with_text_after_it_is_accepted_only_when_glued() {
    let dir = listed();
    let mut model = typing(dir.path(), " x");
    keys(&mut model, "<Left>+work");
    assert_eq!(text_of(&model), "Call client +workx");
    keys(&mut model, "<CR>");
    assert_eq!(text_of(&model), "Call client +work x");
    assert_eq!(todo_of(dir.path()), LIST);
    let mut model = typing(dir.path(), " +work x");
    keys(&mut model, "<Left><Left>");
    assert_eq!(labels(&model).first().map(String::as_str), Some("+work"));
    keys(&mut model, "<CR>");
    assert!(model.edit.is_none());
}

#[test]
fn tab_on_the_word_typed_closes_the_popup_and_typing_goes_on() {
    let dir = listed();
    // Tab does something visible in INSERT mode, to tell an accept from a
    // key handed on to it.
    fs::write(
        dir.path().join("keys.toml"),
        "[insert]\ndelete_to_start = [\"<Tab>\"]\n",
    )
    .unwrap();
    let mut model = typing(dir.path(), " +work");
    keys(&mut model, "<Tab>");
    assert!(model.popup().is_none());
    assert_eq!(text_of(&model), "Call client +work");
    keys(&mut model, "x");
    assert!(model.popup().is_none());
    assert_eq!(text_of(&model), "Call client +workx");
    keys(&mut model, "<CR>");
    assert!(model.edit.is_none());
    assert!(todo_of(dir.path()).ends_with("Call client +workx\n"));
}

#[test]
fn a_rebound_commit_key_saves_on_the_word_typed() {
    let dir = listed();
    fs::write(
        dir.path().join("keys.toml"),
        "[insert]\ncommit = [\"<C-s>\"]\n\
         [completion]\naccept = [\"<Tab>\", \"<C-s>\"]\n",
    )
    .unwrap();
    let mut model = typing(dir.path(), " +work");
    keys(&mut model, "<C-s>");
    assert!(model.edit.is_none());
    assert!(todo_of(dir.path()).ends_with("Call client +work\n"));
}

#[test]
fn an_item_with_an_id_typed_as_a_prerequisite_is_saved_by_enter() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:parser");
    keys(&mut model, "<CR>");
    assert!(model.edit.is_none());
    assert!(todo_of(dir.path()).contains("Call client dep:parser\n"));
}

// 39.8: the selection wraps.
#[test]
fn the_selection_wraps_round_the_list() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    keys(&mut model, "<Down>");
    assert_eq!(selected(&model), Some(1));
    keys(&mut model, "<Down>");
    assert_eq!(selected(&model), Some(0));
    keys(&mut model, "<Up>");
    assert_eq!(selected(&model), Some(1));
}

// 39.37: eight rows are shown, and the window follows the selection.
#[test]
fn at_most_eight_candidates_are_shown_and_the_window_scrolls() {
    let todo = (0..20)
        .fold(String::new(), |all, i| all + &format!("Item +p{i:02}\n"));
    let dir = workdir(&todo);
    let mut model = typing(dir.path(), " +p");
    let shown = |m: &Model, rows: usize| {
        m.popup().map(|p| {
            let (top, cands) = p.shown(rows);
            (top, cands.len())
        })
    };
    assert_eq!(labels(&model).len(), 20);
    assert_eq!(shown(&model, POPUP_ROWS), Some((0, POPUP_ROWS)));
    for _ in 0..POPUP_ROWS {
        keys(&mut model, "<Down>");
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
    let rows: [(&str, &[&str]); 8] = [
        (" file:", &["docs/", "done.txt", "notes.md", "todo.txt"]),
        (" file:d", &["docs/", "done.txt"]),
        (" file:D", &[]),
        (" file:cs", &[]),
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
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "Call client file:docs/");
    assert_eq!(labels(&model), ["b c.pdf"]);
    keys(&mut model, "<Tab>");
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
    keys(&mut model, "gga dep:");
    let want = [
        "3  Write docs  +spec01",
        "4  Write docs  +spec02  fri",
        "5  Call client",
    ];
    assert_eq!(labels(&model), want);
    let mut model = model_of(dir.path());
    keys(&mut model, "jjax dep:");
    assert_eq!(labels(&model).len(), 4);
    let mut model = model_of(dir.path());
    keys(&mut model, "ja dep:");
    assert_eq!(labels(&model).len(), 3);
    assert!(!labels(&model).iter().any(|l| l.starts_with("1  ")));
}

// 39.23, 39.24: an item without an id gets one when the line is
// written, in the same write.
#[test]
fn a_chosen_item_gets_its_id_on_commit_only() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    keys(&mut model, "<Tab>");
    let text = text_of(&model);
    let id = text.rsplit("dep:").next().unwrap().to_owned();
    assert_eq!(id.len(), 4, "{text}");
    assert_eq!(todo_of(dir.path()), LIST);
    keys(&mut model, "<CR>");
    let lines: Vec<String> =
        todo_of(dir.path()).lines().map(str::to_owned).collect();
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
    keys(&mut model, "<Tab> more");
    for _ in 0..16 {
        keys(&mut model, "<BS>");
    }
    keys(&mut model, "<CR>");
    let todo = todo_of(dir.path());
    assert!(todo.contains("\nWrite docs +spec01\n"), "{todo}");
}

// 39.23: a chosen item is offered with its new id, and once the text
// waits for it, not again.
#[test]
fn a_prerequisite_with_a_new_id_is_offered_with_it() {
    let dir = listed();
    let mut model = typing(dir.path(), " dep:docs");
    keys(&mut model, "<Tab> dep:docs");
    assert_eq!(labels(&model), ["4  Write docs  +spec02  fri"]);
}

// A reading made while the popup is open keeps the selection on the
// same candidate.
#[test]
fn a_new_reading_keeps_the_selected_candidate() {
    let dir = listed();
    let mut model = typing(dir.path(), " +w");
    keys(&mut model, "<Down>");
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
    keys(&mut model, "<Down>");
    let fewer = DONE.replace("+work.acme", "+elsewhere");
    fs::write(dir.path().join("done.txt"), fewer).unwrap();
    reload(&mut model);
    assert_eq!(labels(&model), ["+work", "+elsewhere"]);
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
        press(&mut model, &format!("gga{typed}"));
        assert_eq!(labels(&model).len(), want, "{typed:?}");
    }
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
    keys(&mut model, "Ga dep:docs<Tab>");
    assert_eq!(text_of(&model), "Call client dep:docs");
    assert_eq!(labels(&model), Vec::<String>::new());
    assert_eq!(model.message.as_deref(), Some("no randomness"));
}

// No item that waits for the edited one, at any depth, is offered.
#[test]
fn nothing_that_waits_for_the_edited_item_is_offered() {
    // A chain of items, each waiting for the next; the last is edited.
    let chain = (0..4_000).fold(String::new(), |all, i| {
        all + &format!("Task {i} id:c{i} dep:c{}\n", i + 1)
    });
    let dir = workdir(&chain);
    let mut model = typing(dir.path(), " ");
    press(&mut model, "dep:");
    assert_eq!(labels(&model).len(), 0);
}

#[test]
fn the_ring_wraps_in_both_directions() {
    let rows = [(0, 1, 3, 1), (2, 1, 3, 0), (0, -1, 3, 2), (1, -1, 3, 0)];
    for (at, by, n, want) in rows {
        assert_eq!(ring(at, by, n), want, "{at} {by} {n}");
    }
    assert_eq!(ring(0, 0, 1), 0);
    assert_eq!(ring(0, -1, 0), 0);
}

// With text after the cursor the popup opens for the sigil just typed;
// that text does not filter. Accepting replaces the part before the cursor
// and puts a space before text that would run into the name. `back` moves
// the cursor left before `typed`.
#[test]
fn a_name_typed_before_other_text_is_completed() {
    let dir = listed();
    let tail = "Call client";
    let rows = [
        // (line, back, typed, first label, text after accepting)
        (" call bob", 3, "+", "+home", "call +home bob"),
        (" call bob", 3, "@", "@email", "call @email bob"),
        (" call bob", 3, "+w", "+work", "call +work bob"),
        (" call éa", 2, "+w", "+work", "call +work éa"),
        (
            " xy",
            2,
            "dep:",
            "1  Write parser  +spec01",
            "dep:parser xy",
        ),
    ];
    for (line, back, typed, first, want) in rows {
        let mut model = typing(dir.path(), line);
        keys(&mut model, &format!("{}{typed}", "<Left>".repeat(back)));
        let found = labels(&model);
        assert_eq!(found.first().map(String::as_str), Some(first), "{line}");
        keys(&mut model, "<Tab>");
        let text = text_of(&model);
        assert_eq!(text, format!("{tail} {want}"), "{line} {typed}");
    }
}

// Inside a word the popup opens for the sigil just typed and for nothing
// else: a move, a typo fixed in the middle of a name, or more typing after
// the popup was closed leave the line as it is.
#[test]
fn inside_a_name_only_the_sigil_typed_opens_the_popup() {
    let dir = listed();
    let rows = [
        ("a move", " +work", "<Left><Left>"),
        ("a typo fixed", " +wrk", "<Left><Left>o"),
        ("a move after it", " +wrk", "<Left><Left>o<Left>"),
        ("a deletion", " +wxork", "<Left><Left><Left><BS>"),
        ("a closed popup", " call bob", "<Left><Left><Left>+<Esc>w"),
        (
            "a move with it open",
            " call bob",
            "<Left><Left><Left>+<Left>",
        ),
    ];
    for (name, line, typed) in rows {
        let mut model = typing(dir.path(), line);
        keys(&mut model, typed);
        assert!(model.popup().is_none(), "{name}");
    }
    // The line is committed as typed, not completed.
    for (line, typed) in [(" +wrk", "o"), (" +work", "")] {
        let dir = listed();
        let mut model = typing(dir.path(), line);
        keys(&mut model, &format!("<Left><Left>{typed}<CR>"));
        assert!(todo_of(dir.path()).ends_with("Call client +work\n"));
    }
    // A deletion with the popup open keeps it following the word.
    let mut model = typing(dir.path(), " call bob");
    keys(&mut model, "<Left><Left><Left>+wo<BS>");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    keys(&mut model, "o");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    keys(&mut model, "<BS><BS>");
    assert_eq!(labels(&model).len(), 5);
    // Typed on, the popup narrows and accepting keeps the text after.
    let mut model = typing(dir.path(), " call bob");
    keys(&mut model, "<Left><Left><Left>+wo");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
}

// A name after the cursor does not filter the candidates, and no space is
// added at the end of the line.
#[test]
fn the_text_after_the_cursor_does_not_filter() {
    let dir = listed();
    let mut model = typing(dir.path(), " zzz");
    keys(&mut model, "<Left><Left><Left>+");
    assert_eq!(labels(&model).len(), 5);
    let mut model = typing(dir.path(), " +w");
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "Call client +work");
    // The cursor ends after the name and its space, before the next word.
    let mut model = typing(dir.path(), " call bob");
    keys(&mut model, "<Left><Left><Left>+<Tab>");
    let cursor = model.edit.as_ref().map(|d| d.field.cursor);
    assert_eq!(cursor, Some("Call client call +home ".chars().count()));
}

// A read-only token after the cursor keeps its own space.
#[test]
fn a_name_before_a_read_only_token_is_completed() {
    let dir = listed();
    let mut model = model_of(dir.path());
    keys(&mut model, "kA<Left>+w");
    assert_eq!(labels(&model), ["+work", "+work.acme"]);
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "Write parser +spec01 +work id:parser");
}

type Row<'a> = (&'a str, &'a str, &'a [&'a str]);

// A word that is being completed stays so while it is edited: a typo that
// matches nothing, fixed again, and a folder taken, bring the popup back,
// before other text as at the end of the line.
#[test]
fn a_name_being_completed_goes_on_before_other_text() {
    let dir = listed();
    let root = dir.path();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/a.md"), "").unwrap();
    let rows: [Row<'_>; 4] = [
        ("+wo", "q<BS>", &["+work", "+work.acme"]),
        ("@em", "q<BS>", &["@email"]),
        ("file:d", "q<BS>", &["docs/", "done.txt"]),
        ("file:d", "<Tab>", &["a.md"]),
    ];
    for (typed, then, want) in rows {
        let lefts = "<Left>".repeat(3);
        for (line, back) in [(" call bob", lefts.as_str()), (" ", "")] {
            let mut model = typing(root, line);
            keys(&mut model, &format!("{back}{typed}{then}"));
            assert_eq!(labels(&model), want, "{line:?} {typed} {then}");
        }
    }
}
