use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::builtin::BUILTIN;
use super::load::parse;
use super::{Colour, DEFAULT_NAME, Slot, Style, Theme, load, names};
use crate::config::Warning;

const BUILTIN_NAMES: [&str; 11] = [
    "ansi",
    "catppuccin-frappe",
    "catppuccin-latte",
    "catppuccin-macchiato",
    "catppuccin-mocha",
    "gruvbox-dark",
    "gruvbox-light",
    "nord",
    "tokyo-night",
    "tokyo-night-day",
    "tokyo-night-storm",
];

fn themes_dir(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, text) in files {
        fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
    }
    dir
}

fn load_user(name: &str, text: &str) -> (Theme, Vec<Warning>) {
    let dir = themes_dir(&[(name, text)]);
    load(name, dir.path(), Path::new("/"), false)
}

fn builtin_theme(name: &str) -> Theme {
    let (theme, warnings) = load(name, Path::new(""), Path::new("/"), true);
    assert_eq!(warnings, vec![], "{name}");
    theme
}

fn texts(warnings: &[Warning]) -> Vec<String> {
    warnings
        .iter()
        .map(|w| format!("{}|{}", w.source, w.message))
        .collect()
}

fn fg(theme: &Theme, slot: Slot) -> Option<Colour> {
    theme.style(slot).fg
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn strs(names: &[&str]) -> Vec<String> {
    names.iter().map(|&n| n.to_owned()).collect()
}

#[test]
fn builtin_table_is_sorted_and_complete() {
    let listed: Vec<&str> = BUILTIN.iter().map(|&(n, _)| n).collect();
    assert_eq!(listed, BUILTIN_NAMES);
}

/// Files in the folder, whether `noConfig` is set, the names expected.
type NamesRow = (
    &'static str,
    &'static [&'static str],
    bool,
    &'static [&'static str],
);

const EXTRA: &[&str] = &["mine"];

const NAMES_ROWS: [NamesRow; 3] = [
    ("29.1 without user themes", &[], false, &[]),
    (
        "29.13 user themes and an override",
        &["mine", "nord"],
        false,
        EXTRA,
    ),
    ("29.17 no config ignores user themes", &["mine"], true, &[]),
];

#[test]
fn names_lists_built_in_and_user_themes() {
    for (case, files, no_config, extra) in NAMES_ROWS {
        let files: Vec<(&str, &str)> =
            files.iter().map(|&f| (f, "")).collect();
        let dir = themes_dir(&files);
        let got = names(dir.path(), Path::new("/"), no_config);
        let mut want = strs(&BUILTIN_NAMES);
        want.extend(strs(extra));
        assert_eq!(got, sorted(want), "{case}");
    }
}

#[test]
fn names_skips_folders_and_other_files() {
    let dir = themes_dir(&[("mine", "")]);
    fs::create_dir(dir.path().join("sub.toml")).unwrap();
    fs::write(dir.path().join("notes.txt"), "").unwrap();
    let got = names(dir.path(), Path::new("/"), false);
    assert!(got.contains(&"mine".to_owned()));
    assert!(!got.contains(&"sub".to_owned()));
    assert!(!got.contains(&"notes".to_owned()));
}

#[test]
fn names_of_a_missing_folder_are_the_built_in_ones() {
    let got = names(Path::new("/no/such/folder"), Path::new("/"), false);
    assert_eq!(got, strs(&BUILTIN_NAMES));
}

#[test]
fn every_built_in_theme_loads_cleanly_and_defines_every_slot() {
    for (name, text) in BUILTIN {
        let (_, warnings) =
            parse(name, text.as_bytes(), &[Style::default(); Slot::ALL.len()]);
        assert_eq!(warnings, vec![], "29.2 {name}");
        let theme = builtin_theme(name);
        let slots = text
            .lines()
            .skip_while(|l| *l != "[slots]")
            .filter(|l| l.contains('='))
            .count();
        assert_eq!(slots, Slot::ALL.len(), "29.2 {name}: slots in the file");
        assert_eq!(theme.name(), name);
    }
}

#[test]
fn done_is_struck_and_faint_in_every_built_in_theme() {
    for name in BUILTIN_NAMES {
        let done = builtin_theme(name).style(Slot::Done);
        assert!(done.strike && done.faint, "29.3 {name}: {done:?}");
    }
}

#[test]
fn ansi_uses_only_the_terminals_own_colours() {
    let ansi = builtin_theme("ansi");
    for slot in Slot::ALL {
        let style = ansi.style(slot);
        assert_eq!(style.bg, None, "29.4 {slot:?}");
        assert!(
            matches!(style.fg, None | Some(Colour::Index(0..=15))),
            "29.4 {slot:?}: {:?}",
            style.fg
        );
    }
}

#[test]
fn mocha_mauve_is_as_published() {
    let text = BUILTIN
        .iter()
        .find(|&&(n, _)| n == "catppuccin-mocha")
        .map(|&(_, t)| t)
        .unwrap();
    assert!(text.contains("mauve     = \"#cba6f7\""), "29.5");
    assert_eq!(
        fg(&builtin_theme("catppuccin-mocha"), Slot::Project),
        Some(Colour::Rgb(0xcb, 0xa6, 0xf7))
    );
}

#[test]
fn the_documented_example_reads() {
    let (theme, warnings) = load_user(
        "mine",
        r##"
[palette]
mauve = "#cba6f7"
base  = "#1e1e2e"

[slots]
project    = "mauve"
priority_a = { fg = "#f38ba8", bold = true }
done       = { fg = "overlay0", strike = true, faint = true }
cursorline = { bg = "surface0" }
"##,
    );
    assert_eq!(
        fg(&theme, Slot::Project),
        Some(Colour::Rgb(0xcb, 0xa6, 0xf7)),
        "29.6"
    );
    assert!(theme.style(Slot::PriorityA).bold, "29.6");
    assert_eq!(
        texts(&warnings),
        [
            r#"themes/mine.toml|done: bad colour "overlay0""#,
            r#"themes/mine.toml|cursorline: bad colour "surface0""#,
        ],
        "the example names palette entries it does not define"
    );
}

/// A case, the colour as written, and the colour it stands for (none: a
/// warning).
type ColourRow = (&'static str, &'static str, Option<Colour>);

const COLOURS: [ColourRow; 8] = [
    ("29.7 short hex", "#fff", Some(Colour::Rgb(255, 255, 255))),
    (
        "29.7 upper case hex",
        "#FFFFFF",
        Some(Colour::Rgb(255, 255, 255)),
    ),
    ("29.7 low number", "7", Some(Colour::Index(7))),
    ("29.7 high number", "255", Some(Colour::Index(255))),
    ("29.8 number too high", "256", None),
    ("29.8 unknown name", "red", None),
    ("29.8 bad hex digits", "#ggg", None),
    ("29.8 bad hex length", "#12345", None),
];

#[test]
fn colours_are_accepted_or_warned_about() {
    let ansi = builtin_theme("ansi");
    for (case, text, want) in COLOURS {
        let (theme, warnings) =
            load_user("mine", &format!("[slots]\nproject = \"{text}\"\n"));
        let colour = want.or_else(|| fg(&ansi, Slot::Project));
        assert_eq!(fg(&theme, Slot::Project), colour, "{case}");
        let expected: Vec<String> = if want.is_some() {
            Vec::new()
        } else {
            vec![format!("themes/mine.toml|project: bad colour \"{text}\"")]
        };
        assert_eq!(texts(&warnings), expected, "{case}");
    }
}

#[test]
fn a_number_written_with_zeros_is_the_number() {
    let (theme, warnings) = load_user("mine", "[slots]\nproject = \"007\"\n");
    assert_eq!(warnings, vec![]);
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Index(7)));
}

#[test]
fn a_palette_entry_named_like_a_bad_number_is_used() {
    let (theme, warnings) = load_user(
        "mine",
        "[palette]\n\"256\" = \"#010203\"\n[slots]\nproject = \"256\"\n",
    );
    assert_eq!(warnings, vec![]);
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Rgb(1, 2, 3)));
}

#[test]
fn a_palette_entry_may_name_another() {
    let (theme, warnings) = load_user(
        "mine",
        "[palette]\na = \"b\"\nb = \"#112233\"\n[slots]\ntext = \"a\"\n",
    );
    assert_eq!(warnings, vec![], "29.9");
    assert_eq!(
        fg(&theme, Slot::Text),
        Some(Colour::Rgb(0x11, 0x22, 0x33)),
        "29.9"
    );
}

#[test]
fn a_palette_loop_warns_for_the_slots_that_use_it() {
    let (theme, warnings) = load_user(
        "mine",
        r##"
[palette]
a = "b"
b = "a"
c = "#445566"

[slots]
text    = "a"
project = "b"
muted   = "c"
"##,
    );
    let ansi = builtin_theme("ansi");
    assert_eq!(
        fg(&theme, Slot::Muted),
        Some(Colour::Rgb(0x44, 0x55, 0x66)),
        "29.10"
    );
    assert_eq!(theme.style(Slot::Text), ansi.style(Slot::Text), "29.10");
    assert_eq!(
        theme.style(Slot::Project),
        ansi.style(Slot::Project),
        "29.10"
    );
    assert_eq!(
        texts(&warnings),
        [
            r#"themes/mine.toml|text: bad colour "a""#,
            r#"themes/mine.toml|project: bad colour "b""#,
        ],
        "29.10"
    );
}

const ONLY_PROJECT: &str = "[slots]\nproject = \"#010203\"\n";

#[test]
fn a_user_theme_falls_back_to_the_built_in_one_of_its_name_or_ansi() {
    for (case, name, base) in [
        ("29.11 a new theme falls back to ansi", "mine", "ansi"),
        (
            "29.12 an override falls back to the built-in",
            "nord",
            "nord",
        ),
    ] {
        let (theme, warnings) = load_user(name, ONLY_PROJECT);
        assert_eq!(warnings, vec![], "{case}");
        let base = builtin_theme(base);
        for slot in Slot::ALL {
            let want = if slot == Slot::Project {
                Style::fg(Colour::Rgb(1, 2, 3))
            } else {
                base.style(slot)
            };
            assert_eq!(theme.style(slot), want, "{case}: {slot:?}");
        }
        assert_eq!(theme.name(), name);
    }
}

#[test]
fn an_unknown_theme_warns_and_loads_the_default() {
    let (theme, warnings) = load("nope", Path::new(""), Path::new("/"), true);
    let want = vec![Warning {
        source: "config.toml".to_owned(),
        message: "theme: no theme \"nope\"".to_owned(),
    }];
    assert_eq!(warnings, want, "29.14");
    assert_eq!(theme, builtin_theme("catppuccin-mocha"), "29.14");
    assert_eq!(theme.name(), DEFAULT_NAME);
}

#[test]
fn bad_entries_are_skipped_and_the_rest_is_read() {
    let (theme, warnings) = load_user(
        "mine",
        r##"
[slots]
frob    = "#fff"
project = { fg = "#fff", blink = true }
text    = "#123"
context = { fg = "#fff", bold = "yes" }
"##,
    );
    assert_eq!(
        texts(&warnings),
        [
            r#"themes/mine.toml|unknown slot "frob""#,
            r#"themes/mine.toml|project: unknown key "blink""#,
            "themes/mine.toml|context: bold: expected bool",
        ],
        "29.15"
    );
    let ansi = builtin_theme("ansi");
    assert_eq!(theme.style(Slot::Context), ansi.style(Slot::Context));
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Rgb(255, 255, 255)));
    assert_eq!(fg(&theme, Slot::Text), Some(Colour::Rgb(0x11, 0x22, 0x33)));
}

#[test]
fn a_syntax_error_keeps_the_base() {
    for (name, base) in [("mine", "ansi"), ("nord", "nord")] {
        let (theme, warnings) = load_user(name, "[slots\nproject = 1\n");
        assert_eq!(warnings.len(), 1, "29.16 {name}");
        assert_eq!(warnings[0].source, format!("themes/{name}.toml"));
        assert!(warnings[0].message.starts_with("line "), "{warnings:?}");
        let want = builtin_theme(base);
        for slot in Slot::ALL {
            assert_eq!(theme.style(slot), want.style(slot), "29.16 {name}");
        }
    }
}

#[test]
fn text_that_is_not_utf8_is_a_warning() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("mine.toml"),
        b"[slots]\nproject = \"\xff\"\n",
    )
    .unwrap();
    let (_, warnings) = load("mine", dir.path(), Path::new("/"), false);
    assert_eq!(
        texts(&warnings),
        ["themes/mine.toml|line 2: invalid UTF-8 byte: 0xff"]
    );
}

#[test]
fn an_integer_out_of_range_is_a_warning() {
    let (_, warnings) =
        load_user("mine", "[slots]\nproject = 99999999999999999999\n");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("out of range"), "{warnings:?}");
}

#[test]
fn no_config_ignores_a_user_file_of_a_built_in_name() {
    let dir = themes_dir(&[("nord", ONLY_PROJECT)]);
    let (theme, warnings) = load("nord", dir.path(), Path::new("/"), true);
    assert_eq!(warnings, vec![]);
    assert_eq!(theme, builtin_theme("nord"));
}

#[test]
fn a_relative_folder_is_taken_from_the_working_folder() {
    let dir = themes_dir(&[("mine", ONLY_PROJECT)]);
    let (theme, warnings) = load("mine", Path::new(""), dir.path(), false);
    assert_eq!(warnings, vec![]);
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Rgb(1, 2, 3)));
}

#[test]
fn a_user_theme_named_like_the_default_applies_after_an_unknown_name() {
    let dir = themes_dir(&[("catppuccin-mocha", ONLY_PROJECT)]);
    let (theme, warnings) = load("nope", dir.path(), Path::new("/"), false);
    assert_eq!(texts(&warnings), [r#"config.toml|theme: no theme "nope""#]);
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Rgb(1, 2, 3)));
}

#[test]
fn a_slot_table_replaces_the_whole_style() {
    let (theme, _) =
        load_user("mine", "[slots]\npriority_a = { fg = \"3\" }\nlink = {}\n");
    assert_eq!(theme.style(Slot::PriorityA), Style::fg(Colour::Index(3)));
    assert_eq!(theme.style(Slot::Link), Style::default());
}

#[test]
fn a_bad_value_in_a_slot_table_keeps_the_base_slot() {
    let (theme, warnings) = load_user(
        "mine",
        "[slots]\npriority_a = { fg = \"3\", bg = \"red\", bold = true }\n",
    );
    let ansi = builtin_theme("ansi");
    assert_eq!(theme.style(Slot::PriorityA), ansi.style(Slot::PriorityA));
    assert_eq!(
        texts(&warnings),
        [r#"themes/mine.toml|priority_a: bad colour "red""#]
    );
}

/// A value as written, and how a warning shows it.
const SHOWN: [(&str, &str); 14] = [
    ("1979-05-27T07:32:00Z", "1979-05-27 07:32:00 +0000 UTC"),
    (
        "1979-05-27T07:32:00-08:00",
        "1979-05-27 07:32:00 -0800 -0800",
    ),
    (
        "1979-05-27T07:32:00.5",
        "1979-05-27 07:32:00.5 +0000 datetime-local",
    ),
    ("1979-05-27", "1979-05-27 00:00:00 +0000 date-local"),
    ("07:32:00", "0000-01-01 07:32:00 +0000 time-local"),
    ("1.5", "1.5"),
    ("3.0", "3"),
    ("1e20", "1e+20"),
    ("1e-5", "1e-05"),
    ("0.000123", "0.000123"),
    ("1234567.0", "1.234567e+06"),
    ("inf", "+Inf"),
    (
        "[1, \"a b\", [true, 2.5], {x = 1}]",
        "[1 a b [true 2.5] map[x:1]]",
    ),
    ("0x1F", "31"),
];

#[test]
fn values_that_are_not_colours_are_shown_in_the_warning_as_plain_text() {
    for (written, shown) in SHOWN {
        let (_, warnings) =
            load_user("mine", &format!("[slots]\ntext = {written}\n"));
        let want = format!("themes/mine.toml|text: bad colour \"{shown}\"");
        assert_eq!(texts(&warnings), [want], "{written}");
    }
}

#[test]
fn warnings_follow_the_order_of_the_file() {
    let (_, warnings) = load_user(
        "mine",
        "[slots]\nzzz = 1\nmuted = 2\ncontext = { bold = 1, fg = 3 }\n",
    );
    assert_eq!(
        texts(&warnings),
        [
            r#"themes/mine.toml|unknown slot "zzz""#,
            r#"themes/mine.toml|muted: bad colour "2""#,
            "themes/mine.toml|context: bold: expected bool",
            r#"themes/mine.toml|context: bad colour "3""#,
        ]
    );
}
