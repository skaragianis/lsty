use std::{fs, path::Path};

use crate::{
    model::Model,
    test_util::{
        alone, deps, keys, press, reload, started, todo_of, workdir,
        workdir_cancelling,
    },
};

const LIST: &str = "Top\n(A) 2026-09-20 Call client +work id:c1\nEnd\n";
const CHANGED: &str = "this item changed on disk";

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
        "N1\nN2\nTop\n(A) 2026-09-20 Call client +work id:c1 more\nEnd\n";
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
        "Top\n(A) 2026-09-29 Call client +work more\nEnd\n"
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
