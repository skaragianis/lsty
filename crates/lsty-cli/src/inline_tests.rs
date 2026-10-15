use std::{fs, path::Path};

use jiff::tz::TimeZone;
use lsty_core::{
    config::Config,
    dates::Clock,
    discover::{self, Found, Input},
    errs::Code,
    ops,
};
use tempfile::TempDir;

use crate::inline::{Inline, inline_names, inline_options, run_inline};

struct Fixture {
    dir: TempDir,
    found: Found,
    config: Config,
    clock: Clock,
}

fn fixture(todo: &str) -> Fixture {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    fs::write(dir.path().join("todo.txt"), todo)
        .unwrap_or_else(|e| panic!("{e}"));
    let none = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(Path::new("todo.txt")),
        vars: &none,
        cwd: dir.path(),
        home: Some(dir.path()),
        default_file: None,
        todosh: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    let now = "2026-09-29T10:00Z".parse().expect("instant");
    let clock = Clock::new(&|_| None, now, TimeZone::UTC)
        .unwrap_or_else(|e| panic!("{}", e.message()));
    Fixture {
        dir,
        found,
        config: Config::default(),
        clock,
    }
}

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

const REFUSED: &[(&str, &str)] = &[
    ("do 1 --json", "--json is not available in the TUI"),
    ("do 1 --raw", "--raw is not available in the TUI"),
    ("do 1 --owner x", "--owner is not available in the TUI"),
    ("do 1 --human", "--human is not available in the TUI"),
    ("do 1 --file f", "--file is not available in the TUI"),
    ("ls --no-config", "--no-config is not available in the TUI"),
    ("do 1 --color=never", "--color is not available in the TUI"),
    ("do 1 --create", "--create is not available in the TUI"),
    ("do --help", "--help is not available in the TUI"),
    ("do -V", "--version is not available in the TUI"),
    ("add x --print-id", "--print-id is not available in the TUI"),
];

#[test]
fn options_of_the_process_are_refused() {
    let fx = fixture("A\nB\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    for (line, want) in REFUSED {
        let err = run_inline(&env, None, &words(line)).err();
        let err = err.unwrap_or_else(|| panic!("{line} ran"));
        assert_eq!(err.code(), Code::Usage, "{line}");
        assert_eq!(err.message(), *want, "{line}");
        assert_eq!(err.hint(), None, "{line}");
    }
    let data = fs::read_to_string(fx.dir.path().join("todo.txt"));
    assert_eq!(data.as_deref().ok(), Some("A\nB\n"));
}

const OUTSIDE: &[(&str, &str)] = &[
    ("q!", "q!"),
    ("e other.txt", "e"),
    ("help", "help"),
    ("edit 1", "edit"),
    ("open 1", "open"),
    ("tui", "tui"),
];

#[test]
fn commands_of_the_process_are_unknown() {
    let fx = fixture("A\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    for (line, name) in OUTSIDE {
        let err = run_inline(&env, None, &words(line)).err();
        let err = err.unwrap_or_else(|| panic!("{line} ran"));
        assert_eq!(err.code(), Code::UnknownCommand, "{line}");
        assert_eq!(err.message(), format!("unknown command \"{name}\""));
        assert_eq!(err.hint(), None, "{line}");
    }
}

#[test]
fn a_missing_argument_hints_the_synopsis() {
    let fx = fixture("A\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    let err = run_inline(&env, None, &words("dep 1")).err();
    let err = err.unwrap_or_else(|| panic!("dep 1 ran"));
    assert_eq!(err.hint(), Some("usage: lsty dep REFS on REFS"));
}

#[test]
fn starting_work_needs_an_identity() {
    let fx = fixture("A\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    let err = run_inline(&env, None, &words("next --start")).err();
    let err = err.unwrap_or_else(|| panic!("next --start ran"));
    assert_eq!(err.code(), Code::NoIdentity);
}

#[test]
fn a_write_hands_back_its_outcome_and_a_read_its_lines() {
    let fx = fixture("A\nB +w\n");
    let mut env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    env.record_undo = true;
    let write = run_inline(&env, None, &words("do 1"))
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let Inline::Wrote(mut outcome) = write else {
        panic!("a write hands back no outcome");
    };
    assert!(outcome.take_undo().is_some());
    let read = run_inline(&env, None, &words("ls +w"))
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let Inline::Read(lines) = read else {
        panic!("a read hands back an outcome");
    };
    assert_eq!(lines.first().map(Vec::as_slice), Some(&b"2 B +w"[..]));
}

#[test]
fn names_and_options_leave_out_what_the_process_does() {
    let names = inline_names();
    for name in ["do", "done", "ls", "add", "a"] {
        assert!(names.contains(&name), "{name}");
    }
    for name in ["tui", "help", "edit", "open"] {
        assert!(!names.contains(&name), "{name}");
    }
    assert_eq!(
        inline_options("do").first().map(String::as_str),
        Some("--expect")
    );
    assert!(inline_options("ls").iter().all(|o| o != "--raw"));
    assert_eq!(inline_options("edit"), Vec::<String>::new());
    assert_eq!(inline_options("nope"), Vec::<String>::new());
}
