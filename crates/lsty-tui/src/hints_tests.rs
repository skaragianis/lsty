use std::{collections::BTreeMap, fs};

use lsty_core::{
    config::{Config, Hidden},
    keys::{Action, Mode},
    theme::{self, Profile, Slot},
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    style::{Color, Modifier},
};
use tempfile::TempDir;

use crate::{
    focus::Focus,
    hints::{defaults, label, warnings},
    model::Model,
    msg::Msg,
    press::{Answer, decide},
    test_util::{
        alone, deps, keys, plain, press, screen, started, workdir,
        workdir_model,
    },
};

type Hints = BTreeMap<String, Vec<String>>;

fn table(mode: &str, names: &[&str]) -> BTreeMap<String, Vec<String>> {
    let names = names.iter().map(|n| (*n).to_owned()).collect();
    BTreeMap::from([(mode.to_owned(), names)])
}

#[test]
fn hint_warnings() {
    let rows: [(Hints, Vec<&str>); 4] = [
        (
            table("normal", &["help", "frob"]),
            vec![r#"hints.normal: unknown action "frob""#],
        ),
        (table("frob", &["help"]), vec!["hints.frob: unknown mode"]),
        (
            table("a\x1bb", &["help"]),
            vec!["hints.a\\x1bb: unknown mode"],
        ),
        (table("normal", &["help", "quit"]), vec![]),
    ];
    for (hints, want) in rows {
        let got = warnings(&hints);
        assert!(got.iter().all(|w| w.source == "config.toml"));
        let got: Vec<&str> = got.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(got, want);
    }
}

#[test]
fn every_mode_has_default_hints() {
    for mode in Mode::ALL {
        assert!(!defaults(mode).is_empty(), "{}", mode.as_str());
    }
}

/// The INSERT hint row on a screen `width` wide, with a keys file and
/// a `[hints]` row for insert mode.
fn insert_row(keys: &str, hints: &[&str], width: u16) -> String {
    let dir = workdir("A\n");
    fs::write(dir.path().join("keys.toml"), keys)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut config = alone();
    if !hints.is_empty() {
        let names = hints.iter().map(|n| (*n).to_owned()).collect();
        config.hints.insert("insert".to_owned(), names);
    }
    let mut model = started(deps(dir.path(), config));
    model.update(Msg::Resize(width, 12));
    press(&mut model, "o");
    let lines = screen(&model, width, 12);
    let last = lines.last().map_or("", |l| l.trim());
    last.strip_suffix("9.9.9").unwrap_or(last).trim().to_owned()
}

#[test]
fn the_insert_row() {
    let remap =
        "[insert]\ndelete_word_back = [\"<C-h>\"]\ndelete_to_start = []\n";
    let all = "<CR> commit  <C-w> del word  <C-u> del to start";
    let rows = [
        ("default", "", &[][..], 90, all),
        (
            "no room for the last",
            "",
            &[],
            70,
            "<CR> commit  <C-w> del word",
        ),
        ("room for the first only", "", &[], 50, "<CR> commit"),
        (
            "remapped and unbound",
            remap,
            &[],
            80,
            "<CR> commit  <C-h> del word",
        ),
        (
            "configured",
            "",
            &["delete_to_start"],
            80,
            "manual  <C-u> del to start",
        ),
    ];
    for (name, keys, hints, width, end) in rows {
        let row = insert_row(keys, hints, width);
        assert!(row.starts_with("INSERT"), "{name}: {row}");
        assert!(row.ends_with(end), "{name}: {row}");
    }
}

#[test]
fn the_version_needs_the_whole_left_part_and_a_gap() {
    // " NORMAL  todo.txt  manual" is 25 cells; the version takes 2 + 6.
    for (width, shown) in [(33, true), (32, false)] {
        let dir = workdir("A\n");
        let mut model = started(deps(dir.path(), alone()));
        model.update(Msg::Resize(width, 6));
        let lines = screen(&model, width, 6);
        let row = lines.last().map_or("", String::as_str);
        assert_eq!(row.ends_with("9.9.9"), shown, "{width}: {row}");
    }
}

#[test]
fn clear_search_shows_while_a_search_is_in_effect() {
    let clear = "<Esc> clear search";
    let rows = [
        ("none", "gg", 80, "manual  x done"),
        ("kept text", "/A<CR>", 80, "<Esc> clear search  x done"),
        ("moved names only", "/+work<CR>", 80, clear),
        ("cleared", "/+work A<CR><Esc>", 80, "manual  x done"),
        (
            "by hand",
            "/+work<CR><C-w>3<Space><Esc>",
            80,
            "manual  x done",
        ),
        (
            "already selected",
            "/+work<CR>/+work<CR>",
            80,
            "manual  x done",
        ),
        // Only the first hint fits beside the version.
        ("narrow", "/A<CR>", 56, "manual  <Esc> clear search   "),
        ("search prompt", "/A", 80, "/A  <CR> keep  <Tab> next"),
        (": line", ":", 80, ":  <CR> run  <Tab> next"),
        ("due prompt", "gd", 80, "due:   <CR> set  <Tab> next"),
        ("tag prompt", "gt", 80, "tag:   <CR> add  <Tab> next"),
        ("untag prompt", "gT", 80, "untag:   <CR> remove  <Tab> next"),
    ];
    for (name, typed, width, has) in rows {
        let dir = workdir("A +work\nB +home\n");
        let mut model = started(deps(dir.path(), alone()));
        model.update(Msg::Resize(width, 6));
        keys(&mut model, typed);
        let lines = screen(&model, width, 6);
        let row = lines.last().map_or("", String::as_str);
        assert!(row.contains(has), "{name}: {row}");
        assert_eq!(row.contains(clear), has.contains(clear), "{name}: {row}");
    }
}

/// What the status line shows dim, key and word, on a screen `width` wide.
fn shown_dim(model: &Model, width: u16) -> String {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
    model.draw_status(&mut buf, (width, 0));
    let dim = model.styles.get(Some(Slot::HintDim)).fg;
    let cells = buf.content.iter().map(|cell| {
        if Some(cell.fg) == dim {
            cell.symbol()
        } else {
            " "
        }
    });
    let text: String = cells.collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At 16 colours a lit hint and a dim one differ by attribute alone, in the
/// terminal's own text colour, whatever the theme: on a done task `x` is
/// lit, `u` dim.
#[test]
fn hints_at_16_colours_read_lit_and_dim_in_a_theme_that_lost_them() {
    for name in [theme::DEFAULT_NAME, "tokyo-night-storm"] {
        let dir = workdir("x 2026-09-01 A\n");
        let mut with = deps(dir.path(), alone());
        with.profile = Profile::Ansi;
        let mut model = started(with);
        model.show_theme(name);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        model.draw_status(&mut buf, (80, 0));
        let row: String = buf.content.iter().map(Cell::symbol).collect();
        let cell = |text: &str, at: usize| {
            let found = row.find(text).unwrap_or_else(|| panic!("{row}"));
            let found = &buf.content[found + at];
            (found.fg, found.bg, found.modifier)
        };
        let look = |modifier| (Color::Reset, Color::Reset, modifier);
        let (lit, plain, dim) =
            (Modifier::BOLD, Modifier::empty(), Modifier::DIM);
        assert_eq!(cell(" x ", 1), look(lit), "{name} lit key: {row}");
        assert_eq!(cell(" x ", 3), look(plain), "{name} lit word: {row}");
        assert_eq!(cell(" u ", 1), look(dim), "{name} dim key: {row}");
        assert_eq!(cell(" u ", 3), look(dim), "{name} dim word: {row}");
    }
}

/// A model of `todo` and `done`, with `[hints]` of `mode` and `keys.toml`,
/// after `typed`.
fn hinted(
    (todo, done): (&str, &str),
    hints: Option<(&str, &[&str])>,
    keymap: &str,
    typed: &str,
) -> (TempDir, Model) {
    let dir = workdir(todo);
    let write = |name: &str, text: &str| {
        fs::write(dir.path().join(name), text)
            .unwrap_or_else(|e| panic!("{e}"));
    };
    write("done.txt", done);
    write("keys.toml", keymap);
    let mut config = Config {
        hidden_tasks: vec![Hidden::Later],
        show_detail: true,
        show_sidebar: true,
        ..alone()
    };
    if let Some((mode, names)) = hints {
        config.hints = table(mode, names);
    }
    let mut model = started(deps(dir.path(), config));
    if !typed.is_empty() {
        keys(&mut model, typed);
    }
    (dir, model)
}

const OLD: &str = "x 2026-09-01 Old\n";
const STUCK: &str = "Blocker id:b\nStuck dep:b\n";
const STUCK_DOING: &str =
    "Blocker id:b\nStuck dep:b started:2026-09-29T0800Z\n";
const SKIP: &str = "x 2026-09-01 Skip wontdo:true\n";
const DONE: &str = "x 2026-09-01 Done\n";
const FILTERED: &str = "<C-w>5Gx<C-w>3";
const NESTED: &str = "A +a.b @x.y\n";
const NO_FOLD: &str = "h fold l unfold";
const EMPTY: &str = "x filter h fold l unfold";
const BEFORE: &str = "<C-w> del word <C-u> del to start";

/// Each state of the default hints: the files, what is typed on them, and
/// the hints that are dim.
#[test]
#[expect(clippy::too_many_lines, reason = "a table")]
fn the_default_hints_are_dim_when_their_key_would_change_nothing() {
    let rows = [
        (
            "no task",
            "",
            "",
            "",
            "x done t start T wont a append dd delete u undo",
        ),
        (
            "tasks all filtered out",
            "A\n",
            "",
            "/zzz<CR>",
            "x done t start T wont a append dd delete u undo",
        ),
        ("todo", "A\n", "", "", "u undo"),
        ("after a write", "A\n", "", "x", ""),
        ("after an undo", "A\n", "", "xu", "u undo"),
        ("doing", "A started:2026-09-29T0800Z\n", "", "", "u undo"),
        ("blocked", STUCK, "", "j", "x done t start u undo"),
        ("blocked doing", STUCK_DOING, "", "j", "x done u undo"),
        ("done", DONE, "", "", "u undo"),
        ("won't do", SKIP, "", "", "u undo"),
        ("archived", "", OLD, "", "a append dd delete u undo"),
        ("visual todo", "A\nB\n", "", "vj", ""),
        ("visual blocked", STUCK, "", "vj", "x done t start"),
        (
            "visual todo and done",
            "A\nx 2026-09-01 B\n",
            "",
            "vj",
            "t start",
        ),
        (
            "visual done",
            "x 2026-09-01 A\nx 2026-09-01 B\n",
            "",
            "vj",
            "",
        ),
        ("visual archived", "", OLD, "v", "d delete"),
        (
            "visual open and archived",
            "A\n",
            OLD,
            "vj",
            "t start d delete",
        ),
        ("detail todo", "A\n", "", "<C-w>2", "<CR> link"),
        ("detail link", "A https://x.org\n", "", "<C-w>2", ""),
        ("detail blocked", STUCK, "", "j<C-w>2", "x done <CR> link"),
        ("detail archived", "", OLD, "<C-w>2", "<CR> link dd delete"),
        ("empty Projects", "A\n", "", "<C-w>3", EMPTY),
        ("empty Contexts", "A\n", "", "<C-w>4", EMPTY),
        ("Projects, no task", "", "", "<C-w>3", EMPTY),
        ("empty Projects, search", "A\n", "", "/A<CR><C-w>3", EMPTY),
        (
            "empty Projects, search hides all",
            "A\n",
            "",
            "/zzz<CR><C-w>3",
            EMPTY,
        ),
        ("empty Projects, filter", "A\n", "", FILTERED, EMPTY),
        (
            "a search keeps the entry",
            "A +p\n",
            "",
            "/zzz<CR><C-w>3",
            NO_FOLD,
        ),
        ("a filter keeps the entry", "A +p\n", "", FILTERED, NO_FOLD),
        ("flat project", "A +p\n", "", "<C-w>3", NO_FOLD),
        ("flat context", "A @c\n", "", "<C-w>4", NO_FOLD),
        ("parent unfolded", NESTED, "", "<C-w>3", "l unfold"),
        ("parent folded", NESTED, "", "<C-w>3h", "h fold"),
        ("child folds its parent", NESTED, "", "<C-w>3j", "l unfold"),
        ("context parent", NESTED, "", "<C-w>4", "l unfold"),
        ("context parent folded", NESTED, "", "<C-w>4h", "h fold"),
        ("context child", NESTED, "", "<C-w>4j", "l unfold"),
        ("a switch", NESTED, "", "<C-w>5", NO_FOLD),
        ("a filter", NESTED, "", "<C-w>5G", NO_FOLD),
        (": all commands", "A\n", "", ":", ""),
        (": a command", "A\n", "", ":so", ""),
        (": in the text", "A\n", "", ":so<Tab>", ""),
        (": no command", "A\n", "", ":zzz", "<Tab> next"),
        (": typed back", "A\n", "", ":zzz<BS><BS><BS>", ""),
        ("due prompt", "A\n", "", "gd", "<Tab> next"),
        ("tag prompt", "A\n", "", "gt", ""),
        ("untag, a tag", "A +p\n", "", "gT", ""),
        ("untag, no tag", "A\n", "", "gT", "<Tab> next"),
        ("/", "A\n", "", "/", "<Tab> next"),
        ("/zzz", "A\n", "", "/zzz", "<Tab> next"),
        ("/+, a project", "A +p\n", "", "/+", ""),
        ("/+, no project", "A\n", "", "/+", "<Tab> next"),
        ("field, one candidate", "A +p\n", "", "o+", "<C-n> next"),
        ("field, two candidates", "A +p\nB +q\n", "", "o+", ""),
        ("empty field", "A\n", "", "o", BEFORE),
        ("typed in a field", "A\n", "", "ob", ""),
        ("typed, moved to the start", "A\n", "", "ob<Home>", BEFORE),
        ("typed, deleted", "A\n", "", "ob<C-u>", BEFORE),
        ("i on a task", "Abc\n", "", "i", BEFORE),
        ("a on a task", "Abc\n", "", "a", ""),
        ("a, deleted", "Abc\n", "", "a<C-u>", BEFORE),
        ("I before a priority", "(A) Abc\n", "", "I", BEFORE),
        ("i after a priority, which can go", "(A) Abc\n", "", "i", ""),
        ("I on a done task", DONE, "", "I", BEFORE),
        ("o on a project", "A +p\n", "", "o", BEFORE),
        ("between two ids", "id:a id:b\n", "", "i<Right><Right>", ""),
    ];
    for (name, todo, done, typed, want) in rows {
        let (_dir, model) = hinted((todo, done), None, "", typed);
        assert_eq!(shown_dim(&model, 200), want, "{name}");
    }
}

/// Any writing action listed in `[hints]` dims in the three modes, a prompt
/// by the rule too, and other keys never.
#[test]
fn the_configured_hints_are_dim_by_the_same_rule() {
    const STEPS: &[&str] = &["cycle_priority", "stop", "set_due", "quit"];
    const BOUND: &str = "[normal]\nstop = [\"S\"]\n[visual]\nstop = [\"S\"]\n[detail]\nstop = [\"S\"]\n";
    let rows = [
        ("todo", "normal", "A\n", "", "", "S stop"),
        ("done", "normal", DONE, "", "", "gp priority S stop"),
        (
            "archived",
            "normal",
            "",
            OLD,
            "",
            "gp priority S stop gd due",
        ),
        (
            "detail done",
            "detail",
            DONE,
            "",
            "<C-w>2",
            "gp priority S stop",
        ),
        ("visual done", "visual", DONE, "", "v", "gp priority S stop"),
    ];
    for (name, mode, todo, done, typed, want) in rows {
        let hints = Some((mode, STEPS));
        let (_dir, model) = hinted((todo, done), hints, BOUND, typed);
        assert_eq!(shown_dim(&model, 200), want, "{name}");
    }
}

/// The keys that write no task, listed in `[hints]`, dim by the same rule in
/// every mode that has them.
#[test]
#[expect(clippy::too_many_lines, reason = "a table")]
fn the_configured_hints_of_keys_that_write_no_task_dim_by_the_rule() {
    const NORMAL: &[&str] = &[
        "undo",
        "redo",
        "paste_below",
        "paste_above",
        "repeat",
        "edit_description",
        "edit_append",
        "edit_line_start",
        "change_description",
        "insert_dependent",
        "insert_prerequisite",
        "open_link",
        "yank",
        "move_task_down",
        "move_task_up",
    ];
    const VISUAL: &[&str] = &["yank", "move_task_down", "move_task_up"];
    const PANEL: &[&str] = &[
        "cursor_down",
        "clear_filters",
        "focus_left",
        "focus_down",
        "focus_up",
        "focus_right",
        "focus_list",
        "focus_detail",
        "focus_projects",
        "focus_contexts",
        "focus_filters",
    ];
    const NONE: &str = "u undo <C-r> redo . repeat";
    const TEXT: &[&str] = &[
        "delete_word_back",
        "delete_to_start",
        "delete_char",
        "delete_word_forward",
        "delete_to_end",
    ];
    const AFTER: &str = "<C-d> del char <A-d> del word after <C-k> del to end";
    let fields = [
        ("empty :", "command", ":", format!("{BEFORE} {AFTER}")),
        ("end of :", "command", ":ab", AFTER.to_owned()),
        ("start of /", "command", "/ab<Home>", BEFORE.to_owned()),
        ("inside a prompt", "command", "gdab<Left>", String::new()),
        ("help filter", "command", "?/ab<Home>", BEFORE.to_owned()),
        (
            "theme filter",
            "command",
            ":theme<CR>/ab<Home>",
            BEFORE.to_owned(),
        ),
        ("a space that must stay", "insert", "a", AFTER.to_owned()),
        ("up to an id", "insert", "a<Right>", AFTER.to_owned()),
        ("inside a field", "insert", "oab<Left>", String::new()),
    ];
    for (name, mode, typed, want) in fields {
        let hints = Some((mode, TEXT));
        let (_dir, model) = hinted(("A id:x\n", ""), hints, "", typed);
        assert_eq!(shown_dim(&model, 200), want, "{name}");
    }
    let rows = [
        (
            "todo alone",
            "normal",
            NORMAL,
            "A\n",
            "",
            "",
            format!("{NONE} gx link J move down K move up"),
        ),
        (
            "no task",
            "normal",
            NORMAL,
            "",
            "",
            "",
            format!(
                "{NONE} i edit a append I line start cc change \
                 gx link yy yank J move down K move up"
            ),
        ),
        (
            "after a yank and a write, with a link and a row to pass",
            "normal",
            NORMAL,
            "A https://x.org\nB\n",
            "",
            "yyxk",
            "<C-r> redo K move up".to_owned(),
        ),
        (
            "a due date the task has, which `.` leaves to the core",
            "normal",
            NORMAL,
            "A\nB due:2030-01-01\n",
            "",
            "gd2030-01-01<CR>j",
            "<C-r> redo gx link J move down".to_owned(),
        ),
        (
            "archived",
            "normal",
            NORMAL,
            "",
            OLD,
            "",
            format!(
                "{NONE} i edit a append I line start cc change \
                 go new waiting gO new blocker gx link J move down K move up"
            ),
        ),
        (
            "detail, archived",
            "detail",
            NORMAL,
            "",
            OLD,
            "<C-w>2",
            format!(
                "{NONE} i edit a append I line start cc change \
                 go new waiting gO new blocker <CR> link J move down K move up"
            ),
        ),
        (
            "visual, a row each way",
            "visual",
            VISUAL,
            "A\nB\nC\n",
            "",
            "jv",
            String::new(),
        ),
        (
            "visual, the whole list",
            "visual",
            VISUAL,
            "A\nB\n",
            "",
            "vj",
            "J move down K move up".to_owned(),
        ),
        (
            "visual, sorted",
            "visual",
            VISUAL,
            "A\nB\nC\n",
            "",
            "sjv",
            "J move down K move up".to_owned(),
        ),
        (
            "empty Projects, no filter",
            "panel",
            PANEL,
            "A\n",
            "",
            "<C-w>3",
            "j down gc clear filters <C-w>h left <C-w>k above <C-w>3 projects"
                .to_owned(),
        ),
        (
            "Projects, a filter on",
            "panel",
            PANEL,
            "A +p\n",
            "",
            "<C-w>3x",
            "<C-w>h left <C-w>k above <C-w>3 projects".to_owned(),
        ),
        (
            "Filters, no task",
            "panel",
            PANEL,
            "",
            "",
            "<C-w>5",
            "gc clear filters <C-w>h left <C-w>j below <C-w>2 detail \
             <C-w>5 filters"
                .to_owned(),
        ),
    ];
    for (name, mode, names, todo, done, typed, want) in rows {
        let hints = Some((mode, names));
        let (_dir, model) = hinted((todo, done), hints, "", typed);
        assert_eq!(shown_dim(&model, 300), want, "{name}");
    }
}

/// `.` of a key that takes another word keeps its own.
#[test]
fn a_repeat_keeps_its_own_word() {
    let (_dir, model) = hinted(("A\n", ""), None, "", "x");
    let answer = model.answers()(Action::Repeat);
    let word = label(Mode::Normal, Action::Repeat, "open", answer);
    assert_eq!(word, Some(Action::Repeat.hint()));
    let answer = decide(Action::ToggleComplete, &model, &model.summary(1));
    let word = label(Mode::Normal, Action::ToggleComplete, "open", answer);
    assert_eq!(word, Some("reopen"));
}

/// The status line's text.
fn status_after(model: &Model, width: u16) -> String {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
    model.draw_status(&mut buf, (width, 0));
    buf.content.iter().map(Cell::symbol).collect()
}

/// A count shows from its first digit, and goes with the key that ends it.
#[test]
fn a_count_shows_until_a_key_ends_it() {
    let (_dir, mut model) = hinted(("A\nB\nC\n", ""), None, "", "");
    let plain = status_after(&model, 80);
    assert!(plain.starts_with(" NORMAL  todo.txt"), "{plain}");
    for (typed, want) in [
        ("2", " NORMAL 2  todo.txt"),
        ("5", " NORMAL 25  todo.txt"),
        ("<Esc>", " NORMAL  todo.txt"),
        ("2", " NORMAL 2  todo.txt"),
        ("x", " NORMAL  todo.txt"),
        ("2", " NORMAL 2  todo.txt"),
        ("g", " NORMAL 2  todo.txt"),
        ("<Esc>", " NORMAL  todo.txt"),
        ("2d", " NORMAL 2  todo.txt"),
        ("<Esc>", " NORMAL  todo.txt"),
        ("g2", " NORMAL  todo.txt"),
        ("z2", " NORMAL  todo.txt"),
        ("j20", " NORMAL 20  todo.txt"),
        ("<Esc>", " NORMAL  todo.txt"),
        ("0", " NORMAL  todo.txt"),
        ("j99999", " NORMAL 9999  todo.txt"),
        ("<Esc>", " NORMAL  todo.txt"),
        ("ggvj3", " VISUAL 3  2 selected"),
        ("<Esc>", " VISUAL  2 selected"),
        ("<Esc><C-w>24", " NORMAL 4  todo.txt"),
        ("<Esc><C-w>32", " NORMAL 2  todo.txt"),
    ] {
        keys(&mut model, typed);
        let line = status_after(&model, 80);
        assert!(line.starts_with(want), "{typed}: {line}");
    }
}

/// Whether the status line says `word`, on a screen `width` wide.
fn says_at(model: &Model, width: u16, word: &str) -> bool {
    status_after(model, width).contains(word)
}

/// Whether the status line says `word`, on a screen wide enough for all.
fn says(model: &Model, word: &str) -> bool {
    says_at(model, 200, word)
}

/// `<Esc>` drops a count first, and the hint says so where it goes, in place
/// of what `<Esc>` will do next, and goes with the count.
#[test]
fn a_count_says_esc_cancels() {
    let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", "/A<CR>");
    assert!(says(&model, "<Esc> clear search"));
    keys(&mut model, "2");
    assert!(says(&model, "manual  <Esc> cancel  x done"));
    assert!(!says(&model, "clear search"));
    keys(&mut model, "<Esc>");
    assert!(says(&model, "<Esc> clear search"), "the search stays");

    let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", "vj");
    assert!(says(&model, "<Esc> exit"));
    keys(&mut model, "3");
    assert!(says(&model, "manual  <Esc> cancel  x done"));
    assert!(!says(&model, "<Esc> exit"));
    keys(&mut model, "<Esc>");
    assert_eq!(model.mode(), Mode::Visual);
    assert!(says(&model, "<Esc> exit"));
}

/// With no `<Esc>` hint before, one comes with the count, first.
#[test]
fn a_count_brings_the_esc_hint() {
    for first in ["", "<C-w>2", "<C-w>3"] {
        let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", first);
        assert!(!says(&model, "<Esc>"), "{first}");
        keys(&mut model, "2");
        let line = status_after(&model, 200);
        assert!(line.contains("manual  <Esc> cancel  "), "{first}: {line}");
        keys(&mut model, "<Esc>");
        assert!(!says(&model, "<Esc>"), "{first}");
    }
}

/// A pending key is dropped first too: the hint that shows says so, and none
/// comes where none showed, so that `dd` and `gg` do not move the line.
#[test]
fn a_pending_key_says_esc_cancels() {
    let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", "/A<CR>");
    for first in ["g", "d", "<C-w>"] {
        keys(&mut model, first);
        assert!(says(&model, "<Esc> cancel"), "{first}");
        assert!(!says(&model, "clear search"), "{first}");
        keys(&mut model, "<Esc>");
        assert!(says(&model, "<Esc> clear search"), "{first}: stays");
    }
    let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", "vj");
    keys(&mut model, "g");
    assert!(says(&model, "x done  <Esc> cancel"), "where it stands");
    assert!(!says(&model, "<Esc> exit"));
    keys(&mut model, "<Esc>");
    assert_eq!(model.mode(), Mode::Visual);

    let (_dir, mut model) = hinted(("A\nB\n", ""), None, "", "");
    let plain = status_after(&model, 200);
    keys(&mut model, "g");
    assert_eq!(status_after(&model, 200), plain);
    assert_eq!(model.typed_count(), 0);
}

/// A row with no hints asked for none, and a count adds none to it.
#[test]
fn a_count_adds_no_hint_to_an_empty_row() {
    let (_dir, model) = hinted(("A\n", ""), Some(("normal", &[])), "", "2");
    assert!(!says(&model, "<Esc>"));

    let only = Some(("normal", &["clear_search"][..]));
    let (_dir, model) = hinted(("A\n", ""), only, "", "/A<CR>2");
    assert!(says(&model, "<Esc> cancel"), "its one hint");
}

/// `<Esc> cancel` makes way for the first hint behind it, which it would
/// otherwise take off the line, and says nothing untrue meanwhile.
#[test]
fn cancel_gives_way_to_the_next_hint() {
    let (_dir, model) = hinted((STUCK, ""), None, "", "2");
    assert_eq!(shown_dim(&model, 43), "x done");
    assert_eq!(shown_dim(&model, 56), "x done t start");
    assert!(!says_at(&model, 56, "<Esc>"));
    assert_eq!(shown_dim(&model, 57), "x done");
    assert!(says_at(&model, 57, "<Esc> cancel"));

    let (_dir, model) = hinted(("A\nAlso A\n", ""), None, "", "/A<CR>2");
    assert!(says_at(&model, 50, "x done"));
    assert!(!says_at(&model, 50, "<Esc>"));
}

/// The drop is the matcher's, whatever is bound: a key that is not `<Esc>`
/// keeps its word, one that is `<Esc>` or `<C-c>` under any spelling says
/// cancel, and a binding that a pending key and `<Esc>` make is taken first.
#[test]
fn cancel_follows_the_bindings() {
    let moved = "[normal]\nclear_search = [\"<C-l>\"]\n";
    let (_dir, model) = hinted(("A\nB\n", ""), None, moved, "/A<CR>2");
    assert!(says(&model, "manual  <Esc> cancel  <C-l> clear search"));

    let others = [("<esc>", "<esc> cancel"), ("<C-c>", "<C-c> cancel")];
    for (key, want) in others {
        let keymap = format!("[normal]\nclear_search = [\"{key}\"]\n");
        let (_dir, model) = hinted(("A\n", ""), None, &keymap, "/A<CR>2");
        assert!(says(&model, &format!("manual  {want}  x done")), "{key}");
        assert!(!says(&model, "clear search"), "{key}");
        let line = status_after(&model, 200);
        assert_eq!(line.matches("cancel").count(), 1, "{key}: {line}");
    }

    let longer = "[normal]\nclear_search = [\"<Esc>\", \"g<Esc>\"]\n";
    let (_dir, mut model) = hinted(("A\n", ""), None, longer, "/A<CR>g");
    assert!(says(&model, "<Esc> clear search"));
    keys(&mut model, "<Esc>");
    assert!(!says(&model, "clear search"));
}

/// A key the user gave `stop`: its hint reads the typed count, as the key
/// does.
#[test]
fn a_bound_stop_follows_a_count() {
    let list = ("Open\nDoing started:2026-09-29T0900Z\n", "");
    let bound = "[normal]\nstop = [\"S\"]\n";
    let (dir, mut model) =
        hinted(list, Some(("normal", &["stop"])), bound, "");
    let file = dir.path().join("todo.txt");
    assert_eq!(shown_dim(&model, 200), "S stop");
    keys(&mut model, "2");
    assert_eq!(shown_dim(&model, 200), "");
    let before = fs::read(&file).unwrap_or_else(|e| panic!("{e}"));
    keys(&mut model, "S");
    assert_ne!(fs::read(&file).unwrap_or_else(|e| panic!("{e}")), before);
}

/// A digit is no count in a field, a line or a picker.
#[test]
fn a_digit_in_a_field_shows_no_count() {
    for typed in ["a2", ":2", "/2", "gd2"] {
        let (_dir, model) = hinted(("A\n", ""), None, "", typed);
        assert_eq!(model.typed_count(), 0, "{typed}");
        let line = status_after(&model, 80);
        let chip = line.trim_start().split(' ').nth(1);
        assert_eq!(chip, Some(""), "{typed}: {line}");
    }
}

/// A count on a window too narrow for it is cut with the mode.
#[test]
fn a_count_is_cut_with_the_mode() {
    let (_dir, model) = hinted(("A\n", ""), None, "", "9999");
    for (width, want) in [(10, " NORMAL 99"), (4, " NOR")] {
        assert_eq!(status_after(&model, width), want);
    }
}

/// The hints follow the digits: dim over the blocked task, the word over the
/// open one, and back with `<Esc>`.
#[test]
fn the_hints_follow_a_count() {
    let (_dir, mut model) =
        hinted(("Blocker id:b\nStuck dep:b\n", ""), None, "", "");
    assert!(!shown_dim(&model, 200).contains("x done"));
    keys(&mut model, "2");
    assert!(shown_dim(&model, 200).contains("x done"));
    keys(&mut model, "<Esc>");
    assert!(!shown_dim(&model, 200).contains("x done"));
    keys(&mut model, "<C-w>2");
    assert!(!shown_dim(&model, 200).contains("x done"));
    keys(&mut model, "2");
    assert!(shown_dim(&model, 200).contains("x done"));

    let (_dir, mut model) =
        hinted(("x 2026-09-01 Done\nOpen\n", ""), None, "", "");
    assert!(status_after(&model, 200).contains("x reopen"));
    keys(&mut model, "2");
    assert!(status_after(&model, 200).contains("x done"));
    keys(&mut model, "<Esc>");
    assert!(status_after(&model, 200).contains("x reopen"));
}

/// A selection overrides a count over the tasks.
#[test]
fn a_selection_overrides_a_count_over_the_hints() {
    let list = ("A\nBlocker id:b\nStuck dep:b\n", "");
    let (_dir, model) = hinted(list, None, "", "vj3");
    assert!(!shown_dim(&model, 200).contains("x done"));
}

/// No other mode dims by the rule: help and a picker move whatever lies
/// behind, and it refuses no key of a field with text on both sides.
#[test]
fn other_modes_never_dim() {
    let opened = [
        ("", "<C-w>3?", Mode::Help),
        ("", "<C-w>3:theme<CR>", Mode::Picker),
        ("", "?", Mode::Help),
        ("", ":theme<CR>", Mode::Picker),
        ("", "oab<Left>", Mode::Insert),
        ("", ":so<Left>", Mode::Command),
        ("A +p\nB +q\n", "o+", Mode::Completion),
    ];
    for (todo, typed, mode) in opened {
        let (_dir, model) = hinted((todo, ""), None, "", typed);
        assert_eq!(model.key_mode(), mode, "{typed}");
        let answer = model.answers();
        for action in Action::ALL {
            let keyed = !model.deps.keys.keys(mode, *action).is_empty();
            assert!(
                !keyed || answer(*action) == Answer::Applies,
                "{typed} {action:?}"
            );
        }
    }
}

/// A dim hint keeps its place: the same line at every width, lit or dim.
#[test]
fn a_dim_hint_shifts_nothing() {
    let (_dir, lit) = hinted(("Blocker id:b\nStuck\n", ""), None, "", "j");
    let (_dir2, dimmed) = hinted((STUCK, ""), None, "", "j");
    assert_eq!(shown_dim(&lit, 200), "u undo");
    assert_eq!(shown_dim(&dimmed, 200), "x done t start u undo");
    for width in 1..=120 {
        let row = |model: &Model| {
            let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
            model.draw_status(&mut buf, (width, 0));
            let cells = buf.content.iter().map(Cell::symbol);
            cells.collect::<String>()
        };
        assert_eq!(row(&lit), row(&dimmed), "{width}");
    }
}

/// `LSTY_BENCH_DRAWS` draws of the status line over a whole large list, after
/// `typed`, so that two runs under `perf stat` give the instructions of one.
/// `cargo test -p lsty-tui --lib --release -- --ignored status_line_over`.
fn draws_after(typed: &str, line: impl Fn(usize) -> String) {
    #[expect(clippy::disallowed_methods, reason = "a test run by hand")]
    let draws = std::env::var("LSTY_BENCH_DRAWS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let lines: String = (0..10_000).map(line).collect();
    let dir = workdir(lines);
    let mut model = started(deps(dir.path(), alone()));
    press(&mut model, typed);
    assert!(model.rows_of(model.typed_count()).len() >= 9999);
    let mut buf = Buffer::empty(Rect::new(0, 0, 200, 1));
    for _ in 0..draws {
        model.draw_status(&mut buf, (200, 0));
    }
}

fn draws_over_all_of(line: impl Fn(usize) -> String) {
    draws_after("ggVG", line);
}

/// The status line after `tabs` presses of `<Tab>` in the detail panel: `key`
/// has the word of the place it `leaves` for and goes there, or has its own
/// word and moves the link.
fn tab_says(
    (todo, tabs, show_sidebar): (&str, usize, bool),
    key: &str,
    leaves: Option<(&str, Focus)>,
) {
    let config = Config {
        show_sidebar,
        hints: table("detail", &["next_link", "prev_link"]),
        ..plain()
    };
    let (_dir, mut model) = workdir_model(todo, config);
    model.update(Msg::Resize(130, 30));
    keys(&mut model, "<C-w>2");
    for _ in 0..tabs {
        keys(&mut model, "<Tab>");
    }
    let what = format!("{todo:?} at {tabs} {key} {show_sidebar}");
    let (own, step) = if key == "<Tab>" {
        ("next link", 1)
    } else {
        ("prev link", -1)
    };
    let word = leaves.map_or(own, |(word, _)| word);
    let status = status_after(&model, 130);
    assert!(
        status.contains(&format!("{key} {word}")),
        "{what}: {status}"
    );
    let link = model.detail_link().saturating_add_signed(step);
    keys(&mut model, key);
    let place = leaves.map_or(Focus::Detail, |(_, place)| place);
    assert_eq!(model.focus, place, "{what}");
    if leaves.is_none() {
        assert_eq!(model.detail_link(), link, "{what}");
    }
}

/// With the sidebar beside the list or coming over it.
#[test]
fn the_tab_words_say_what_the_keys_do() {
    const LINKS: &str = "A file:a.md file:b.md file:c.md\n";
    const PROJECTS: Option<(&str, Focus)> =
        Some(("projects", Focus::Projects));
    const LIST: Option<(&str, Focus)> = Some(("list", Focus::List));
    let cases = [
        ("A\n", 0, PROJECTS, LIST),
        (LINKS, 0, None, LIST),
        (LINKS, 1, None, None),
        (LINKS, 2, PROJECTS, None),
    ];
    for (todo, tabs, next, prev) in cases {
        for show_sidebar in [true, false] {
            tab_says((todo, tabs, show_sidebar), "<Tab>", next);
            tab_says((todo, tabs, show_sidebar), "<S-Tab>", prev);
        }
    }
}

/// On an empty list the panel gives the keyboard back, and its hints go.
#[test]
fn an_empty_list_shows_the_hints_of_the_list() {
    let (_dir, model) = hinted(("", ""), None, "", "<C-w>2");
    let status = status_after(&model, 130);
    assert!(status.contains("/ search"), "{status}");
    assert!(!status.contains("<CR> link"), "{status}");
}

#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_open_tasks() {
    draws_over_all_of(|i| format!("Task {i} +p{} id:i{i}\n", i % 7));
}

/// `9999` typed, over a list as long: a summary over them and one over the
/// cursor task on each draw.
#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_a_count() {
    draws_after("9999", |i| format!("Task {i} +p{} id:i{i}\n", i % 7));
}

/// The links of the cursor task are found on each draw.
#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_long_tasks() {
    draws_over_all_of(|i| format!("Task {i} {}\n", "word ".repeat(200)));
}

#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_wont_do_tasks() {
    draws_over_all_of(|i| {
        format!("x 2026-09-01 Task {i} +p{} wontdo:true\n", i % 7)
    });
}
