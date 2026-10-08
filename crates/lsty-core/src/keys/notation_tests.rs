use super::{FunctionKey, Mods, Named, Stroke, parse_keys};
use crate::errs::Code;

fn ch(c: char) -> Stroke {
    Stroke::char(c)
}

fn ctrl(c: char) -> Stroke {
    ch(c).with_mods(Mods::CTRL)
}

fn named(n: Named) -> Stroke {
    Stroke::named(n)
}

const SPACE: Stroke = Stroke::char(' ');

struct Row {
    case: &'static str,
    input: &'static str,
    leader: Stroke,
    want: fn() -> Vec<Stroke>,
}

const OK: &[Row] = &[
    Row {
        case: "28.1 j",
        input: "j",
        leader: SPACE,
        want: || vec![ch('j')],
    },
    Row {
        case: "28.1 J",
        input: "J",
        leader: SPACE,
        want: || vec![ch('J')],
    },
    Row {
        case: "28.1 gg",
        input: "gg",
        leader: SPACE,
        want: || vec![ch('g'), ch('g')],
    },
    Row {
        case: "28.1 dd",
        input: "dd",
        leader: SPACE,
        want: || vec![ch('d'), ch('d')],
    },
    Row {
        case: "28.1 C-w h",
        input: "<C-w>h",
        leader: SPACE,
        want: || vec![ctrl('w'), ch('h')],
    },
    Row {
        case: "28.1 C-d",
        input: "<C-d>",
        leader: SPACE,
        want: || vec![ctrl('d')],
    },
    Row {
        case: "28.1 S-Tab",
        input: "<S-Tab>",
        leader: SPACE,
        want: || vec![named(Named::Tab).with_mods(Mods::SHIFT)],
    },
    Row {
        case: "28.1 CR",
        input: "<CR>",
        leader: SPACE,
        want: || vec![named(Named::Cr)],
    },
    Row {
        case: "28.2 upper C-D",
        input: "<C-D>",
        leader: SPACE,
        want: || vec![ctrl('d')],
    },
    Row {
        case: "28.2 lower c-d",
        input: "<c-d>",
        leader: SPACE,
        want: || vec![ctrl('d')],
    },
    Row {
        case: "28.3 leader d",
        input: "<leader>d",
        leader: SPACE,
        want: || vec![ch(' '), ch('d')],
    },
    Row {
        case: "28.3 comma d",
        input: "<leader>d",
        leader: Stroke::char(','),
        want: || vec![ch(','), ch('d')],
    },
    Row {
        case: "28.4 lt",
        input: "<lt>",
        leader: SPACE,
        want: || vec![ch('<')],
    },
    Row {
        case: "gt",
        input: "<gt>",
        leader: SPACE,
        want: || vec![ch('>')],
    },
    Row {
        case: "C-lt",
        input: "<C-lt>",
        leader: SPACE,
        want: || vec![ctrl('<')],
    },
    Row {
        case: "A-gt",
        input: "<A-gt>",
        leader: SPACE,
        want: || vec![ch('>').with_mods(Mods::ALT)],
    },
    Row {
        case: "28.4 Enter",
        input: "<Enter>",
        leader: SPACE,
        want: || vec![named(Named::Cr)],
    },
    Row {
        case: "28.4 Return",
        input: "<Return>",
        leader: SPACE,
        want: || vec![named(Named::Cr)],
    },
    Row {
        case: "28.4 F5",
        input: "<F5>",
        leader: SPACE,
        want: || vec![named(Named::function(5).unwrap())],
    },
    Row {
        case: "28.4 C-A-x",
        input: "<C-A-x>",
        leader: SPACE,
        want: || vec![ch('x').with_mods(Mods::CTRL.with(Mods::ALT))],
    },
    Row {
        case: "28.7 Dow",
        input: "Dow",
        leader: SPACE,
        want: || vec![ch('D'), ch('o'), ch('w')],
    },
    Row {
        case: "28.7 Downs",
        input: "Downs",
        leader: SPACE,
        want: || vec![ch('D'), ch('o'), ch('w'), ch('n'), ch('s')],
    },
    Row {
        case: "28.3 named Space",
        input: "<Space>",
        leader: SPACE,
        want: || vec![ch(' ')],
    },
    Row {
        case: "shift letter",
        input: "<S-x>",
        leader: SPACE,
        want: || vec![ch('X')],
    },
    Row {
        case: "shift iota subscript 1F80",
        input: "<S-\u{1F80}>",
        leader: SPACE,
        want: || vec![ch('\u{1F88}')],
    },
    Row {
        case: "shift iota subscript 1F87",
        input: "<S-\u{1F87}>",
        leader: SPACE,
        want: || vec![ch('\u{1F8F}')],
    },
    Row {
        case: "shift iota subscript 1F90",
        input: "<S-\u{1F90}>",
        leader: SPACE,
        want: || vec![ch('\u{1F98}')],
    },
    Row {
        case: "shift iota subscript 1F97",
        input: "<S-\u{1F97}>",
        leader: SPACE,
        want: || vec![ch('\u{1F9F}')],
    },
    Row {
        case: "shift iota subscript 1FA0",
        input: "<S-\u{1FA0}>",
        leader: SPACE,
        want: || vec![ch('\u{1FA8}')],
    },
    Row {
        case: "shift iota subscript 1FA7",
        input: "<S-\u{1FA7}>",
        leader: SPACE,
        want: || vec![ch('\u{1FAF}')],
    },
    Row {
        case: "shift iota subscript 1FB3",
        input: "<S-\u{1FB3}>",
        leader: SPACE,
        want: || vec![ch('\u{1FBC}')],
    },
    Row {
        case: "shift iota subscript 1FC3",
        input: "<S-\u{1FC3}>",
        leader: SPACE,
        want: || vec![ch('\u{1FCC}')],
    },
    Row {
        case: "shift iota subscript 1FF3",
        input: "<S-\u{1FF3}>",
        leader: SPACE,
        want: || vec![ch('\u{1FFC}')],
    },
    Row {
        case: "shift without a simple upper case DF",
        input: "<S-\u{DF}>",
        leader: SPACE,
        want: || vec![ch('\u{DF}')],
    },
    Row {
        case: "shift without a simple upper case 149",
        input: "<S-\u{149}>",
        leader: SPACE,
        want: || vec![ch('\u{149}')],
    },
    Row {
        case: "shift without a simple upper case FB01",
        input: "<S-\u{FB01}>",
        leader: SPACE,
        want: || vec![ch('\u{FB01}')],
    },
    Row {
        case: "shift without a simple upper case 1F88",
        input: "<S-\u{1F88}>",
        leader: SPACE,
        want: || vec![ch('\u{1F88}')],
    },
];

#[test]
fn parse_keys_reads_the_notation() {
    for row in OK {
        let got = parse_keys(row.input, Some(row.leader));
        assert_eq!(got.ok(), Some((row.want)()), "{}", row.case);
    }
}

const ERRORS: &[(&str, &str, &str)] = &[
    (
        "28.5 Down",
        "Down",
        r#""Down" reads as the keys D-o-w-n; write "<Down>""#,
    ),
    (
        "28.5 Esc",
        "Esc",
        r#""Esc" reads as the keys E-s-c; write "<Esc>""#,
    ),
    (
        "28.5 tab",
        "tab",
        r#""tab" reads as the keys t-a-b; write "<Tab>""#,
    ),
    (
        "28.5 SPACE",
        "SPACE",
        r#""SPACE" reads as the keys S-P-A-C-E; write "<Space>""#,
    ),
    ("28.6 unknown name", "<Foo>", r#"invalid key "<Foo>""#),
    ("28.6 unclosed", "<C-", r#"invalid key "<C-""#),
    ("28.6 empty", "", r#"invalid key """#),
    ("empty brackets", "<>", r#"invalid key "<>""#),
    ("plain modifier only", "<C>", r#"invalid key "<C>""#),
    ("modified word", "<C-ab>", r#"invalid key "<C-ab>""#),
    ("no leader", "<leader>", r#"invalid key "<leader>""#),
];

#[test]
fn parse_keys_rejects_bad_notation() {
    for &(case, input, message) in ERRORS {
        let leader = (case != "no leader").then_some(SPACE);
        let Err(e) = parse_keys(input, leader) else {
            panic!("{case}: accepted");
        };
        assert_eq!(e.code(), Code::InvalidValue, "{case}");
        assert_eq!(e.message(), message, "{case}");
    }
}

#[test]
fn named_keys_have_their_names() {
    assert_eq!(Named::PageDown.as_str(), "PageDown");
    assert_eq!(Named::function(12).unwrap().as_str(), "F12");
    assert_eq!(Named::function(1).unwrap().as_str(), "F1");
}

#[test]
fn function_keys_are_one_to_twelve() {
    assert_eq!(Named::function(0), None);
    assert_eq!(Named::function(13), None);
    assert_eq!(parse_keys("<F13>", None).ok(), None);
}

#[test]
fn names_are_lower_cased_per_character() {
    // `İ` lower-cases to one character only when taken alone.
    let got = parse_keys("<C-\u{130}>", None).ok();
    assert_eq!(got, Some(vec![ctrl('i')]));
}

#[test]
fn modifier_sets_combine_and_compare() {
    let both = Mods::CTRL.with(Mods::ALT);
    assert!(both.has(Mods::CTRL) && both.has(Mods::ALT));
    assert!(!both.has(Mods::SHIFT));
    assert!(both.has(both));
    assert!(!Mods::CTRL.has(both));
    assert!(!Mods::CTRL.has(Mods::ALT));
    assert!(!Mods::NONE.has(Mods::CTRL));
    assert!(!both.has(Mods::NONE));
    assert!(Mods::CTRL.with(Mods::CTRL).has(Mods::CTRL));
    assert_eq!(both.without(Mods::CTRL), Mods::ALT);
}

#[test]
fn function_keys_are_numbered_one_to_twelve() {
    for n in 1..=12 {
        assert_eq!(FunctionKey::new(n).map(FunctionKey::get), Some(n));
    }
    assert_eq!(FunctionKey::new(0), None);
    assert_eq!(FunctionKey::new(13), None);
}
