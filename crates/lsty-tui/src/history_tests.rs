use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use lsty_core::config::{Config, Hidden};

use crate::{
    model::Model,
    test_util::{
        alone, cursor_text, deps, held_lock, keys, press, reload, started,
        todo_of, workdir, workdir_model,
    },
};

const LIST: &str = "A\nB\nC\nD\nE\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn kept(model: &Model) -> (usize, usize) {
    (model.hist.undo.len(), model.hist.redo.len())
}

// 36.39: nothing to repeat, and a sort is not a change.
#[test]
fn repeat_with_nothing_to_repeat_does_nothing() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "j.");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(model.message, None);
    press(&mut model, "s.");
    assert_eq!(todo_of(dir.path()), LIST);
    assert_eq!(kept(&model), (0, 0));
}

// 36.35, 36.38: the next item, and a count replaces the stored one.
#[test]
fn repeat_runs_the_last_change_on_the_item_under_the_cursor() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "x.");
    let day = "x 2026-09-29";
    assert_eq!(todo_of(dir.path()), format!("{day} A\n{day} B\nC\nD\nE\n"));
    press(&mut model, "3.");
    let want = format!("{day} A\n{day} B\n{day} C\n{day} D\n{day} E\n");
    assert_eq!(todo_of(dir.path()), want);
    assert_eq!(kept(&model).0, 3, "each repeat is an action to undo");
}

// 36.36: the count it had.
#[test]
fn repeat_keeps_the_count_of_the_change() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "2ddj.");
    assert_eq!(todo_of(dir.path()), "\n\nC\n\n\n");
}

// 36.37: the text of a prompt, without opening it.
#[test]
fn repeat_runs_a_prompt_again_with_its_text() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "gd2026-10-02<CR>");
    assert_eq!(todo_of(dir.path()), "A due:2026-10-02\nB\nC\nD\nE\n");
    press(&mut model, "j.");
    assert!(model.line.is_none(), "no prompt opens");
    assert_eq!(
        todo_of(dir.path()),
        "A due:2026-10-02\nB due:2026-10-02\nC\nD\nE\n"
    );
}

// 36.40: a repeat is an action of its own.
#[test]
fn undo_and_redo_are_never_repeated() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "xxu.");
    let day = "x 2026-09-29";
    assert_eq!(todo_of(dir.path()), format!("{day} A\n{day} B\nC\nD\nE\n"));
    press(&mut model, "uu");
    assert_eq!(todo_of(dir.path()), LIST);
}

// 36.3, 36.4: empty stacks, and a new action empties the redo stack.
#[test]
fn nothing_to_undo_or_redo() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "u");
    assert_eq!(model.message.as_deref(), Some("nothing to undo"));
    press(&mut model, "xu");
    assert_eq!(kept(&model), (0, 1));
    press(&mut model, "gp");
    assert_eq!(kept(&model), (1, 0));
    keys(&mut model, "<C-r>");
    assert_eq!(todo_of(dir.path()), "(A) A\nB\nC\nD\nE\n");
}

// 36.33: a busy lock keeps the record; the refusal of a changed line
// drops it.
#[test]
fn a_busy_lock_keeps_the_record_and_a_changed_line_drops_it() {
    let config = Config {
        lock_timeout: Duration::from_millis(30),
        ..alone()
    };
    let (dir, mut model) = workdir_model(LIST, config);
    press(&mut model, "x");
    let mut held = held_lock(dir.path());
    press(&mut model, "u");
    assert_eq!(kept(&model), (1, 0));
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    fs::write(dir.path().join("todo.txt"), "x 2026-09-29 A +t\nB\n")
        .unwrap_or_else(|e| panic!("{e}"));
    press(&mut model, "u");
    assert_eq!(kept(&model), (0, 0));
    assert_eq!(
        model.message.as_deref(),
        Some("can't undo: \"x 2026-09-29 A\" was changed since")
    );
}

// 36.19: lines are followed across a reload that shifts them.
#[test]
fn the_lines_of_a_record_are_followed_across_a_reload() {
    let dir = workdir("A\nB\nC\n");
    let mut model = model_of(dir.path());
    press(&mut model, "jx");
    fs::write(dir.path().join("todo.txt"), "Z\nA\nx 2026-09-29 B\nC\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    press(&mut model, "u");
    assert_eq!(todo_of(dir.path()), "Z\nA\nB\nC\n");
}

// An undo on a big file is a check and one write.
#[test]
#[ignore = "timing, run by hand"]
fn undo_of_a_big_file_is_quick() {
    let lines = "item\n".repeat(10_000);
    let dir = workdir(&lines);
    let mut model = model_of(dir.path());
    press(&mut model, "x");
    let start = Instant::now();
    press(&mut model, "u");
    eprintln!("undo of 10000 lines: {:?}", start.elapsed());
    assert_eq!(todo_of(dir.path()), lines);
}

/// Items that differ, so that every line is its own.
fn numbered(count: usize) -> String {
    let mut out = String::new();
    for i in 0..count {
        let line = format!("Item {i} +p{} id:i{i}\n", i % 7);
        out.push_str(&line);
    }
    out
}

#[test]
fn undo_puts_back_a_selection_that_was_done_or_deleted() {
    let lines = numbered(300);
    for seq in ["ggVGx", "ggVGd"] {
        let dir = workdir(&lines);
        let mut model = model_of(dir.path());
        press(&mut model, seq);
        assert_ne!(todo_of(dir.path()), lines, "{seq}");
        press(&mut model, "u");
        assert_eq!(todo_of(dir.path()), lines, "{seq}");
        keys(&mut model, "<C-r>");
        assert_ne!(todo_of(dir.path()), lines, "{seq}");
    }
}

// A bulk change and its undo take time in proportion to the lines.
#[test]
#[ignore = "timing, run by hand"]
fn bulk_changes_and_their_undos_grow_with_the_lines() {
    for count in [5_000, 10_000, 20_000] {
        let dir = workdir(numbered(count));
        let mut model = model_of(dir.path());
        let start = Instant::now();
        press(&mut model, "ggVGx");
        let did = start.elapsed();
        let start = Instant::now();
        press(&mut model, "u");
        eprintln!("{count} lines: ggVGx {did:?}, u {:?}", start.elapsed());
    }
}

/// A name, a file, the keys typed on it, the text under the cursor.
type Row<'a> = (&'a str, &'a str, &'a str, &'a str);

/// Types the keys of each row on its file and checks the cursor.
fn lands_on(rows: &[Row<'_>]) {
    for &(name, file, typed, want) in rows {
        let dir = workdir(file);
        let mut model = model_of(dir.path());
        keys(&mut model, typed);
        assert_eq!(cursor_text(&model), want, "{name}");
    }
}

// After an undo or a redo the cursor is on the lines the action was
// about: the first row of lines that appeared, else the nearest row left
// below the line removed, else the topmost row changed. A changed item
// that a search would hide stays listed, so it counts.
#[test]
fn undo_and_redo_put_the_cursor_on_the_changed_item() {
    let done = "x 2026-09-29";
    // The view sorted is the file backwards: `A` is the topmost row and the
    // last line.
    let backwards = "D\nC\nB\nA\n";
    lands_on(&[
        ("undo of a delete", LIST, "jjddu", "C"),
        ("redo of a delete", LIST, "jjddu<C-r>", "D"),
        ("undo of the last row", LIST, "Gddu", "E"),
        ("redo of the last row", LIST, "Gddu<C-r>", "D"),
        ("undo of a count", LIST, "j2ddu", "B"),
        ("redo of a count", LIST, "j2ddu<C-r>", "D"),
        ("undo of a selection", LIST, "jVjdu", "B"),
        ("undo of a done", LIST, "jjxGu", "C"),
        ("redo of a done", LIST, "jjxGu<C-r>", &format!("{done} C")),
        ("undo of several", LIST, "jVjjxGu", "B"),
        (
            "redo of several",
            LIST,
            "jVjjxGu<C-r>",
            &format!("{done} B"),
        ),
        ("undo of a priority", LIST, "jjgpGu", "C"),
        ("undo of an add", LIST, "jjoN<CR>Gu", "D"),
        ("redo of an add", LIST, "jjoN<CR>Gu<C-r>", "2026-09-29 N"),
        (
            "undo of several, the view's topmost",
            backwards,
            ":sort alpha<CR>ggVjxGu",
            "A",
        ),
        ("undo outside the search", LIST, "jjdd/D<CR>u", "C"),
    ]);
}

// Lines that a write makes beside the action's own (a dependent's `dep:`,
// a paste's relink, the occurrence a `do` spawns) never take the cursor,
// and an action that is only a `dep:` edit still does,
// by undo or by redo, in file order or sorted, with or without a search.
#[test]
fn side_effects_never_take_the_cursor_on_undo_or_redo() {
    let dep = "B dep:a\nA id:a\nC\nD\n";
    let link = "A id:a\nB dep:a\nC\n";
    let arch = "B dep:a\nx 2026-09-29 A id:a\nC\nD\n";
    let archived = "x 2026-09-29 A id:a";
    let pay = "M\nPay rec:1w due:2026-09-30\nZ\n";
    let (open, shut) = ("Pay rec:1w due:2026-09-30", "x 2026-09-29 ");
    let shut = &format!("{shut}{open}");
    lands_on(&[
        ("delete with a dependent", dep, "jdd", "C"),
        ("undo of it", dep, "jddGu", "A id:a"),
        ("redo of it", dep, "jddGu<C-r>", "C"),
        ("undo with a search", dep, "jdd/D<CR>u", "A id:a"),
        ("redo with a search", dep, "jdd/D<CR>u<C-r>", "D"),
        ("untag an id", dep, "jgTid:<CR>u", "A id:a"),
        ("redo of it", dep, "jgTid:<CR>u<C-r>", "A"),
        ("undo of a dep edit alone", dep, "gTdep:<CR>Gu", "B dep:a"),
        ("redo of it", dep, "gTdep:<CR>Gu<C-r>", "B"),
        ("undo of a paste with a dependent", link, "ddjpu", "C"),
        ("redo of it", link, "ddjpu<C-r>", "A id:a"),
        ("undo of the delete before it", link, "ddjpuu", "A id:a"),
        ("undo of an archive", arch, "j:archive<CR>u", archived),
        ("redo of it", arch, "j:archive<CR>u<C-r>", archived),
        ("undo of a done that spawns", pay, "jxu", open),
        ("redo of it", pay, "jxugg<C-r>", shut),
        ("redo of it from the end", pay, "jxuG<C-r>", shut),
        ("undo of it, sorted", pay, ":sort alpha<CR>jxu", open),
        ("redo of it, sorted", pay, ":sort alpha<CR>jxugg<C-r>", shut),
        (
            "undo with auto_archive",
            pay,
            ":set auto_archive<CR>jxu",
            open,
        ),
        (
            "redo with auto_archive",
            pay,
            ":set auto_archive<CR>jxuG<C-r>",
            shut,
        ),
    ]);
}

// The id granted to the item a `dep:` points at is a side effect, and
// the id is made up, so only the start of the line is known.
#[test]
fn a_dep_typed_in_keeps_the_cursor_on_its_item_through_undo_and_redo() {
    let dir = workdir("Alpha\nBravo\nCharlie\nDelta\nEcho\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "jjjA dep:Alp<Tab><CR>u");
    assert_eq!(cursor_text(&model), "Delta");
    keys(&mut model, "G<C-r>");
    assert!(cursor_text(&model).starts_with("Delta dep:"));
}

// The cursor stays on the item the action is about, or goes on to the row
// after a completion; the lines it changes beside it never take it.
#[test]
fn side_effects_never_take_the_cursor_on_the_forward_action() {
    let both = "B dep:a\nA id:a\nC dep:a\nD\n";
    let above = "D\nB dep:a\nA id:a\n";
    let pay = "M\nPay rec:1w due:2026-09-30\nZ\n";
    lands_on(&[
        ("untag an id, dependents around", both, "jgTid:<CR>", "A"),
        ("untag an id, dependents above", above, "jjgTid:<CR>", "A"),
        ("delete a prerequisite", both, "jdd", "C"),
        ("done a prerequisite", both, "jx", "C dep:a"),
        ("recurring done", pay, "jx", "Z"),
    ]);
}

// After a completion the cursor is on the row that followed the last item
// acted on, wherever the item went; on the last row it stays on the item.
// A reopening, and a command naming another row, leave the cursor where it
// is.
#[test]
fn completing_moves_the_cursor_to_the_next_item() {
    let done = "x 2026-09-29";
    let pay = "A\nPay rec:1w due:2026-09-30\nZ\n";
    let rows: &[Row<'_>] = &[
        ("done shown", LIST, "jx", "C"),
        ("done hidden", LIST, "jx", "C"),
        ("auto-archive", LIST, "jx", "C"),
        ("auto-archive, archive shown", LIST, "jx", "C"),
        ("sorted away", LIST, ":sort alpha<CR>jx", "C"),
        ("the last row", LIST, "Gx", &format!("{done} E")),
        ("the last row, hidden", LIST, "Gx", &format!("{done} E")),
        ("the only row", "A\n", "x", &format!("{done} A")),
        ("the last row, archived", LIST, "Gx", &format!("{done} E")),
        (
            "the last row, archive shown",
            LIST,
            "Gx",
            &format!("{done} E"),
        ),
        ("a repeat", LIST, "x.", "C"),
        (
            "a cycle",
            "A\nB started:2026-09-29T0900Z\nC\n",
            "jt",
            &format!("{done} B started:2026-09-29T0900Z"),
        ),
        ("a recurring item", pay, "jx", "Z"),
        ("a reopened item", "A\nx 2026-09-28 B\nC\n", "jx", "B"),
        (
            "reopened items",
            "A\nx 2026-09-28 B\nx 2026-09-28 C\n",
            "jVjx",
            "B",
        ),
        ("a count", LIST, "j2x", "D"),
        ("a selection", LIST, "jVjx", "D"),
        ("a selection upwards", LIST, "jjVkx", "D"),
        ("do with a number", LIST, "j:do 4<CR>", "B"),
        ("do two others", LIST, "j:do 2 4<CR>", "C"),
        ("do without a number", LIST, "j:do<CR>", "B"),
        ("done without a number", LIST, "j:done<CR>", "B"),
        ("undo", LIST, "jxu", "B"),
    ];
    for &(name, file, typed, want) in rows {
        let config = Config {
            auto_archive: name.contains("archive"),
            hidden_items: [
                (!name.contains("shown")).then_some(Hidden::Archive),
                name.contains("hidden").then_some(Hidden::Done),
            ]
            .into_iter()
            .flatten()
            .collect(),
            ..alone()
        };
        let (_dir, mut model) = workdir_model(file, config);
        keys(&mut model, typed);
        assert_eq!(cursor_text(&model), want, "{name}");
    }
}

// While an item is kept in view, a command naming other rows leaves the
// cursor on its row.
#[test]
fn a_command_on_other_rows_keeps_the_cursor_while_they_stay_in_view() {
    let pair = "x 2026-09-28 A\nB\nx 2026-09-28 C\nD\n";
    let both = [Hidden::Archive, Hidden::Done];
    let rows = [
        (LIST, "j:do 4<CR>", &both[..], "B"),
        (LIST, "j:do 4 5<CR>", &both, "B"),
        (LIST, "j:do 1<CR>", &both, "B"),
        (LIST, "G:do 4 5<CR>", &both, "x 2026-09-29 E"),
        (LIST, "G:do 4 5<CR>", &both[..0], "x 2026-09-29 E"),
        (pair, "jj:archive<CR>", &both[..1], "x 2026-09-28 C"),
    ];
    for auto_archive in [true, false] {
        for (file, typed, hidden, want) in rows {
            let config = Config {
                auto_archive,
                hidden_items: hidden.to_vec(),
                ..alone()
            };
            let (_dir, mut model) = workdir_model(file, config);
            keys(&mut model, typed);
            assert_eq!(cursor_text(&model), want, "{typed} {auto_archive}");
        }
    }
}
