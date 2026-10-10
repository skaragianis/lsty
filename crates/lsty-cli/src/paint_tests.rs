use std::path::Path;

use lsty_core::{
    config::Warning,
    theme::{self, Profile, Slot, Theme},
};

use super::paint::Painter;

fn theme(name: &str) -> Theme {
    let (theme, warnings) =
        theme::load(name, Path::new(""), Path::new("/"), true);
    assert_eq!(warnings, vec![]);
    theme
}

fn painter(name: &str, profile: Profile) -> Painter {
    Painter::new(&theme(name), profile)
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

#[test]
fn an_off_painter_writes_text_as_it_is() {
    let p = Painter::off();
    assert_eq!(p.paint(Slot::PriorityA, b"(A)"), b"(A)");
    assert_eq!(p.paint(Slot::Done, b"\xe9"), b"\xe9");
}

/// The theme, the profile, the slot, the text and what is written.
type Row = (&'static str, Profile, Slot, &'static str, &'static str);

const ROWS: [Row; 8] = [
    (
        "ansi",
        Profile::TrueColor,
        Slot::PriorityA,
        "(A)",
        "\x1b[1;91m(A)\x1b[m",
    ),
    (
        "ansi",
        Profile::TrueColor,
        Slot::PriorityB,
        "(B)",
        "\x1b[33m(B)\x1b[m",
    ),
    ("ansi", Profile::TrueColor, Slot::Text, "plain", "plain"),
    ("ansi", Profile::TrueColor, Slot::Muted, "", ""),
    (
        "ansi",
        Profile::TrueColor,
        Slot::Link,
        "a b",
        "\x1b[4;34ma b\x1b[m",
    ),
    (
        "ansi",
        Profile::TrueColor,
        Slot::Done,
        "x y",
        "\x1b[2;90;9mx y\x1b[m",
    ),
    (
        "catppuccin-mocha",
        Profile::TrueColor,
        Slot::PriorityA,
        "(A)",
        "\x1b[1;38;2;243;139;168m(A)\x1b[m",
    ),
    (
        "catppuccin-mocha",
        Profile::Ansi256,
        Slot::ModeNormal,
        "N",
        "\x1b[1;38;5;233;48;5;111mN\x1b[m",
    ),
];

#[test]
fn slots_are_written_as_their_sequences() {
    for (name, profile, slot, input, want) in ROWS {
        let got = painter(name, profile).paint(slot, input.as_bytes());
        assert_eq!(text(got), want, "{name} {slot:?} {input:?}");
    }
}

#[test]
fn sixteen_colours_reduce_a_24_bit_colour() {
    let got = painter("catppuccin-mocha", Profile::Ansi);
    assert_eq!(
        text(got.paint(Slot::PriorityA, b"(A)")),
        "\x1b[1;91m(A)\x1b[m"
    );
    assert_eq!(text(got.paint(Slot::Project, b"+p")), "\x1b[94m+p\x1b[m");
}

#[test]
fn a_struck_faint_run_is_one_pair_around_the_whole_text() {
    let got = painter("ansi", Profile::TrueColor).paint(Slot::Done, b"x a b");
    assert_eq!(text(got), "\x1b[2;90;9mx a b\x1b[m");
}

#[test]
fn an_underlined_run_underlines_once() {
    let got =
        painter("ansi", Profile::TrueColor).paint(Slot::Link, b"file:a b");
    assert_eq!(text(got), "\x1b[4;34mfile:a b\x1b[m");
}

#[test]
fn a_struck_run_keeps_bytes_that_are_not_utf8() {
    let got = painter("ansi", Profile::TrueColor).paint(Slot::Done, b"x\xe9");
    assert_eq!(got, b"\x1b[2;90;9mx\xe9\x1b[m");
}

#[test]
fn text_that_is_not_utf8_stays_as_it_is_in_a_plain_run() {
    let got =
        painter("ansi", Profile::TrueColor).paint(Slot::PriorityB, b"\xe9");
    assert_eq!(got, b"\x1b[33m\xe9\x1b[m");
}

#[test]
fn errors_and_warnings_have_their_first_words_painted() {
    let p = painter("ansi", Profile::TrueColor);
    let lines = p.error_lines("bad", Some("try"));
    assert_eq!(text(lines[0].clone()), "\x1b[1;91mlsty:\x1b[m bad");
    assert_eq!(text(lines[1].clone()), "hint: try");
    let warning = Warning {
        source: "config.toml".to_owned(),
        message: "x".to_owned(),
    };
    assert_eq!(
        text(p.warning_line(&warning)),
        "\x1b[33mlsty: warning:\x1b[m config.toml: x"
    );
}
