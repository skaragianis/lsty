use std::{fs, path::Path};

use tempfile::TempDir;

use super::{
    Colour, Profile, Slot, Style, Theme, builtin::BUILTIN, load, load::parse,
    names,
};
use crate::{config::Warning, ops::test_util::strings};

const BUILTIN_NAMES: [&str; 14] = [
    "ansi",
    "catppuccin-frappe",
    "catppuccin-latte",
    "catppuccin-macchiato",
    "catppuccin-mocha",
    "gruvbox-dark",
    "gruvbox-light",
    "monokai",
    "nord",
    "one-dark",
    "one-light",
    "tokyo-night",
    "tokyo-night-day",
    "tokyo-night-storm",
];

fn themes_dir<T: AsRef<[u8]>>(files: &[(&str, T)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, text) in files {
        fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
    }
    dir
}

fn load_user(name: &str, text: impl AsRef<[u8]>) -> (Theme, Vec<Warning>) {
    let dir = themes_dir(&[(name, text)]);
    load(name, dir.path(), Path::new("/"), false, Profile::TrueColor)
}

fn builtin_theme(name: &str) -> Theme {
    builtin_at(name, Profile::TrueColor)
}

fn builtin_at(name: &str, profile: Profile) -> Theme {
    let (theme, warnings) =
        load(name, Path::new(""), Path::new("/"), true, profile);
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
        let mut want = strings(&BUILTIN_NAMES);
        want.extend(strings(extra));
        assert_eq!(got, sorted(want), "{case}");
    }
}

#[test]
fn names_skips_folders_and_other_files() {
    let dir = themes_dir(&[("mine", "")]);
    fs::create_dir(dir.path().join("sub.toml")).unwrap();
    fs::write(dir.path().join("notes.txt"), "").unwrap();
    let got = names(dir.path(), Path::new("/"), false);
    let mut want = strings(&BUILTIN_NAMES);
    want.push("mine".to_owned());
    assert_eq!(got, sorted(want));
}

#[test]
fn names_of_a_missing_folder_are_the_built_in_ones() {
    let got = names(Path::new("/no/such/folder"), Path::new("/"), false);
    assert_eq!(got, strings(&BUILTIN_NAMES));
}

#[test]
fn every_built_in_theme_loads_cleanly_and_defines_every_slot() {
    for (name, text) in BUILTIN {
        let (_, warnings) =
            parse(name, text.as_bytes(), &[Style::default(); Slot::ALL.len()]);
        assert_eq!(warnings, vec![], "29.2 {name}");
        let slots = text
            .lines()
            .skip_while(|l| *l != "[slots]")
            .filter(|l| l.contains('='))
            .count();
        assert_eq!(slots, Slot::ALL.len(), "29.2 {name}: slots in the file");
    }
}

#[test]
fn code_differs_from_every_slot_that_paints_task_text_in_every_built_in_theme()
{
    use Slot::*;
    for (name, _) in BUILTIN {
        let theme = builtin_theme(name);
        let code = theme.style(Code);
        for slot in [
            PriorityA,
            PriorityB,
            PriorityC,
            PriorityOther,
            Project,
            Context,
            Link,
            Text,
            Muted,
            DueOverdue,
            DueToday,
            DueSoon,
            Done,
            Doing,
            Blocked,
            Warning,
        ] {
            assert_ne!(
                code,
                theme.style(slot),
                "{name}: code is like {slot:?}"
            );
            assert_ne!(
                code.fg,
                theme.style(slot).fg,
                "{name}: code colour is {slot:?}'s"
            );
        }
    }
}

#[test]
fn ids_are_the_colour_of_the_normal_mode_label_in_every_built_in_theme() {
    for name in BUILTIN_NAMES {
        let theme = builtin_theme(name);
        let label = theme.style(Slot::ModeNormal);
        let want = Style {
            fg: label.bg.or(label.fg),
            ..Style::default()
        };
        assert_eq!(theme.style(Slot::Id), want, "{name}");
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

const COLOURS: [ColourRow; 15] = [
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
    ("a sign is no digit", "+5", None),
    ("a minus is no digit", "-1", None),
    ("too many digits with zeros", "0255", None),
    ("a digit of another script", "\u{663}", None),
    ("a hex sign", "#+1f", None),
    ("a hex digit of another script", "#\u{663}1", None),
    ("a lone hash", "#", None),
];

#[test]
fn colours_are_accepted_or_warned_about() {
    let ansi = builtin_theme("ansi");
    for (case, text, want) in COLOURS {
        let (theme, warnings) =
            load_user("mine", format!("[slots]\nproject = \"{text}\"\n"));
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
    }
}

/// A file that sets `muted` and not `hint_dim` dims with its `muted`; else
/// `hint_dim` is a slot like any other.
#[test]
fn hint_dim_falls_back_to_muted() {
    let muted = Style::fg(Colour::Index(100));
    let dim = Style::fg(Colour::Index(101));
    let ansi = builtin_theme("ansi").style(Slot::HintDim);
    let nord = builtin_theme("nord").style(Slot::HintDim);
    let rows = [
        ("mine", "[slots]\nmuted = \"100\"\n", muted),
        ("nord", "[slots]\nmuted = \"100\"\n", muted),
        (
            "mine",
            "[slots]\nmuted = \"100\"\nhint_dim = \"101\"\n",
            dim,
        ),
        (
            "mine",
            "[slots]\nhint_dim = \"101\"\nmuted = \"100\"\n",
            dim,
        ),
        ("mine", "[slots]\ntext = \"100\"\n", ansi),
        ("nord", "[slots]\ntext = \"100\"\n", nord),
    ];
    for (name, text, want) in rows {
        let (theme, warnings) = load_user(name, text);
        assert_eq!(warnings, vec![], "{text}");
        assert_eq!(theme.style(Slot::HintDim), want, "{name} {text}");
    }
}

const PROFILES: [Profile; 3] =
    [Profile::Ansi, Profile::Ansi256, Profile::TrueColor];

/// The style of `slot` as a terminal with `profile` gets it.
fn shown(theme: &Theme, slot: Slot, profile: Profile) -> Style {
    let style = theme.style(slot);
    let reduce = |c: Option<Colour>| c.map(|c| profile.reduce(c));
    Style {
        fg: reduce(style.fg),
        bg: reduce(style.bg),
        ..style
    }
}

/// The colour `slot` paints as a fill.
fn fill(theme: &Theme, slot: Slot, profile: Profile) -> Option<Colour> {
    let style = shown(theme, slot, profile);
    style.bg.or(style.fg)
}

/// A dim hint shows: no built-in theme paints it like a lit one.
#[test]
fn every_built_in_theme_shows_a_dim_hint() {
    for name in BUILTIN_NAMES {
        for profile in PROFILES {
            let theme = builtin_at(name, profile);
            let look = |slot| shown(&theme, slot, profile);
            let dim = look(Slot::HintDim);
            assert_ne!(dim, look(Slot::HintDesc), "{name} {profile:?}");
            assert_ne!(dim, look(Slot::HintKey), "{name} {profile:?}");
            let status = fill(&theme, Slot::StatusBg, profile);
            assert!(
                dim.fg != status || dim.fg.is_none(),
                "{name} {profile:?}"
            );
        }
    }
}

/// `ansi` says lit and dim by attribute alone: no colour of the terminal's
/// reads on both a dark background and a light one.
#[test]
fn the_ansi_hints_differ_by_attribute_alone() {
    let theme = builtin_theme("ansi");
    let [key, desc, dim] =
        [Slot::HintKey, Slot::HintDesc, Slot::HintDim].map(|s| theme.style(s));
    assert_eq!((key.fg, desc.fg, dim.fg), (None, None, None));
    assert_ne!(key, desc);
    assert_ne!(key, dim);
    assert_ne!(desc, dim);
}

#[test]
fn ansi_keeps_its_chosen_warning_mode_border_and_date_styles() {
    let theme = builtin_theme("ansi");
    let (blue, red) = (Some(Colour::Index(12)), Some(Colour::Index(9)));
    for (slot, fg, bold) in [
        (Slot::Warning, None, true),
        (Slot::ModeCommand, None, true),
        (Slot::PanelBorderActive, blue, true),
        (Slot::DueOverdue, red, true),
        (Slot::DueToday, red, false),
    ] {
        let style = theme.style(slot);
        assert_eq!((style.fg, style.bold), (fg, bold), "{}", slot.name());
    }
}

/// Slots the person has to tell apart where they meet.
const APART: [(Slot, Slot); 22] = [
    (Slot::PriorityA, Slot::PriorityB),
    (Slot::PriorityA, Slot::PriorityC),
    (Slot::PriorityB, Slot::PriorityC),
    (Slot::PriorityA, Slot::PriorityOther),
    (Slot::PriorityB, Slot::PriorityOther),
    (Slot::PriorityC, Slot::PriorityOther),
    (Slot::DueOverdue, Slot::DueToday),
    (Slot::DueOverdue, Slot::DueSoon),
    (Slot::DueToday, Slot::DueSoon),
    (Slot::ModeNormal, Slot::ModeInsert),
    (Slot::ModeNormal, Slot::ModeVisual),
    (Slot::ModeNormal, Slot::ModeCommand),
    (Slot::ModeInsert, Slot::ModeVisual),
    (Slot::ModeInsert, Slot::ModeCommand),
    (Slot::ModeVisual, Slot::ModeCommand),
    (Slot::Project, Slot::Context),
    (Slot::Doing, Slot::Blocked),
    (Slot::Done, Slot::Muted),
    (Slot::Warning, Slot::Error),
    (Slot::PanelBorder, Slot::PanelBorderActive),
    (Slot::HintKey, Slot::HintDesc),
    (Slot::Selection, Slot::Cursorline),
];

/// Every pair of [`APART`] looks different, in colour or in attribute.
#[test]
fn no_built_in_theme_loses_a_pair_in_any_profile() {
    let mut lost = Vec::new();
    for name in BUILTIN_NAMES {
        for profile in PROFILES {
            let theme = builtin_at(name, profile);
            let same = |&&(one, two): &&(Slot, Slot)| {
                shown(&theme, one, profile) == shown(&theme, two, profile)
            };
            lost.extend(
                APART
                    .iter()
                    .filter(same)
                    .map(|pair| format!("{name} {profile:?} {pair:?}")),
            );
        }
    }
    assert_eq!(lost, Vec::<String>::new());
}

/// What is drawn on the status line.
const ON_STATUS: [Slot; 10] = [
    Slot::HintKey,
    Slot::HintDesc,
    Slot::HintDim,
    Slot::Muted,
    Slot::Warning,
    Slot::Error,
    Slot::ModeNormal,
    Slot::ModeInsert,
    Slot::ModeVisual,
    Slot::ModeCommand,
];

/// What is drawn on the row under the cursor and on a selected one.
const ON_ROW: [Slot; 17] = [
    Slot::PriorityA,
    Slot::PriorityB,
    Slot::PriorityC,
    Slot::PriorityOther,
    Slot::Project,
    Slot::Context,
    Slot::Link,
    Slot::Text,
    Slot::Muted,
    Slot::Id,
    Slot::Code,
    Slot::DueOverdue,
    Slot::DueToday,
    Slot::DueSoon,
    Slot::Done,
    Slot::Doing,
    Slot::Blocked,
];

/// No colour equals the background it is drawn on: its own, or else the
/// fill under it. At 16 colours there is no background to equal.
#[test]
fn no_built_in_colour_equals_its_background_above_16_colours() {
    let places = [
        (Slot::StatusBg, ON_STATUS.as_slice()),
        (Slot::Cursorline, ON_ROW.as_slice()),
        (Slot::Selection, ON_ROW.as_slice()),
    ];
    let mut lost = Vec::new();
    for name in BUILTIN_NAMES {
        for profile in [Profile::Ansi256, Profile::TrueColor] {
            let theme = builtin_at(name, profile);
            let on = |slot, under: Option<Colour>| {
                let style = shown(&theme, slot, profile);
                let under = style.bg.or(under);
                (style.fg.is_some() && style.fg == under)
                    .then(|| format!("{name} {profile:?} {slot:?} {under:?}"))
            };
            lost.extend(Slot::ALL.into_iter().filter_map(|s| on(s, None)));
            for (place, slots) in places {
                let under = fill(&theme, place, profile);
                lost.extend(slots.iter().filter_map(|&s| on(s, under)));
            }
        }
    }
    assert_eq!(lost, Vec::<String>::new());
}

/// At 16 colours the name makes no difference; at more it is what it was.
#[test]
fn a_built_in_theme_is_ansi_at_16_colours_only() {
    let ansi = builtin_theme("ansi");
    for name in BUILTIN_NAMES {
        assert_eq!(builtin_at(name, Profile::Ansi), ansi, "{name}");
        let own = builtin_theme(name);
        assert_eq!(builtin_at(name, Profile::Ansi256), own, "{name}");
        assert_eq!(own == ansi, name == "ansi", "{name}");
    }
}

/// At 16 colours a file keeps the slots it sets, reduced when drawn, and
/// the rest comes from `ansi`, whatever the file is called.
#[test]
fn a_file_at_16_colours_keeps_its_slots_over_ansi() {
    let ansi = builtin_theme("ansi");
    let text = "[slots]\nproject = \"#ff8800\"\n";
    for name in ["mine", "nord"] {
        let dir = themes_dir(&[(name, text)]);
        let at =
            |profile| load(name, dir.path(), Path::new("/"), false, profile).0;
        let theme = at(Profile::Ansi);
        assert!(theme.from_file(), "{name}");
        let project = shown(&theme, Slot::Project, Profile::Ansi);
        assert_eq!(project.fg, Some(Colour::Index(9)), "{name}");
        for slot in Slot::ALL.into_iter().filter(|&s| s != Slot::Project) {
            assert_eq!(theme.style(slot), ansi.style(slot), "{name} {slot:?}");
        }
        let base = builtin_theme(if name == "nord" { name } else { "ansi" });
        let text_slot = at(Profile::Ansi256).style(Slot::Text);
        assert_eq!(text_slot, base.style(Slot::Text), "{name} at 256");
    }
}

/// A name that is no theme warns at 16 colours too, and draws as `ansi`.
#[test]
fn an_unknown_theme_at_16_colours_warns_and_draws_as_ansi() {
    let (theme, warnings) =
        load("nope", Path::new(""), Path::new("/"), true, Profile::Ansi);
    assert_eq!(warnings.len(), 1);
    assert_eq!(theme, builtin_theme("ansi"));
}

#[test]
fn an_unknown_theme_warns_and_loads_the_default() {
    let (theme, warnings) = load(
        "nope",
        Path::new(""),
        Path::new("/"),
        true,
        Profile::TrueColor,
    );
    let want = vec![Warning {
        source: "config.toml".to_owned(),
        message: "theme: no theme \"nope\"".to_owned(),
    }];
    assert_eq!(warnings, want, "29.14");
    assert_eq!(theme, builtin_theme("catppuccin-frappe"), "29.14");
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
    let (_, warnings) = load_user("mine", b"[slots]\nproject = \"\xff\"\n");
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
    let (theme, warnings) =
        load("nord", dir.path(), Path::new("/"), true, Profile::TrueColor);
    assert_eq!(warnings, vec![]);
    assert_eq!(theme, builtin_theme("nord"));
}

#[test]
fn a_relative_folder_is_taken_from_the_working_folder() {
    let dir = themes_dir(&[("mine", ONLY_PROJECT)]);
    let (theme, warnings) =
        load("mine", Path::new(""), dir.path(), false, Profile::TrueColor);
    assert_eq!(warnings, vec![]);
    assert_eq!(fg(&theme, Slot::Project), Some(Colour::Rgb(1, 2, 3)));
}

#[test]
fn a_user_theme_named_like_the_default_applies_after_an_unknown_name() {
    let dir = themes_dir(&[("catppuccin-frappe", ONLY_PROJECT)]);
    let (theme, warnings) = load(
        "nope",
        dir.path(),
        Path::new("/"),
        false,
        Profile::TrueColor,
    );
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
const SHOWN: [(&str, &str); 21] = [
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
    ("0.5", "0.5"),
    ("100.0", "100"),
    ("12.25", "12.25"),
    ("1234567.0", "1.234567e+06"),
    ("inf", "+Inf"),
    ("-inf", "-Inf"),
    ("nan", "NaN"),
    ("-2.5", "-2.5"),
    ("0.0", "0"),
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
            load_user("mine", format!("[slots]\ntext = {written}\n"));
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

#[test]
fn every_flag_of_a_style_is_read() {
    let (theme, warnings) = load_user(
        "mine",
        "[slots]\n\
         project = { italic = true }\n\
         context = { underline = true }\n\
         text = { strike = true }\n\
         muted = { faint = true }\n\
         done = { bold = true }\n",
    );
    assert_eq!(warnings, vec![]);
    let flags = |slot| {
        let s = theme.style(slot);
        (s.bold, s.italic, s.underline, s.strike, s.faint)
    };
    assert_eq!(flags(Slot::Project), (false, true, false, false, false));
    assert_eq!(flags(Slot::Context), (false, false, true, false, false));
    assert_eq!(flags(Slot::Text), (false, false, false, true, false));
    assert_eq!(flags(Slot::Muted), (false, false, false, false, true));
    assert_eq!(flags(Slot::Done), (true, false, false, false, false));
}

#[test]
fn the_default_config_names_the_default_theme() {
    assert_eq!(crate::config::Config::default().theme, super::DEFAULT_NAME);
}
