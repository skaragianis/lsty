use std::{collections::BTreeMap, fs};

use lsty_core::{
    config::{Config, Hidden},
    keys::{Action, Mode},
    theme::Slot,
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
};
use tempfile::TempDir;

use crate::{
    hints::{defaults, warnings},
    model::Model,
    msg::Msg,
    press::{Answer, decide},
    test_util::{alone, deps, keys, press, screen, started, workdir},
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

/// Each state of the default hints: the files, what is typed on them, and
/// the hints that are dim.
#[test]
fn the_default_hints_are_dim_when_their_key_would_change_nothing() {
    let rows = [
        ("no task", "", "", "", ""),
        ("todo", "A\n", "", "", ""),
        ("doing", "A started:2026-09-29T0800Z\n", "", "", ""),
        ("blocked", STUCK, "", "j", "x done t start"),
        ("blocked doing", STUCK_DOING, "", "j", "x done"),
        ("done", DONE, "", "", "T wont"),
        ("won't do", SKIP, "", "", ""),
        ("archived", "", OLD, "", "T wont dd delete"),
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
            "T wont",
        ),
        ("visual archived", "", OLD, "v", "T wont d delete"),
        (
            "visual open and archived",
            "A\n",
            OLD,
            "vj",
            "x done t start T wont d delete",
        ),
        ("detail todo", "A\n", "", "<C-w>2", ""),
        ("detail blocked", STUCK, "", "j<C-w>2", "x done"),
        ("detail archived", "", OLD, "<C-w>2", "dd delete"),
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

/// No other mode dims: none of its keys is an action the rule can refuse.
#[test]
fn other_modes_never_dim() {
    let (_dir, model) = hinted(("", OLD), None, "", "");
    let summary = model.summary(1);
    for mode in Mode::ALL {
        if matches!(mode, Mode::Normal | Mode::Visual | Mode::Detail) {
            continue;
        }
        for action in Action::ALL {
            let keyed = !model.deps.keys.keys(mode, *action).is_empty();
            let answer = decide(*action, &summary);
            assert!(!keyed || answer == Answer::Applies, "{action:?}");
        }
    }
}

/// A dim hint keeps its place: the same line at every width, lit or dim.
#[test]
fn a_dim_hint_shifts_nothing() {
    let (_dir, lit) = hinted(("Blocker id:b\nStuck\n", ""), None, "", "j");
    let (_dir2, dimmed) = hinted((STUCK, ""), None, "", "j");
    assert_eq!(shown_dim(&lit, 200), "");
    assert_eq!(shown_dim(&dimmed, 200), "x done t start");
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

/// `LSTY_BENCH_DRAWS` draws of the status line with a whole large list
/// selected, so that two runs under `perf stat` give the instructions of one.
/// `cargo test -p lsty-tui --lib --release -- --ignored status_line_over`.
fn draws_over_all_of(line: impl Fn(usize) -> String) {
    #[expect(clippy::disallowed_methods, reason = "a test run by hand")]
    let draws = std::env::var("LSTY_BENCH_DRAWS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let lines: String = (0..10_000).map(line).collect();
    let dir = workdir(lines);
    let mut model = started(deps(dir.path(), alone()));
    press(&mut model, "ggVG");
    assert_eq!(model.rows_of(1).len(), 10_000);
    let mut buf = Buffer::empty(Rect::new(0, 0, 200, 1));
    for _ in 0..draws {
        model.draw_status(&mut buf, (200, 0));
    }
}

#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_open_tasks() {
    draws_over_all_of(|i| format!("Task {i} +p{} id:i{i}\n", i % 7));
}

#[test]
#[ignore = "timing, run by hand"]
fn status_line_over_wont_do_tasks() {
    draws_over_all_of(|i| {
        format!("x 2026-09-01 Task {i} +p{} wontdo:true\n", i % 7)
    });
}
