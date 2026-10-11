use std::{collections::BTreeMap, fs};

use lsty_core::keys::Mode;

use crate::{
    hints::{defaults, warnings},
    test_util::{alone, deps, press, screen, started, workdir},
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
    press(&mut model, "o");
    let lines = screen(&model, width, 12);
    lines
        .last()
        .map_or_else(String::new, |l| l.trim().to_owned())
}

#[test]
fn the_insert_row() {
    let remap =
        "[insert]\ndelete_word_back = [\"<C-h>\"]\ndelete_to_start = []\n";
    let all = "<CR> commit  <C-w> del word  <C-u> del to start";
    let rows = [
        ("default", "", &[][..], 80, all),
        (
            "no room for the last",
            "",
            &[],
            60,
            "<CR> commit  <C-w> del word",
        ),
        ("room for the first only", "", &[], 40, "<CR> commit"),
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
