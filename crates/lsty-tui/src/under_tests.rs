use std::{cell::Cell, fs, path::Path};

use lsty_core::{
    errs,
    ops::{IdSource, LIST_CHANGED},
};

use crate::{
    model::Model,
    msg::Msg,
    test_util::{
        alone, deps, keys, press, reload, started, todo_of, workdir,
        workdir_cancelling,
    },
};

const LIST: &str = "Top\n(A) 2026-09-20 Call client +work id:c1\nEnd\n";
const LINE_CHANGED: &str =
    "Top\n(A) 2026-09-20 Call client +work @phone id:c1\nEnd\n";
const LINE_MOVED: &str = "(A) 2026-09-20 Call client +work id:c1\nTop\nEnd\n";
const LINE_TWIN: &str = "Top\n(A) 2026-09-20 Call client +work id:c1\nEnd\n\
                         (A) 2026-09-20 Call client +work id:c1\n";
const REFUSED: &str =
    "the list changed \u{2014} look again; <Esc> drops your edit";
const DEP_REFUSED: &str =
    "the list changed \u{2014} <Esc> drops it, or delete the dep:";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

/// Writes the file as another writer would and reloads the list.
fn outside(model: &mut Model, dir: &Path, todo: &str) {
    fs::write(dir.join("todo.txt"), todo).unwrap();
    reload(model);
}

/// Opens the field on the second line with its text changed.
fn typing(dir: &Path) -> Model {
    let mut model = model_of(dir);
    keys(&mut model, "jA more");
    model
}

// 40.17, 40.19: reloads that miss the line say nothing; the draft is
// written where the line is now.
#[test]
fn a_reload_that_misses_the_line_says_nothing() {
    let dir = workdir(LIST);
    let mut model = typing(dir.path());
    outside(&mut model, dir.path(), &format!("N1\nN2\n{LIST}"));
    assert_eq!(model.message, None);
    keys(&mut model, "<CR>");
    let want =
        "N1\nN2\nTop\n(A) 2026-09-20 Call client +work more id:c1\nEnd\n";
    assert_eq!(todo_of(dir.path()), want);
}

// What can become of a line under a field. The third is whether a reading
// showed it before the key, and the keys are the first and the next.
type Under = (&'static str, &'static str, bool, &'static str, &'static str);
const UNDER: &[Under] = &[
    ("changed", LINE_CHANGED, true, "<CR>", "<Esc>"),
    ("gone", "Top\nEnd\n", true, "<CR>", "<Esc>"),
    ("blank", "Top\n\nEnd\n", true, "<CR>", "<Esc>"),
    ("moved", LINE_MOVED, true, "<CR>", "<Esc>"),
    ("twin", LINE_TWIN, true, "<CR>", "<Esc>"),
    ("first Esc", LINE_CHANGED, true, "<Esc>", "<Esc>"),
    ("first C-c", LINE_CHANGED, true, "<C-c>", "<C-c>"),
    ("not read", LINE_CHANGED, false, "<CR>", "<Esc>"),
    ("not read Esc", LINE_CHANGED, false, "<Esc>", "<C-c>"),
];

fn stays_open(model: &Model, text: &str, name: &str) {
    assert!(field_text(model).contains(text), "{name}");
    assert_eq!(model.message.as_deref(), Some(REFUSED), "{name}");
}

#[test]
fn a_field_whose_line_changed_refuses_then_drops_on_esc() {
    for &(name, disk, read, first, next) in UNDER {
        let dir = workdir(LIST);
        let mut model = typing(dir.path());
        if read {
            outside(&mut model, dir.path(), disk);
            assert_eq!(model.message.as_deref(), Some(LIST_CHANGED), "{name}");
        } else {
            fs::write(dir.path().join("todo.txt"), disk).unwrap();
        }
        keys(&mut model, first);
        reload(&mut model);
        assert_eq!(todo_of(dir.path()), disk, "{name}");
        stays_open(&model, "more", name);
        keys(&mut model, "<CR>x");
        stays_open(&model, "morex", name);
        let disk = format!("N\n{disk}");
        outside(&mut model, dir.path(), &disk);
        stays_open(&model, "morex", name);
        keys(&mut model, next);
        assert!(model.edit.is_none(), "{name}");
        assert!(model.kept.is_none(), "{name}");
        assert_eq!(todo_of(dir.path()), disk, "{name}");
    }
}

#[test]
fn esc_closes_the_popup_of_a_refused_field_before_the_field() {
    let dir = workdir(LIST);
    let mut model = typing(dir.path());
    outside(&mut model, dir.path(), LINE_CHANGED);
    keys(&mut model, "<CR> dep:To");
    assert!(model.popup().is_some());
    keys(&mut model, "<Esc>");
    assert!(model.popup().is_none());
    assert!(model.edit.is_some());
}

// 40.33: a draft equal to the base writes nothing, whatever changed.
#[test]
fn a_draft_that_equals_the_base_writes_nothing() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "ja");
    let disk = LIST.replace("+work", "+work @phone");
    outside(&mut model, dir.path(), &disk);
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), disk);
    assert!(model.edit.is_none());
}

// `u` gives a cancelled draft back through a shift, and refuses when the line
// changed. A cancel of a line that already changed refuses like a commit.
#[test]
fn a_cancelled_draft_comes_back_only_to_its_own_line() {
    let (dir, mut model) = workdir_cancelling(LIST, alone());
    keys(&mut model, "jA more<C-x>");
    outside(&mut model, dir.path(), &format!("N\n{LIST}"));
    keys(&mut model, "u<CR>");
    let want = "N\nTop\n(A) 2026-09-20 Call client +work more id:c1\nEnd\n";
    assert_eq!(todo_of(dir.path()), want);

    let (dir, mut model) = workdir_cancelling(LIST, alone());
    keys(&mut model, "jA more<C-x>");
    outside(&mut model, dir.path(), LINE_CHANGED);
    keys(&mut model, "u");
    assert!(model.edit.is_none());
    assert_eq!(model.message.as_deref(), Some(LIST_CHANGED));
    assert_eq!(todo_of(dir.path()), LINE_CHANGED);

    let (dir, mut model) = workdir_cancelling(LIST, alone());
    keys(&mut model, "jA more");
    outside(&mut model, dir.path(), LINE_CHANGED);
    keys(&mut model, "<C-x>");
    assert_eq!(model.message.as_deref(), Some(REFUSED));
    keys(&mut model, "<C-x>");
    assert!(model.edit.is_none() && model.kept.is_none());
}

fn field_text(model: &Model) -> String {
    model
        .edit
        .as_ref()
        .map(|d| d.field.text())
        .unwrap_or_default()
}

/// Ids that come in the order given, round and round.
struct Ids(Cell<usize>, &'static [&'static str]);

impl IdSource for Ids {
    fn next(&self, _: usize) -> errs::Result<String> {
        let at = self.0.get();
        self.0.set(at + 1);
        Ok(self.1[at % self.1.len()].to_owned())
    }
}

/// A model of `dir` whose ids are `ids`, with the field on the second
/// line replaced by one that waits for the first and the third.
fn waiting(dir: &Path, ids: &'static [&'static str]) -> Model {
    let mut with = deps(dir, alone());
    with.ids = Box::new(Ids(Cell::new(0), ids));
    let mut model = started(with);
    keys(&mut model, "jccMine dep:Alp<Tab> dep:Cha<Tab>");
    model
}

const WAITS: &str = "Alpha\nBravo\nCharlie\n";

#[test]
fn the_ids_a_draft_holds_are_not_given_twice() {
    let dir = workdir(WAITS);
    let mut model = waiting(dir.path(), &["aaaa", "aaaa", "bbbb"]);
    assert_eq!(field_text(&model), "Mine dep:aaaa dep:bbbb");
    // Picked again, the id is the same and is still given.
    keys(&mut model, "<C-w>dep:Cha<Tab><CR>");
    let want = "Alpha id:aaaa\nMine dep:aaaa dep:bbbb\nCharlie id:bbbb\n";
    assert_eq!(todo_of(dir.path()), want);
}

// A field that waits for a task whose line changed stays open and says the
// way out; taking the `dep:` out of the text lets it write.
const HAS_ID: &str = "Alpha id:a1\nBravo\n";
type DeadDep = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);
const DEAD_DEPS: &[DeadDep] = &[
    (
        "new task",
        "Alpha\nBravo\n",
        "gonew<End>",
        "Alpha again\n",
        "Alpha again\n2026-09-29 new id:t002\n",
    ),
    (
        "task's own line",
        "Alpha\nBravo\n",
        "jccnew dep:Alp<Tab>",
        "Alpha again\nBravo\n",
        "Alpha again\nnew\n",
    ),
    (
        "go, its id on a changed line",
        HAS_ID,
        "gonew<End>",
        "Alpha again id:a1\nBravo\n",
        "Alpha again id:a1\n2026-09-29 new id:t001\nBravo\n",
    ),
    (
        "go, its id on a moved line",
        HAS_ID,
        "gonew<End>",
        "Bravo\nAlpha id:a1\n",
        "Bravo\nAlpha id:a1\n2026-09-29 new id:t001\n",
    ),
    (
        "go, its id gone",
        HAS_ID,
        "gonew<End>",
        "Bravo\n",
        "2026-09-29 new id:t001\nBravo\n",
    ),
    (
        "picked, its id gone",
        HAS_ID,
        "jccnew dep:Alp<Tab>",
        "Bravo\n",
        "new\n",
    ),
];

#[test]
fn a_refused_grant_goes_once_its_dep_is_taken_out() {
    for &(name, list, start, disk, want) in DEAD_DEPS {
        let dir = workdir(list);
        let mut model = model_of(dir.path());
        keys(&mut model, start);
        fs::write(dir.path().join("todo.txt"), disk).unwrap();
        keys(&mut model, "<CR>");
        assert_eq!(todo_of(dir.path()), disk, "{name}");
        assert_eq!(model.message.as_deref(), Some(DEP_REFUSED), "{name}");
        assert!(!field_text(&model).is_empty(), "{name}");
        keys(&mut model, "<C-w><CR>");
        assert_eq!(todo_of(dir.path()), want, "{name}");
    }
}

// An id picked from the popup is not written on its line, which may change
// as long as it holds the id.
#[test]
fn a_picked_id_goes_while_a_line_holds_it() {
    let dir = workdir(HAS_ID);
    let mut model = model_of(dir.path());
    keys(&mut model, "jccnew dep:Alp<Tab>");
    fs::write(dir.path().join("todo.txt"), "Alpha again id:a1\nBravo\n")
        .unwrap();
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), "Alpha again id:a1\nnew dep:a1\n");

    // The same id picked after a dead `go`: the dead grant goes with its word.
    let dir = workdir(HAS_ID);
    let mut model = model_of(dir.path());
    keys(&mut model, "gonew<End>");
    outside(&mut model, dir.path(), "Alpha again id:a1\nBravo\n");
    keys(&mut model, "<CR>");
    assert_eq!(model.message.as_deref(), Some(DEP_REFUSED));
    keys(&mut model, "<C-w>dep:Alp<Tab><CR>");
    let want = "Alpha again id:a1\n2026-09-29 new dep:a1 id:t001\nBravo\n";
    assert_eq!(todo_of(dir.path()), want);

    // Another task picked: the dead `dep:` is still in the text.
    let disk = "Alpha again id:a1\nBravo id:b1\n";
    let dir = workdir("Alpha id:a1\nBravo id:b1\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "gonew<End>");
    outside(&mut model, dir.path(), disk);
    keys(&mut model, " dep:Bra<Tab><CR>");
    assert_eq!(todo_of(dir.path()), disk);
    assert_eq!(model.message.as_deref(), Some(DEP_REFUSED));
}

// A line that reads as the field opened on it again differs in nothing, even
// after a truncate: the field is on it again and the refusal is over.
#[test]
fn a_line_that_comes_back_lets_the_field_write() {
    let shifted = format!("N\n{LIST}");
    for (name, between, back) in [
        ("changed back", LINE_CHANGED, LIST),
        ("truncated", "", shifted.as_str()),
        ("moved back", LINE_MOVED, LIST),
    ] {
        let dir = workdir(LIST);
        let mut model = typing(dir.path());
        outside(&mut model, dir.path(), between);
        keys(&mut model, "<CR>");
        assert_eq!(model.message.as_deref(), Some(REFUSED), "{name}");
        outside(&mut model, dir.path(), back);
        assert_eq!(model.message, None, "{name}");
        keys(&mut model, "<Esc>");
        assert!(model.edit.is_none(), "{name}");
        let want = back.replace("+work", "+work more");
        assert_eq!(todo_of(dir.path()), want, "{name}");
    }
    // The bytes alone do not bring it back: the rule asks from where it stood.
    for (between, end) in [
        ("", LINE_TWIN),
        ("Top\nEnd\n", LINE_MOVED),
        ("", LINE_MOVED),
    ] {
        let dir = workdir(LIST);
        let mut model = typing(dir.path());
        outside(&mut model, dir.path(), between);
        outside(&mut model, dir.path(), end);
        keys(&mut model, "<CR>");
        assert_eq!(todo_of(dir.path()), end);
        assert_eq!(model.message.as_deref(), Some(REFUSED));
    }
}

// `go` and `gO` write a link on the line: on a changed row they open nothing.
#[test]
fn go_on_a_changed_row_opens_no_field() {
    for key in ["go", "gO"] {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        keys(&mut model, "j");
        fs::write(dir.path().join("todo.txt"), LINE_CHANGED).unwrap();
        keys(&mut model, key);
        assert!(model.edit.is_none(), "{key}");
        assert_eq!(model.message.as_deref(), Some(LIST_CHANGED), "{key}");
        assert_eq!(todo_of(dir.path()), LINE_CHANGED, "{key}");
        // The refusal showed the line as it is: now the key is aimed at it.
        keys(&mut model, key);
        assert!(model.edit.is_some(), "{key}");
    }
}

// `gO` cannot write its link once the line changed, whatever is typed.
#[test]
fn a_prerequisite_field_whose_line_changed_refuses_then_drops() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "jgOnew");
    outside(&mut model, dir.path(), LINE_CHANGED);
    assert_eq!(model.message.as_deref(), Some(LIST_CHANGED));
    keys(&mut model, "<CR>");
    assert_eq!(model.message.as_deref(), Some(REFUSED));
    assert_eq!(todo_of(dir.path()), LINE_CHANGED);
    keys(&mut model, "<Esc>");
    assert!(model.edit.is_none());
    assert_eq!(todo_of(dir.path()), LINE_CHANGED);
}

#[test]
fn a_grant_follows_its_task_through_an_outside_change() {
    let dir = workdir(WAITS);
    let mut model = waiting(dir.path(), &["aaaa", "bbbb"]);
    outside(&mut model, dir.path(), &format!("N1\nN2\n{WAITS}"));
    keys(&mut model, " dep:Alp<Tab>");
    assert_eq!(field_text(&model), "Mine dep:aaaa dep:bbbb dep:Alp");
    keys(&mut model, "<CR>");
    let want = "N1\nN2\nAlpha id:aaaa\nMine dep:aaaa dep:bbbb dep:Alp\n\
                Charlie id:bbbb\n";
    assert_eq!(todo_of(dir.path()), want);
}

// A line that was deleted leaves the field after the lines before it.
#[test]
fn a_field_of_a_deleted_line_is_not_in_the_place_of_a_row() {
    let dir = workdir("A\nB\nC\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "GA more");
    assert_eq!(model.field_at(), 2);
    outside(&mut model, dir.path(), "A\nB\n");
    assert_eq!(model.field_at(), 2);
}

// `o` and `O` always write: beside the line when it is there, changed or
// not, and about where it was when it is gone, and say nothing of it.
#[test]
fn a_new_task_writes_beside_its_line_or_about_where_it_was() {
    const NEW: &str = "2026-09-29 new +work id:t001";
    let call = "(A) 2026-09-20 Call client +home id:c1";
    for (key, disk, want) in [
        (
            "o",
            format!("Top\n{call}\nEnd\n"),
            format!("Top\n{call}\n{NEW}\nEnd\n"),
        ),
        (
            "O",
            format!("Top\n{call}\nEnd\n"),
            format!("Top\n{NEW}\n{call}\nEnd\n"),
        ),
        ("o", "Top\nEnd\n".to_owned(), format!("Top\n{NEW}\nEnd\n")),
        ("O", "Top\nEnd\n".to_owned(), format!("Top\n{NEW}\nEnd\n")),
    ] {
        let dir = workdir(LIST);
        let mut model = model_of(dir.path());
        keys(&mut model, &format!("j{key}new"));
        outside(&mut model, dir.path(), &disk);
        assert_eq!(model.message, None, "{key} {disk}");
        keys(&mut model, "<CR>");
        assert_eq!(todo_of(dir.path()), want, "{key} {disk}");
    }
}

// A task written above the cursor lands on the row of the field, and the
// view does not move.
#[test]
fn a_new_task_lands_where_the_field_was_with_the_view_still() {
    let todo = (0..40)
        .map(|i| format!("T{i:02}\n"))
        .collect::<Vec<_>>()
        .concat();
    for keys_before in ["10j<C-e><C-e>", "15j", "Gk"] {
        let dir = workdir(&todo);
        let mut model = model_of(dir.path());
        model.update(Msg::Resize(80, 12));
        keys(&mut model, keys_before);
        keys(&mut model, "Onew");
        let (top, row) = (model.top, model.field_at() - model.top);
        keys(&mut model, "<CR>");
        assert_eq!(model.top, top, "{keys_before}");
        assert_eq!(model.cursor - model.top, row, "{keys_before}");
    }
}
