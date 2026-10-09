use std::ffi::OsString;

use super::env::Stream;
use super::tty::{ColorMode, Terminals};

type Vars = Vec<(&'static str, &'static str)>;

fn terminals(is_tty: [bool; 3], vars: &Vars) -> Terminals {
    let lookup = |name: &str| {
        vars.iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| OsString::from(*v))
    };
    Terminals::new(is_tty, &lookup)
}

type Row = (&'static str, [bool; 3], Vars, ColorMode, bool);

const TTY: [bool; 3] = [true; 3];
const NO_TTY: [bool; 3] = [false; 3];

/// 10.30: the colour decision.
#[test]
fn colour_decision() {
    let rows: [Row; 7] = [
        ("tty", TTY, vec![], ColorMode::Auto, true),
        ("no tty", NO_TTY, vec![], ColorMode::Auto, false),
        (
            "NO_COLOR",
            TTY,
            vec![("NO_COLOR", "1")],
            ColorMode::Auto,
            false,
        ),
        (
            "TERM=dumb",
            TTY,
            vec![("TERM", "dumb")],
            ColorMode::Auto,
            false,
        ),
        ("always", NO_TTY, vec![], ColorMode::Always, true),
        ("never", TTY, vec![], ColorMode::Never, false),
        (
            "always beats NO_COLOR",
            TTY,
            vec![("NO_COLOR", "1")],
            ColorMode::Always,
            true,
        ),
    ];
    for (name, tty, vars, mode, want) in rows {
        assert_eq!(
            terminals(tty, &vars).color_on(mode, Stream::Stdout),
            want,
            "{name}"
        );
        assert_eq!(
            terminals(tty, &vars).color_on(mode, Stream::Stderr),
            want,
            "{name}"
        );
    }
}

#[test]
fn an_empty_no_color_changes_nothing() {
    let t = terminals(TTY, &vec![("NO_COLOR", "")]);
    assert!(t.color_on(ColorMode::Auto, Stream::Stdout));
}

#[test]
fn colour_follows_the_stream() {
    let t = terminals([true, true, false], &vec![]);
    assert!(t.color_on(ColorMode::Auto, Stream::Stdout));
    assert!(!t.color_on(ColorMode::Auto, Stream::Stderr));
}

/// 10.31: `LSTY_NONINTERACTIVE=1` turns every terminal off.
#[test]
fn non_interactive_turns_every_stream_off() {
    let t = terminals(TTY, &vec![("LSTY_NONINTERACTIVE", "1")]);
    assert!(!t.can_run_tui());
    assert!(!t.has_terminals());
    assert!(!t.pages_help());
    for stream in Stream::ALL {
        assert!(!t.is_tty(stream));
        assert!(!t.color_on(ColorMode::Auto, stream));
    }
}

#[test]
fn only_one_turns_non_interactive_on() {
    for value in ["", "0", "yes", "true"] {
        let t = terminals(TTY, &vec![("LSTY_NONINTERACTIVE", value)]);
        assert!(t.can_run_tui(), "{value:?}");
    }
}

#[test]
fn the_tui_needs_stdin_and_stdout() {
    assert!(terminals(TTY, &vec![]).can_run_tui());
    assert!(!terminals([true, false, true], &vec![]).can_run_tui());
    assert!(!terminals([false, true, true], &vec![]).can_run_tui());
}

#[test]
fn a_dumb_terminal_runs_no_tui_and_no_pager_but_edit_still_runs() {
    let t = terminals(TTY, &vec![("TERM", "dumb")]);
    assert!(!t.can_run_tui());
    assert!(!t.pages_help());
    assert!(t.has_terminals());
}

#[test]
fn color_mode_comes_from_the_option() {
    assert_eq!(ColorMode::parse("always"), Some(ColorMode::Always));
    assert_eq!(ColorMode::parse("never"), Some(ColorMode::Never));
    assert_eq!(ColorMode::parse("auto"), Some(ColorMode::Auto));
    assert_eq!(ColorMode::parse("sometimes"), None);
    for name in ColorMode::ALL.map(ColorMode::as_str) {
        assert!(ColorMode::parse(name).is_some(), "{name}");
    }
}
