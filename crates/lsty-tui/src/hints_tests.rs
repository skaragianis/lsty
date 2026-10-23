use std::{collections::BTreeMap, fs};

use lsty_core::keys::Mode;

use crate::{
    hints::{defaults, warnings},
    msg::Msg,
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
