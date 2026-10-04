use std::collections::BTreeMap;

use lsty_core::keys::Mode;

use crate::hints::{defaults, key_label, warnings};

#[test]
fn key_labels() {
    const ROWS: [(&str, &str); 13] = [
        ("x", "x"),
        ("gg", "gg"),
        ("<CR>", "Enter"),
        ("<Esc>", "Esc"),
        ("<Tab>", "Tab"),
        ("<Space>", "Space"),
        ("<C-r>", "Ctrl-r"),
        ("<A-x>", "Alt-x"),
        ("<S-Tab>", "Shift-Tab"),
        ("<Up>", "↑"),
        ("<Down>", "↓"),
        ("<C-w>h", "Ctrl-w h"),
        ("<lt>", "<"),
    ];
    for (written, want) in ROWS {
        assert_eq!(key_label(written), want, "{written}");
    }
}

type Hints = BTreeMap<String, Vec<String>>;

fn table(mode: &str, names: &[&str]) -> BTreeMap<String, Vec<String>> {
    let names = names.iter().map(|n| (*n).to_owned()).collect();
    BTreeMap::from([(mode.to_owned(), names)])
}

#[test]
fn hint_warnings() {
    let rows: [(Hints, Vec<&str>); 3] = [
        (
            table("normal", &["help", "frob"]),
            vec![r#"hints.normal: unknown action "frob""#],
        ),
        (table("frob", &["help"]), vec!["hints.frob: unknown mode"]),
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
