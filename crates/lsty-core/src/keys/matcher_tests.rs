use std::fs;
use std::path::Path;

use super::matcher::Outcome;
use super::{Fed, Keymap, Matcher, Mode, Stroke, load, parse_keys};

fn keymap_for(text: &str) -> Keymap {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("keys.toml");
    fs::write(&file, text).unwrap();
    load(&file, Path::new("/"), false, "<Space>").0
}

fn strokes(written: &str) -> Vec<Stroke> {
    parse_keys(written, Some(Stroke::char(' '))).unwrap()
}

/// What a stroke should give: the outcome and the text to type first.
struct Want {
    outcome: Outcome,
    text: &'static str,
}

const fn matched(action: &'static str, count: u32) -> Want {
    Want {
        outcome: Outcome::Matched { action, count },
        text: "",
    }
}

const fn typed(action: &'static str, text: &'static str) -> Want {
    Want {
        outcome: Outcome::Matched { action, count: 0 },
        text,
    }
}

const PENDING: Want = Want {
    outcome: Outcome::Pending,
    text: "",
};

const fn no_match(text: &'static str) -> Want {
    Want {
        outcome: Outcome::NoMatch,
        text,
    }
}

struct Row {
    case: &'static str,
    mode: Mode,
    file: &'static str,
    keys: &'static [&'static str],
    want: &'static [Want],
}

const JK: &str = "[insert]\ncommit = [\"jk\"]";
const JK_ENTER_ESC: &str = "[insert]\ncommit = [\"jk\", \"<CR>\", \"<Esc>\"]";

const ROWS: &[Row] = &[
    Row {
        case: "28.23 j",
        mode: Mode::Normal,
        file: "",
        keys: &["j"],
        want: &[matched("cursor_down", 0)],
    },
    Row {
        case: "28.24 gg",
        mode: Mode::Normal,
        file: "",
        keys: &["g", "g"],
        want: &[PENDING, matched("go_top", 0)],
    },
    Row {
        case: "28.25 gz then j",
        mode: Mode::Normal,
        file: "",
        keys: &["g", "z", "j"],
        want: &[PENDING, no_match("z"), matched("cursor_down", 0)],
    },
    Row {
        case: "28.26 5j",
        mode: Mode::Normal,
        file: "",
        keys: &["5", "j"],
        want: &[PENDING, matched("cursor_down", 5)],
    },
    Row {
        case: "28.27 10dd",
        mode: Mode::Normal,
        file: "",
        keys: &["1", "0", "d", "d"],
        want: &[PENDING, PENDING, PENDING, matched("delete", 10)],
    },
    Row {
        case: "28.28 count then esc",
        mode: Mode::Normal,
        file: "",
        keys: &["3", "<Esc>", "j"],
        want: &[PENDING, no_match(""), matched("cursor_down", 0)],
    },
    Row {
        case: "28.29 d esc d",
        mode: Mode::Normal,
        file: "",
        keys: &["d", "<Esc>", "d"],
        want: &[PENDING, no_match(""), PENDING],
    },
    Row {
        case: "28.30 C-w h",
        mode: Mode::Normal,
        file: "",
        keys: &["<C-w>", "h"],
        want: &[PENDING, matched("focus_left", 0)],
    },
    Row {
        case: "28.31 3 colon",
        mode: Mode::Normal,
        file: "",
        keys: &["3", ":"],
        want: &[PENDING, matched("command_line", 0)],
    },
    Row {
        case: "28.32 insert a",
        mode: Mode::Insert,
        file: "",
        keys: &["a"],
        want: &[no_match("a")],
    },
    Row {
        case: "28.33 insert 5",
        mode: Mode::Insert,
        file: "",
        keys: &["5"],
        want: &[no_match("5")],
    },
    Row {
        case: "28.37 2gx",
        mode: Mode::Normal,
        file: "",
        keys: &["2", "g", "x"],
        want: &[PENDING, PENDING, matched("open_link", 2)],
    },
    Row {
        case: "28.37 3 dot",
        mode: Mode::Normal,
        file: "",
        keys: &["3", "."],
        want: &[PENDING, matched("repeat", 3)],
    },
    Row {
        case: "28.38 12G",
        mode: Mode::Normal,
        file: "",
        keys: &["1", "2", "G"],
        want: &[PENDING, PENDING, matched("go_bottom", 12)],
    },
    Row {
        case: "28.38 3p",
        mode: Mode::Normal,
        file: "",
        keys: &["3", "p"],
        want: &[PENDING, matched("paste_below", 0)],
    },
    Row {
        case: "28.39 picker",
        mode: Mode::Picker,
        file: "",
        keys: &["j", "<CR>", "3", "q"],
        want: &[
            matched("cursor_down", 0),
            matched("accept", 0),
            no_match("3"),
            matched("close", 0),
        ],
    },
    Row {
        case: "panel count",
        mode: Mode::Panel,
        file: "",
        keys: &["3", "j"],
        want: &[PENDING, matched("cursor_down", 3)],
    },
    Row {
        case: "visual count",
        mode: Mode::Visual,
        file: "",
        keys: &["3", "j"],
        want: &[PENDING, matched("cursor_down", 3)],
    },
    Row {
        case: "visual esc",
        mode: Mode::Visual,
        file: "",
        keys: &["<Esc>"],
        want: &[matched("exit_visual", 0)],
    },
    Row {
        case: "insert failed sequence returns pending keys",
        mode: Mode::Insert,
        file: JK,
        keys: &["j", "a"],
        want: &[PENDING, no_match("ja")],
    },
    Row {
        case: "insert jk then enter",
        mode: Mode::Insert,
        file: JK_ENTER_ESC,
        keys: &["j", "<CR>"],
        want: &[PENDING, typed("commit", "j")],
    },
    Row {
        case: "insert jjk",
        mode: Mode::Insert,
        file: JK,
        keys: &["j", "j", "k"],
        want: &[
            PENDING,
            Want {
                outcome: Outcome::Pending,
                text: "j",
            },
            matched("commit", 0),
        ],
    },
    Row {
        case: "insert jk then esc",
        mode: Mode::Insert,
        file: JK_ENTER_ESC,
        keys: &["j", "<Esc>"],
        want: &[PENDING, typed("commit", "j")],
    },
    Row {
        case: "insert jk then ctrl-w",
        mode: Mode::Insert,
        file: JK,
        keys: &["j", "<C-w>"],
        want: &[PENDING, typed("delete_word_back", "j")],
    },
    Row {
        case: "28.11 repeat unbound",
        mode: Mode::Normal,
        file: "[normal]\nrepeat = []",
        keys: &["."],
        want: &[no_match(".")],
    },
    Row {
        case: "28.10 dd unbound",
        mode: Mode::Normal,
        file: "[normal]\ndelete = [\"X\"]",
        keys: &["d", "d"],
        want: &[no_match("d"), no_match("d")],
    },
    Row {
        case: "28.14 g stays a start",
        mode: Mode::Normal,
        file: "[normal]\nhelp = [\"g\"]",
        keys: &["g", "p"],
        want: &[PENDING, matched("cycle_priority", 0)],
    },
    Row {
        case: "count capped at 9999",
        mode: Mode::Normal,
        file: "",
        keys: &["1", "2", "3", "4", "5", "j"],
        want: &[
            PENDING,
            PENDING,
            PENDING,
            PENDING,
            PENDING,
            matched("cursor_down", 1234),
        ],
    },
    Row {
        case: "zero after a count",
        mode: Mode::Normal,
        file: "",
        keys: &["1", "0", "0", "j"],
        want: &[PENDING, PENDING, PENDING, matched("cursor_down", 100)],
    },
    Row {
        case: "zero alone is a key",
        mode: Mode::Normal,
        file: "",
        keys: &["0"],
        want: &[no_match("0")],
    },
];

fn fed_matches(fed: &Fed, want: &Want) -> bool {
    fed.outcome == want.outcome
        && (want.text.is_empty() && fed.text.is_empty()
            || fed.text == strokes(want.text))
}

#[test]
fn matcher_rows() {
    for row in ROWS {
        let keymap = keymap_for(row.file);
        let mut m = keymap.matcher(row.mode);
        let all: Vec<Stroke> =
            row.keys.iter().flat_map(|k| strokes(k)).collect();
        assert_eq!(all.len(), row.want.len(), "{}", row.case);
        for (s, want) in all.into_iter().zip(row.want) {
            let fed = m.feed(s);
            assert!(fed_matches(&fed, want), "{}: {fed:?}", row.case);
        }
    }
}

const COMPLETION: &[(&str, &str)] = &[
    ("<Tab>", "accept"),
    ("<CR>", "accept"),
    ("<Esc>", "close"),
    ("<C-w>", "delete_word_back"),
];

#[test]
fn completion_over_insert() {
    let keymap = keymap_for("");
    for &(key, action) in COMPLETION {
        let mut m = keymap.layered(Mode::Completion, Mode::Insert);
        let fed = m.feed(strokes(key)[0]);
        assert_eq!(
            fed.outcome,
            Outcome::Matched { action, count: 0 },
            "28.34 {key}"
        );
    }
}

#[test]
fn reset_drops_count_and_pending() {
    let keymap = keymap_for("");
    let mut m = keymap.matcher(Mode::Normal);
    m.feed(strokes("3")[0]);
    m.feed(strokes("g")[0]);
    m.reset();
    let fed = m.feed(strokes("j")[0]);
    assert_eq!(
        fed.outcome,
        Outcome::Matched {
            action: "cursor_down",
            count: 0
        }
    );
}

fn feed_all(m: &mut Matcher, keys: &str) -> Vec<Fed> {
    strokes(keys).into_iter().map(|s| m.feed(s)).collect()
}

#[test]
fn take_moves_pending_keys_and_returns_text() {
    let keymap = keymap_for(JK);
    let mut insert = keymap.matcher(Mode::Insert);
    feed_all(&mut insert, "j");
    let mut command = keymap.matcher(Mode::Command);
    // `j` starts nothing in the command table: it comes back as text.
    assert_eq!(command.take(&mut insert), strokes("j"));
    assert_eq!(insert.take(&mut command).len(), 0);
}

#[test]
fn take_keeps_keys_that_start_a_binding() {
    let keymap = keymap_for(JK);
    let mut command = keymap.matcher(Mode::Command);
    let mut insert = keymap.matcher(Mode::Insert);
    feed_all(&mut insert, "j");
    let mut other = keymap.matcher(Mode::Insert);
    assert_eq!(other.take(&mut insert).len(), 0);
    let fed = other.feed(strokes("k")[0]);
    assert_eq!(
        fed.outcome,
        Outcome::Matched {
            action: "commit",
            count: 0
        }
    );
    assert_eq!(command.take(&mut other).len(), 0);
}
