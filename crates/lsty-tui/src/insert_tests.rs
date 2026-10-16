use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use lsty_core::config::{Config, Filter, LineNumbers};

use crate::{
    model::Model,
    msg::Msg,
    test_util::{
        alone, deps, held_lock, keys, press, reload, screen, started, todo_of,
        workdir, workdir_cancelling, workdir_model,
    },
};

const LIST: &str = "Buy\nCall\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn keeping() -> Config {
    Config {
        keep_adding: true,
        ..alone()
    }
}

fn mode_of(model: &Model) -> String {
    let rows = screen(model, 80, 24);
    rows.last().cloned().unwrap_or_default()
}

// 38.7: nothing is written when nothing changed.
#[test]
fn a_draft_that_was_not_changed_writes_nothing() {
    for key in ["<CR>", "<Esc>"] {
        let dir = workdir(LIST);
        let path = dir.path().join("todo.txt");
        let old = SystemTime::UNIX_EPOCH + Duration::from_hours(438_288);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(old).unwrap();
        let mut model = model_of(dir.path());
        press(&mut model, "a");
        assert!(model.cursor_at().is_some());
        keys(&mut model, key);
        assert!(model.cursor_at().is_none(), "{key}: still in INSERT");
        let seen = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(seen, old, "{key}");
        assert_eq!(todo_of(dir.path()), LIST);
    }
}

// 38.7 with a line that is not UTF-8: the field shows it lossily, and
// nothing is written while it is not changed.
#[test]
fn a_line_that_is_not_utf8_is_left_alone_when_unchanged() {
    let raw = b"caf\xe9\n";
    let dir = workdir(raw);
    let mut model = model_of(dir.path());
    keys(&mut model, "i<CR>");
    assert_eq!(fs::read(dir.path().join("todo.txt")).unwrap(), raw);
}

// 38.24 in an empty list: the first task.
#[test]
fn o_creates_the_first_task_of_an_empty_list() {
    let dir = workdir("");
    fs::write(dir.path().join("todo.txt"), "").unwrap();
    let mut model = model_of(dir.path());
    keys(&mut model, "oFirst<CR>");
    assert_eq!(todo_of(dir.path()), "2026-09-29 First\n");
    assert!(model.cursor_at().is_none());
    assert_eq!(model.cursor, 0);
}

#[test]
fn keep_adding_opens_another_new_task_below_the_one_written() {
    for (seq, want) in [
        ("o", "Buy\n2026-09-29 One\n2026-09-29 Two\nCall\n"),
        ("O", "2026-09-29 One\n2026-09-29 Two\nBuy\nCall\n"),
    ] {
        let (dir, mut model) = workdir_model(LIST, keeping());
        keys(&mut model, &format!("{seq}One<CR>"));
        assert!(model.cursor_at().is_some(), "{seq}: a field is open");
        keys(&mut model, "Two<CR><CR>");
        assert_eq!(todo_of(dir.path()), want, "{seq}");
        assert!(model.cursor_at().is_none(), "{seq}: an empty one ends");
    }
}

#[test]
fn cancel_on_a_new_task_with_nothing_typed_keeps_no_draft() {
    let (dir, mut model) = workdir_model(LIST, keeping());
    keys(&mut model, "oOne<CR><Esc>");
    assert!(model.cursor_at().is_none());
    assert_eq!(model.message, None);
    press(&mut model, "u");
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn keep_adding_leaves_an_edited_task_alone() {
    let (dir, mut model) = workdir_model(LIST, keeping());
    keys(&mut model, "A!<CR>");
    assert_eq!(todo_of(dir.path()), "Buy!\nCall\n");
    assert!(model.cursor_at().is_none());
}

#[test]
fn a_new_task_ends_insert_mode_unless_keep_adding_is_on() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "oOne<CR>");
    assert_eq!(todo_of(dir.path()), "Buy\n2026-09-29 One\nCall\n");
    assert!(model.cursor_at().is_none());
}

// 38.26: nothing typed, nothing created.
#[test]
fn a_new_task_with_only_spaces_is_not_created() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "o<CR>o   <Esc>");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.message, None);
}

// 38.34 and 38.35: the draft comes back with `u`, once.
#[test]
fn u_brings_a_discarded_draft_back_once() {
    let (dir, mut model) = workdir_cancelling(LIST);
    keys(&mut model, "izzz<C-x>");
    assert_eq!(
        model.message.as_deref(),
        Some("edit discarded — u restores")
    );
    press(&mut model, "u");
    assert!(model.cursor_at().is_some());
    assert!(mode_of(&model).contains("INSERT"));
    keys(&mut model, "<Esc>");
    assert_eq!(todo_of(dir.path()), "zzzBuy\nCall\n");
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("undid: 1 zzzBuy"));
    assert_eq!(todo_of(dir.path()), LIST);
}

// 38.35: the task is gone when `u` brings the draft back.
#[test]
fn a_kept_draft_of_a_deleted_task_says_so() {
    let (dir, mut model) = workdir_cancelling(LIST);
    keys(&mut model, "izzz<C-x>");
    fs::write(dir.path().join("todo.txt"), "Call\n").unwrap();
    reload(&mut model);
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("the task is gone"));
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("nothing to undo"));
}

// 38.33: the edit actions do not open on an archive row.
#[test]
fn an_archived_task_cannot_be_edited() {
    let dir = workdir("Open\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n").unwrap();
    let config = Config {
        hidden_tasks: Vec::new(),
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
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), "Buy one two\nCall\n");
}

// 38.44: a lock timeout keeps the draft open; commit again retries.
#[test]
fn a_busy_lock_keeps_the_draft() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "ax");
    let mut held = held_lock(dir.path());
    keys(&mut model, "<CR>");
    assert!(model.cursor_at().is_some());
    assert!(model.message.as_deref().unwrap_or("").contains("is busy"));
    held.release().unwrap();
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), "Buyx\nCall\n");
}

// With keep_adding on, a new task whose write fails does not chain: the
// draft stays with its text and no second field opens.
#[test]
fn a_failed_write_of_a_new_task_does_not_chain() {
    let (dir, mut model) = workdir_model(LIST, keeping());
    keys(&mut model, "ox");
    let mut held = held_lock(dir.path());
    keys(&mut model, "<CR>");
    let text = model.edit.as_ref().map(|d| d.field.text());
    assert_eq!(text.as_deref(), Some("x"), "the draft was not kept");
    assert!(model.message.as_deref().unwrap_or("").contains("is busy"));
    held.release().unwrap();
    assert_eq!(todo_of(dir.path()), LIST);
}

// A word ends at a space or a tab only: a no-break space is typed text.
#[test]
fn a_new_task_differing_from_its_prefill_by_a_no_break_space_is_kept() {
    let (_dir, mut model) = workdir_model("A +home\nB\n", keeping());
    keys(&mut model, "o");
    let text = |m: &Model| m.edit.as_ref().map(|d| d.field.text());
    let shown = text(&model).unwrap_or_default();
    assert!(shown.contains("+home"), "{shown:?}");
    keys(&mut model, "\u{a0}<Esc>");
    assert!(model.cursor_at().is_some(), "the draft was thrown away");
}

/// What the field of a new task holds after `keys` opened it.
fn opened(config: Config, todo: &str, keys_: &str) -> String {
    let (_dir, mut model) = workdir_model(todo, config);
    keys(&mut model, keys_);
    model
        .edit
        .as_ref()
        .map(|d| d.field.text())
        .unwrap_or_default()
}

#[test]
fn a_new_task_starts_with_the_projects_of_the_task_beside_it() {
    let todo = "A +x +work.acme @c\nB +y\nC\n";
    for (case, keys_, want) in [
        ("o, two projects", "o", " +x +work.acme"),
        ("O, two projects", "O", " +x +work.acme"),
        ("o, one project", "jo", " +y"),
        ("O, none", "jjO", ""),
        ("o, none", "jjo", ""),
    ] {
        assert_eq!(opened(alone(), todo, keys_), want, "{case}");
    }
}

#[test]
fn a_new_task_has_no_inherited_project_without_an_task_beside_it() {
    assert_eq!(opened(alone(), "", "o"), "");
    let dir = workdir("A\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old +gone\n")
        .unwrap();
    let config = Config {
        hidden_tasks: Vec::new(),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    keys(&mut model, "jo");
    let text = model.edit.as_ref().map(|d| d.field.text());
    assert_eq!(text.as_deref(), Some(""));
}

#[test]
fn the_sidebar_a_search_and_a_filter_have_no_bearing_on_the_prefill() {
    let config = Config {
        inherit_context: true,
        show_sidebar: true,
        ..alone()
    };
    let todo = "A +x @c\nB +y\nC\n";
    for (case, keys_, want) in [
        ("a search", "/B<CR>o", " +y"),
        ("a chosen project", "<C-w>3x<C-w>1jo", " +x @c"),
        ("a chosen context, none beside", "<C-w>4x<C-w>1/zz<CR>o", ""),
        (
            "a chosen project, an empty list",
            "<C-w>3x<C-w>1/B<CR>o",
            "",
        ),
    ] {
        assert_eq!(opened(config.clone(), todo, keys_), want, "{case}");
    }
    let named = Config {
        filters: vec![Filter {
            name: "first".to_owned(),
            query: "A".to_owned(),
        }],
        start_filter: Some("first".to_owned()),
        ..config
    };
    assert_eq!(opened(named, todo, "o"), " +x @c");
}

#[test]
fn the_option_off_leaves_no_prefill_whatever_the_sidebar_chose() {
    let config = Config {
        inherit_project: false,
        show_sidebar: true,
        ..alone()
    };
    assert_eq!(opened(config, "A +x @c\n", "<C-w>3x<C-w>1o"), "");
}

#[test]
fn a_prefilled_new_task_with_nothing_typed_writes_nothing() {
    let (dir, mut model) = workdir_model("A +x\n", alone());
    keys(&mut model, "o<CR>");
    assert!(model.edit.is_none());
    assert_eq!(todo_of(dir.path()), "A +x\n");
}

#[test]
fn the_follow_up_of_keep_adding_inherits_from_the_task_just_written() {
    let (_dir, mut model) = workdir_model("A\n", keeping());
    keys(&mut model, "oB +x<CR>");
    let text = model.edit.as_ref().map(|d| d.field.text());
    assert_eq!(text.as_deref(), Some(" +x"));
}

#[test]
fn the_two_inherit_options_are_independent() {
    let todo = "A +x +y @c @d.e\nB\n";
    for (projects, contexts, want) in [
        (false, false, ""),
        (true, false, " +x +y"),
        (false, true, " @c @d.e"),
        (true, true, " +x +y @c @d.e"),
    ] {
        let config = Config {
            inherit_project: projects,
            inherit_context: contexts,
            ..alone()
        };
        let case = format!("projects {projects}, contexts {contexts}");
        assert_eq!(opened(config.clone(), todo, "o"), want, "{case}");
        assert_eq!(opened(config.clone(), todo, "O"), want, "{case}");
        assert_eq!(opened(config, todo, "jo"), "", "{case}: no names");
    }
}

#[test]
fn the_follow_up_of_keep_adding_inherits_the_contexts_too() {
    let config = Config {
        inherit_context: true,
        ..keeping()
    };
    let (_dir, mut model) = workdir_model("A\n", config);
    keys(&mut model, "oB @c<CR>");
    let text = model.edit.as_ref().map(|d| d.field.text());
    assert_eq!(text.as_deref(), Some(" @c"));
}

// Insert edits as vim does: `<C-h>` deletes back; `<C-a>` and `<C-e>`
// are not bound and change neither the text nor the cursor.
#[test]
fn insert_takes_c_h_and_leaves_c_a_and_c_e_alone() {
    let rows = [
        ("<C-h> deletes", "Buy abc<C-h>", ("Buy ab", 6)),
        ("<C-a> does nothing", "Buy abc<Left><C-a>", ("Buy abc", 6)),
        ("<C-e> does nothing", "Buy abc<Left><C-e>", ("Buy abc", 6)),
    ];
    for (name, typed, want) in rows {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        keys(&mut model, &format!("cc{typed}"));
        let field = &model.edit.as_ref().expect("insert is open").field;
        assert_eq!((field.text().as_str(), field.cursor), want, "{name}");
    }
}

#[test]
fn a_dependent_task_starts_with_the_dep_after_the_prefill() {
    let todo = "A +x id:k3f9\nB\n";
    for (case, todo, keys_, want) in [
        ("id and project", todo, "go", " +x dep:k3f9"),
        ("nothing to depend on", "", "go", ""),
    ] {
        assert_eq!(opened(alone(), todo, keys_), want, "{case}");
    }
}

#[test]
fn a_dependent_task_waits_for_the_id_the_other_will_get() {
    let text = opened(alone(), "A\nB\n", "jgo");
    assert!(text.starts_with(" dep:") && text.len() == 9, "{text:?}");
}

#[test]
fn a_dependent_task_of_an_untouched_draft_writes_nothing() {
    let (dir, mut model) = workdir_model("A\nB\n", alone());
    keys(&mut model, "go<CR>");
    assert!(model.cursor_at().is_none(), "still in INSERT");
    assert_eq!(todo_of(dir.path()), "A\nB\n");
}

#[test]
fn a_prerequisite_draft_names_the_task_it_blocks() {
    let (_dir, mut model) = workdir_model("A\nB id:k3f9\n", alone());
    let said = |m: &Model| m.blocking();
    keys(&mut model, "gO");
    assert_eq!(said(&model).as_deref(), Some("blocks A"));
    keys(&mut model, "<Esc>");
    keys(&mut model, "jgO");
    assert_eq!(said(&model).as_deref(), Some("blocks B"));
    keys(&mut model, "<Esc>");
    assert_eq!(said(&model), None);
    keys(&mut model, "O");
    assert_eq!(said(&model), None);
}

/// A model over `todo` and an archive, with the archive rows shown and
/// the cursor on the row after `down` presses of `j`.
fn with_archive(
    todo: &str,
    done: &str,
    down: &str,
) -> (tempfile::TempDir, Model) {
    let dir = workdir(todo);
    fs::write(dir.path().join("done.txt"), done).unwrap();
    let config = Config {
        hidden_tasks: Vec::new(),
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    if !down.is_empty() {
        keys(&mut model, down);
    }
    (dir, model)
}

fn field_of(model: &Model) -> Option<String> {
    model.edit.as_ref().map(|d| d.field.text())
}

// What `dep` does: a done task in the todo file is a prerequisite and
// waits like any other task, and an archived one is a prerequisite with
// an id, never one without, and never waits.
#[test]
fn a_linked_task_of_a_done_task_in_the_todo_file_links() {
    let todo = "x 2026-09-01 A id:k3f9\nx 2026-09-01 B\n";
    let (_dir, mut model) = with_archive(todo, "", "");
    keys(&mut model, "go");
    assert_eq!(field_of(&model).as_deref(), Some(" dep:k3f9"));
    keys(&mut model, "<Esc>");
    keys(&mut model, "jgo");
    let text = field_of(&model).unwrap();
    assert!(text.starts_with(" dep:") && text.len() == 9, "{text:?}");
    keys(&mut model, "<Esc>");
    keys(&mut model, "gO");
    assert_eq!(model.blocking().as_deref(), Some("blocks B"));
}

#[test]
fn a_dependent_task_of_an_archived_task_follows_dep() {
    let (dir, mut model) =
        with_archive("Open\n", "x 2026-09-01 Old id:o1d2\n", "j");
    keys(&mut model, "go");
    assert_eq!(field_of(&model).as_deref(), Some(" dep:o1d2"));
    keys(&mut model, "New<CR>");
    assert!(todo_of(dir.path()).ends_with(" New dep:o1d2\n"));

    let (dir, mut model) = with_archive("Open\n", "x 2026-09-01 Old\n", "j");
    keys(&mut model, "go");
    assert_eq!(field_of(&model), None);
    assert_eq!(
        model.message.as_deref(),
        Some("Old is archived and has no id — x restores it first")
    );
    assert_eq!(todo_of(dir.path()), "Open\n");
}

#[test]
fn a_prerequisite_task_of_an_archived_task_is_refused() {
    let (dir, mut model) =
        with_archive("Open\n", "x 2026-09-01 Old id:o1d2\n", "j");
    keys(&mut model, "gO");
    assert_eq!(field_of(&model), None);
    assert_eq!(
        model.message.as_deref(),
        Some("Old is archived — x restores it")
    );
    assert_eq!(todo_of(dir.path()), "Open\n");
}

#[test]
fn the_status_line_names_the_start_of_the_text_in_24_cells() {
    for (text, want) in [
        ("A rather long text of a task", "A rather long text of a…"),
        ("漢字漢字漢字漢字漢字漢字漢字", "漢字漢字漢字漢字漢字漢…"),
        ("+p @c k:v", "+p @c k:v"),
        ("+proj @ctx due:2026-10-20", "+proj @ctx due:2026-10-…"),
    ] {
        let (_dir, mut model) = workdir_model(&format!("{text}\n"), alone());
        keys(&mut model, "gO");
        assert_eq!(
            model.blocking().as_deref(),
            Some(format!("blocks {want}").as_str()),
            "{text}"
        );
    }
}

#[test]
fn an_archived_task_of_only_labels_is_named_by_its_line() {
    let (_dir, mut model) =
        with_archive("Open\n", "x 2026-09-01 +p id:o1d2\n", "j");
    keys(&mut model, "gO");
    assert_eq!(
        model.message.as_deref(),
        Some("x 2026-09-01 +p id:o1d2 is archived — x restores it")
    );
    let (_dir, mut model) = with_archive("Open\n", "x 2026-09-01 +p\n", "j");
    keys(&mut model, "go");
    assert_eq!(
        model.message.as_deref(),
        Some(
            "x 2026-09-01 +p is archived and has no id — x restores it first"
        )
    );
}

#[test]
fn a_replaced_task_shows_its_number_only_with_the_numbers_on() {
    for (numbers, want) in [
        (LineNumbers::Off, "│                            │"),
        (LineNumbers::Absolute, "│  2                         │"),
    ] {
        let config = Config {
            line_numbers: numbers,
            ..alone()
        };
        let (_dir, mut model) = workdir_model("one\ntwo\nthree\n", config);
        model.update(Msg::Resize(30, 8));
        keys(&mut model, "jcc");
        assert_eq!(screen(&model, 30, 8)[2], want, "{numbers:?}");
    }
}
