use std::{fs, path::Path};

use super::{
    Keymap,
    actions::{Action, Mode},
    defaults::defaults,
    load,
    matcher::Binding,
};
use crate::config::Warning;

fn load_text(
    text: impl AsRef<[u8]>,
    no_config: bool,
    leader: &str,
) -> (Keymap, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("keys.toml");
    fs::write(&file, text).unwrap();
    let (keymap, warnings) = load(&file, Path::new("/"), no_config, leader);
    (keymap, messages(&warnings))
}

fn messages(warnings: &[Warning]) -> Vec<String> {
    warnings
        .iter()
        .map(|w| format!("{}|{}", w.source, w.message))
        .collect()
}

fn keys_warning(msg: &str) -> String {
    format!("keys.toml|{msg}")
}

#[test]
fn defaults_load_without_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("none.toml");
    let (keymap, warnings) = load(&missing, Path::new("/"), false, "<Space>");
    assert_eq!(warnings.len(), 0, "28.8 {warnings:?}");
    for mode in Mode::ALL {
        for &(action, written) in defaults(mode) {
            assert_eq!(
                keymap.keys(mode, action),
                written,
                "28.8 [{}] {action}",
                mode.as_str()
            );
        }
    }
    assert_eq!(
        keymap.keys(Mode::Normal, Action::Quit),
        ["q", "ZZ"],
        "28.35"
    );
}

#[test]
fn relative_path_is_taken_from_cwd() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("k.toml"), "[normal]\nrepeat = []").unwrap();
    let (keymap, warnings) =
        load(Path::new("k.toml"), dir.path(), false, "<Space>");
    assert_eq!(warnings.len(), 0);
    assert_eq!(keymap.keys(Mode::Normal, Action::Repeat).len(), 0);
}

#[test]
fn unreadable_file_is_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let (keymap, warnings) =
        load(dir.path(), Path::new("/"), false, "<Space>");
    let got = messages(&warnings);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].starts_with("keys.toml|read "), "{got:?}");
    assert_eq!(keymap.keys(Mode::Normal, Action::Delete), ["dd"]);
}

#[test]
fn defaults_have_no_conflicts() {
    for mode in Mode::ALL {
        let all: Vec<Vec<_>> = defaults(mode)
            .iter()
            .flat_map(|&(_, written)| written)
            .map(|w| super::parse_keys(w, None).unwrap())
            .collect();
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                let clash = i != j && b.starts_with(a);
                assert!(
                    !clash,
                    "28.9 [{}] {a:?} clashes with {b:?}",
                    mode.as_str()
                );
            }
        }
    }
}

struct Row {
    case: &'static str,
    text: &'static str,
    no_config: bool,
    leader: &'static str,
    mode: Mode,
    action: Action,
    want: &'static [&'static str],
    warnings: &'static [&'static str],
}

const BASE: Row = Row {
    case: "",
    text: "",
    no_config: false,
    leader: "<Space>",
    mode: Mode::Normal,
    action: Action::Delete,
    want: &[],
    warnings: &[],
};

const X_WARNING: &str =
    r#"[normal] "x" is bound to toggle_complete and delete; delete wins"#;
const Y_WARNING: &str =
    r#"[normal] "Y" is bound to yank and delete; delete wins"#;
const G_WARNING: &str = r#"[normal] "g" is hidden by "gg""#;

const ROWS: &[Row] = &[
    Row {
        case: "28.10 replace",
        text: "[normal]\ndelete = [\"X\"]",
        want: &["X"],
        ..BASE
    },
    Row {
        case: "28.10 others keep",
        text: "[normal]\ndelete = [\"X\"]",
        action: Action::Yank,
        want: &["yy"],
        ..BASE
    },
    Row {
        case: "28.11 unbind",
        text: "[normal]\nrepeat = []",
        action: Action::Repeat,
        ..BASE
    },
    Row {
        case: "28.12 default loses",
        text: "[normal]\ndelete = [\"x\"]",
        action: Action::ToggleComplete,
        warnings: &[X_WARNING],
        ..BASE
    },
    Row {
        case: "28.12 user wins",
        text: "[normal]\ndelete = [\"x\"]",
        want: &["x"],
        warnings: &[X_WARNING],
        ..BASE
    },
    Row {
        case: "28.13 later wins",
        text: "[normal]\nyank = [\"Y\"]\ndelete = [\"Y\"]",
        want: &["Y"],
        warnings: &[Y_WARNING],
        ..BASE
    },
    Row {
        case: "28.13 earlier loses",
        text: "[normal]\nyank = [\"Y\"]\ndelete = [\"Y\"]",
        action: Action::Yank,
        warnings: &[Y_WARNING],
        ..BASE
    },
    Row {
        case: "28.14 hidden",
        text: "[normal]\nhelp = [\"g\"]",
        action: Action::Help,
        warnings: &[G_WARNING],
        ..BASE
    },
    Row {
        case: "28.14 longer stays",
        text: "[normal]\nhelp = [\"g\"]",
        action: Action::GoTop,
        want: &["gg"],
        warnings: &[G_WARNING],
        ..BASE
    },
    Row {
        case: "28.15 unknown action",
        text: "[normal]\nfrobnicate = [\"f\"]\n[insert]\nyank = [\"y\"]",
        want: &["dd"],
        warnings: &[
            r#"[normal] unknown action "frobnicate""#,
            r#"[insert] unknown action "yank""#,
        ],
        ..BASE
    },
    Row {
        case: "28.16 unknown mode",
        text: "[vizual]\ndelete = [\"d\"]",
        mode: Mode::Visual,
        want: &["d"],
        warnings: &[r#"unknown mode "[vizual]""#],
        ..BASE
    },
    Row {
        case: "unknown mode dotted key",
        text: "vizual.delete = [\"x\"]\nvizual.yank = [\"y\"]",
        mode: Mode::Visual,
        want: &["d"],
        warnings: &[r#"unknown mode "[vizual]""#],
        ..BASE
    },
    Row {
        case: "mode that is not a table",
        text: "normal = [\"x\"]",
        want: &["dd"],
        warnings: &["[normal]: expected a table"],
        ..BASE
    },
    Row {
        case: "28.17 string value",
        text: "[normal]\ndelete = \"dd\"",
        want: &["dd"],
        warnings: &["[normal] delete: expected a list of keys"],
        ..BASE
    },
    Row {
        case: "28.17 mixed list",
        text: "[normal]\ndelete = [\"dd\", 5]",
        want: &["dd"],
        warnings: &["[normal] delete: expected a list of keys"],
        ..BASE
    },
    Row {
        case: "28.18 bad key dropped",
        text: "[normal]\ndelete = [\"dd\", \"Down\"]",
        want: &["dd"],
        warnings: &[
            r#"[normal] delete: "Down" reads as the keys D-o-w-n; write "<Down>""#,
        ],
        ..BASE
    },
    Row {
        case: "28.19 digit start",
        text: "[normal]\ndelete = [\"3d\"]",
        want: &["dd"],
        warnings: &[
            r#"[normal] delete: invalid key "3d": starts with a digit"#,
        ],
        ..BASE
    },
    Row {
        case: "28.21 no config",
        text: "[normal]\ndelete = [\"X\"]",
        no_config: true,
        want: &["dd"],
        ..BASE
    },
    Row {
        case: "28.22 bad leader",
        leader: "gg",
        want: &["dd"],
        warnings: &["config.toml|leader: \"gg\" is not one key"],
        ..BASE
    },
    Row {
        case: "28.40 picker keys",
        text: "[picker]\naccept = [\"o\", \"<CR>\"]",
        mode: Mode::Picker,
        action: Action::Accept,
        want: &["o", "<CR>"],
        ..BASE
    },
    Row {
        case: "28.40 picker digit",
        text: "[picker]\naccept = [\"3\"]",
        mode: Mode::Picker,
        action: Action::Accept,
        want: &["<CR>"],
        warnings: &[
            r#"[picker] accept: invalid key "3": starts with a digit"#,
        ],
        ..BASE
    },
    Row {
        case: "panel digit",
        text: "[panel]\ntoggle_entry = [\"3\"]",
        mode: Mode::Panel,
        action: Action::ToggleEntry,
        want: &["<Space>"],
        warnings: &[
            r#"[panel] toggle_entry: invalid key "3": starts with a digit"#,
        ],
        ..BASE
    },
    Row {
        case: "insert may start with a digit",
        text: "[insert]\ncommit = [\"3x\"]",
        mode: Mode::Insert,
        action: Action::Commit,
        want: &["3x"],
        ..BASE
    },
    Row {
        case: "28.3 leader binding",
        text: "[normal]\ndelete = [\"<leader>d\"]",
        leader: ",",
        want: &["<leader>d"],
        ..BASE
    },
    Row {
        case: "all keys invalid keeps the default",
        text: "[normal]\ndelete = [\"Down\"]",
        want: &["dd"],
        warnings: &[
            r#"[normal] delete: "Down" reads as the keys D-o-w-n; write "<Down>""#,
        ],
        ..BASE
    },
];

#[test]
fn merge_rows() {
    for row in ROWS {
        let (keymap, warnings) =
            load_text(row.text, row.no_config, row.leader);
        assert_eq!(
            keymap.keys(row.mode, row.action),
            row.want,
            "{}",
            row.case
        );
        let want: Vec<String> = row
            .warnings
            .iter()
            .map(|w| {
                if w.contains('|') {
                    (*w).to_owned()
                } else {
                    keys_warning(w)
                }
            })
            .collect();
        assert_eq!(warnings, want, "{}", row.case);
    }
}

#[test]
fn panel_leader_conflicts_with_toggle_entry() {
    let (keymap, warnings) =
        load_text("[panel]\nfold = [\"<leader>\"]", false, "<Space>");
    let want = [keys_warning(
        r#"[panel] "<leader>" is bound to toggle_entry and fold; fold wins"#,
    )];
    assert_eq!(warnings, want, "28.12");
    assert_eq!(keymap.keys(Mode::Panel, Action::ToggleEntry).len(), 0);
}

#[test]
fn syntax_error_gives_defaults_and_the_line() {
    let text = "[normal]\ndelete = [\"x\"]\nbad = = 1\n";
    let (keymap, warnings) = load_text(text, false, "<Space>");
    assert_eq!(warnings.len(), 1, "28.20 {warnings:?}");
    assert!(warnings[0].starts_with("keys.toml|line 3:"), "{warnings:?}");
    assert_eq!(keymap.keys(Mode::Normal, Action::Delete), ["dd"], "28.20");
}

#[test]
fn invalid_utf8_gives_defaults_and_one_warning() {
    let (keymap, warnings) = load_text(b"[normal]\n\xff", false, "<Space>");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].starts_with("keys.toml|line 2:"), "{warnings:?}");
    assert_eq!(keymap.keys(Mode::Normal, Action::Delete), ["dd"]);
}

#[test]
fn warnings_follow_file_order() {
    let text = "[vizual]\na = [\"x\"]\n[normal]\nfrobnicate = [\"f\"]\n\
                [insert]\nyank = [\"y\"]\n";
    let (_, warnings) = load_text(text, false, "<Space>");
    assert_eq!(
        warnings,
        [
            keys_warning(r#"unknown mode "[vizual]""#),
            keys_warning(r#"[normal] unknown action "frobnicate""#),
            keys_warning(r#"[insert] unknown action "yank""#),
        ]
    );
}

#[test]
fn bindings_of_a_mode_are_unique() {
    let (keymap, _) = load_text("[normal]\nhelp = [\"g\"]", false, "<Space>");
    for mode in Mode::ALL {
        let table: &[Binding] = keymap.table(mode);
        for (i, a) in table.iter().enumerate() {
            assert!(table[..i].iter().all(|b| b.strokes != a.strokes));
        }
    }
}

#[test]
fn leader_reaches_the_bound_keys() {
    let (keymap, warnings) =
        load_text("[normal]\ndelete = [\"<leader>d\"]", false, ",");
    assert_eq!(warnings.len(), 0, "28.3 {warnings:?}");
    let mut m = keymap.matcher(Mode::Normal);
    let comma = super::parse_keys(",", None).unwrap()[0];
    let d = super::parse_keys("d", None).unwrap()[0];
    m.feed(comma);
    let fed = m.feed(d);
    assert_eq!(
        fed.outcome,
        super::Outcome::Matched {
            action: Action::Delete,
            count: None
        },
        "28.3"
    );
}

const RANGE_ROWS: &[(&str, &str, &str)] = &[
    (
        "integer in a list",
        "[normal]\ndelete = [99999999999999999999]\nyank = [\"Y\"]",
        "keys.toml|line 2: 99999999999999999999 is out of range for int64",
    ),
    (
        "integer of an unknown key",
        "[normal]\ndelete = [\"X\"]\nfoo = 99999999999999999999",
        "keys.toml|line 3: 99999999999999999999 is out of range for int64",
    ),
    (
        "integer in an unknown mode",
        "[vizual]\na = { b = [-99999999999999999999] }",
        "keys.toml|line 2: -99999999999999999999 is out of range for int64",
    ),
    (
        "float",
        "[normal]\nf = 1e999",
        "keys.toml|line 2: 1e999 is out of range for float64",
    ),
    (
        "float in a list",
        "[normal]\ndelete = [1e999]",
        "keys.toml|line 2: 1e999 is out of range for float64",
    ),
];

#[test]
fn out_of_range_numbers_give_one_warning_and_the_defaults() {
    for &(case, text, want) in RANGE_ROWS {
        let (keymap, warnings) = load_text(text, false, "<Space>");
        assert_eq!(warnings, [want], "{case}");
        assert_eq!(
            keymap.keys(Mode::Normal, Action::Delete),
            ["dd"],
            "{case}"
        );
        assert_eq!(keymap.keys(Mode::Normal, Action::Yank), ["yy"], "{case}");
    }
}

#[test]
fn infinite_floats_are_not_out_of_range() {
    let (_, warnings) =
        load_text("[normal]\nf = inf\ng = nan", false, "<Space>");
    assert_eq!(
        warnings,
        [
            keys_warning(r#"[normal] unknown action "f""#),
            keys_warning(r#"[normal] unknown action "g""#)
        ]
    );
}

#[test]
fn array_of_tables_warns_for_each_key() {
    let (keymap, warnings) = load_text(
        "[[normal]]\ndelete = [\"X\"]\nyank = [\"Y\"]",
        false,
        "<Space>",
    );
    let one = keys_warning("[normal]: expected a table");
    assert_eq!(warnings, [one.clone(), one.clone(), one]);
    assert_eq!(keymap.keys(Mode::Normal, Action::Delete), ["dd"]);
}

#[test]
fn an_empty_array_is_no_table_either() {
    let (keymap, warnings) = load_text("normal = []", false, "<Space>");
    assert_eq!(warnings, [keys_warning("[normal]: expected a table")]);
    assert_eq!(keymap.keys(Mode::Normal, Action::Delete), ["dd"]);
}

#[test]
fn keys_of_tables_nested_in_arrays_are_counted_too() {
    let (_, warnings) =
        load_text("[[normal]]\nx = [{ a = 1, b = 2 }]", false, "<Space>");
    // The table, its key `x`, and the keys `a` and `b` of the table in
    // the array.
    assert_eq!(warnings.len(), 4, "{warnings:?}");
}
