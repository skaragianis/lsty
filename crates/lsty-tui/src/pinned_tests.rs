use std::{cell::Cell, fs, path::Path};

use lsty_core::{errs, ops::IdSource};

use crate::{
    model::Model,
    msg::Msg,
    test_util::{
        alone, deps, keys, press, reload, started, todo_of, workdir,
        workdir_cancelling,
    },
};

const LIST: &str = "Top\n(A) 2026-09-20 Call client +work id:c1\nEnd\n";
const CHANGED: &str = "this task changed on disk";

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

// 40.19, 40.16: a change of the line is told once, the commit merges.
#[test]
fn a_change_of_the_line_is_told_and_merged_at_the_commit() {
    let dir = workdir(LIST);
    let mut model = typing(dir.path());
    let disk = LIST.replace("+work", "+work @phone");
    outside(&mut model, dir.path(), &disk);
    assert_eq!(model.message.as_deref(), Some(CHANGED));
    assert_eq!(todo_of(dir.path()), disk);
    keys(&mut model, "<CR>");
    let want =
        "Top\n(A) 2026-09-20 Call client more +work @phone id:c1\nEnd\n";
    assert_eq!(todo_of(dir.path()), want);
}

// 40.20, 40.21: a clash keeps the field open with the draft; Enter
// writes it over the disk's text.
#[test]
fn a_clash_keeps_the_field_open_and_enter_writes_the_draft() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "jccRing client");
    let disk = LIST.replace("Call client", "Call the client");
    outside(&mut model, dir.path(), &disk);
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), disk);
    assert!(model.cursor_at().is_some());
    let said = model.message.clone().unwrap_or_default();
    assert_eq!(
        said,
        "changed on disk meanwhile: text (\"Call the client\") \
         \u{2014} <CR> writes your version"
    );
    keys(&mut model, "<CR>");
    let want = "Top\n(A) 2026-09-20 Ring client +work id:c1\nEnd\n";
    assert_eq!(todo_of(dir.path()), want);
}

// 40.24: a line that was deleted comes back as a copy.
#[test]
fn a_deleted_line_comes_back_as_a_new_task() {
    let dir = workdir(LIST);
    let mut model = typing(dir.path());
    outside(&mut model, dir.path(), "Top\n\nEnd\n");
    keys(&mut model, "<CR>");
    assert_eq!(
        todo_of(dir.path()),
        "Top\n(A) 2026-09-29 Call client +work more id:t001\nEnd\n"
    );
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
}

// 40.23: a cancelled conflict gives the rebased draft back with `u`, and
// Enter writes it over the disk line, which gained a tag meanwhile.
#[test]
fn a_cancelled_conflict_is_restored_rebased_and_written() {
    let (dir, mut model) = workdir_cancelling(LIST);
    keys(&mut model, "jccRing client");
    let disk = LIST.replace("Call client", "Call the client @x");
    outside(&mut model, dir.path(), &disk);
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), disk);
    keys(&mut model, "<C-x>u<CR>");
    let want = "Top\n(A) 2026-09-20 Ring client @x +work id:c1\nEnd\n";
    assert_eq!(todo_of(dir.path()), want);
}

// An id granted by a completion survives a clash: the draft that is written
// over the disk still points at a task that has it.
#[test]
fn a_granted_id_survives_a_clash() {
    let dir = workdir("Alpha\nBravo\nCharlie\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "jccMine dep:Alp<Tab>");
    outside(&mut model, dir.path(), "Alpha\nTheirs\nCharlie\n");
    keys(&mut model, "<CR><CR>");
    let text = todo_of(dir.path());
    let lines: Vec<&str> = text.lines().collect();
    let id = lines.get(1).and_then(|l| l.strip_prefix("Mine dep:"));
    assert!(id.is_some_and(|id| id.len() == 4), "{text}");
    let first = format!("Alpha id:{}", id.unwrap_or_default());
    assert_eq!(lines.first().copied(), Some(first.as_str()), "{text}");
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

fn field_text(model: &Model) -> String {
    model
        .edit
        .as_ref()
        .map(|d| d.field.text())
        .unwrap_or_default()
}

const WAITS: &str = "Alpha\nBravo\nCharlie\n";

#[test]
fn the_ids_a_draft_holds_are_not_given_twice() {
    let dir = workdir(WAITS);
    let mut model = waiting(dir.path(), &["aaaa", "aaaa", "bbbb"]);
    assert_eq!(field_text(&model), "Mine dep:aaaa dep:bbbb");
    keys(&mut model, "<CR>");
    let want = "Alpha id:aaaa\nMine dep:aaaa dep:bbbb\nCharlie id:bbbb\n";
    assert_eq!(todo_of(dir.path()), want);
}

#[test]
fn an_id_another_writer_gave_replaces_the_grant_and_keeps_the_others() {
    let dir = workdir(WAITS);
    let mut model = waiting(dir.path(), &["aaaa", "bbbb"]);
    let theirs = "Alpha id:zzzz\nBravo\nCharlie\n";
    fs::write(dir.path().join("todo.txt"), theirs).unwrap();
    keys(&mut model, "<CR>");
    assert_eq!(field_text(&model), "Mine dep:zzzz dep:bbbb");
    keys(&mut model, "<CR>");
    let want = "Alpha id:zzzz\nMine dep:zzzz dep:bbbb\nCharlie id:bbbb\n";
    assert_eq!(todo_of(dir.path()), want);
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

// A new task below a line that was deleted goes after the line before
// it, and says nothing of a change.
#[test]
fn a_new_task_below_a_deleted_line_goes_after_the_line_before() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "jo new");
    assert_eq!(model.pin_spot().map(|(_, nr)| nr.index()), Some(1));
    outside(&mut model, dir.path(), "Top\nEnd\n");
    assert_eq!(model.pin_spot().map(|(_, nr)| nr.index()), Some(1));
    assert_eq!(model.message, None);
}

// A new task does not tell that its anchor changed.
#[test]
fn a_new_task_says_nothing_of_a_change_of_its_anchor() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "jo new");
    outside(&mut model, dir.path(), &LIST.replace("+work", "+home"));
    assert_eq!(model.message, None);
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
