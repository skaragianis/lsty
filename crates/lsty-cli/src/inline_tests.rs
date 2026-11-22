use std::{fs, path::Path};

use jiff::tz::TimeZone;
use lsty_core::{
    config::Config,
    dates::Clock,
    discover::{self, Found, Input},
    errs::{Code, Nr, Source, lossy},
    ops::{self, Actor},
};
use tempfile::TempDir;

use crate::inline::{
    Inline, Places, inline_command, inline_commands, inline_names,
    inline_options, run_inline,
};

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
    ("do --version", "--version is not available in the TUI"),
    ("add x --print-id", "--print-id is not available in the TUI"),
];

#[test]
fn options_of_the_process_are_refused() {
    let fx = fixture("A\nB\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    for (line, want) in REFUSED {
        let err = run_inline(&env, None, &words(line), &Places::new()).err();
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
        let err = run_inline(&env, None, &words(line), &Places::new()).err();
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
    let err = run_inline(&env, None, &words("dep 1"), &Places::new()).err();
    let err = err.unwrap_or_else(|| panic!("dep 1 ran"));
    assert_eq!(err.hint(), Some("usage: lsty dep REFS on REFS"));
}

#[test]
fn starting_work_needs_an_identity() {
    let fx = fixture("A\n");
    let env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    let err =
        run_inline(&env, None, &words("next --start"), &Places::new()).err();
    let err = err.unwrap_or_else(|| panic!("next --start ran"));
    assert_eq!(err.code(), Code::NoIdentity);
}

#[test]
fn a_write_hands_back_its_outcome_and_a_read_its_lines() {
    let fx = fixture("A id:b\nB +w\nC +w\nX id:x\nD dep:x\n");
    let mut env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    env.record_undo = true;
    let shown = env.load().unwrap_or_else(|e| panic!("{}", e.message()));
    // The list shown holds `C`, `B`, `X` and `D`, not `A`: a number is a
    // place.
    let places = placed(&[3, 2, 4, 5]);
    let run = |line: &str, env: &ops::Env<'_>| {
        run_inline(env, None, &words(line), &places)
            .unwrap_or_else(|e| panic!("{}", e.message()))
    };
    let Inline::Wrote(mut outcome) = run("do 1", &env) else {
        panic!("a write hands back no outcome");
    };
    assert!(outcome.take_undo().is_some());
    let read = |line: &str, env: &ops::Env<'_>| -> Vec<String> {
        match run(line, env) {
            Inline::Read(lines) => lines.iter().map(|l| lossy(l)).collect(),
            Inline::Wrote(_) => panic!("a read hands back an outcome"),
        }
    };
    let first = |line: &str, env: &ops::Env<'_>| -> String {
        read(line, env).swap_remove(0)
    };
    assert_eq!(first("ls +w", &env), "2 B +w");
    // A task with no row is named by its id, a blocker by its place.
    assert_eq!(first("ls", &env), "id:b A id:b");
    assert_eq!(first("next", &env), "id:b A id:b");
    let shown_task: Vec<String> = read("show 4", &env);
    assert!(
        shown_task
            .iter()
            .any(|l| l.contains("blocked by") && l.contains("3 X")),
        "{shown_task:?}"
    );
    // A position is a place too, and past the last row it is the end.
    let todo = || {
        fs::read_to_string(fx.dir.path().join("todo.txt")).unwrap_or_default()
    };
    run("reorder 2 1", &env);
    assert_eq!(todo().lines().nth(2), Some("B +w"), "{}", todo());
    run("reorder 2 9", &env);
    assert!(todo().lines().last().is_some_and(|l| l.starts_with("x ")));
    // Files that are not the ones the list shows give no place.
    env.aim = Some(&shown);
    assert_eq!(first("ls +w", &env), "line 2 B +w");
}

/// The places of the lines of the todo file given, from 1.
fn placed(lines: &[u32]) -> Places {
    let nrs = lines.iter().filter_map(|n| Nr::new(*n));
    nrs.zip(1..)
        .map(|(nr, place)| ((Source::Todo, nr), place))
        .collect()
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

#[test]
fn a_name_or_alias_finds_the_command_that_runs_inline() {
    for (name, want) in [
        ("do", Some("do")),
        ("done", Some("do")),
        ("a", Some("add")),
        ("ls", Some("ls")),
        ("tui", None),
        ("help", None),
        ("nope", None),
    ] {
        assert_eq!(inline_command(name), want, "{name}");
    }
}

#[test]
fn every_inline_command_has_a_synopsis_and_a_summary() {
    let all: Vec<_> = inline_commands().collect();
    let add = ("add TEXT…", "Adds a task at the end of the list.");
    assert!(all.contains(&add), "{all:?}");
    assert!(all.iter().all(|(s, a)| !s.is_empty() && !a.is_empty()));
    assert!(all.iter().all(|(s, _)| !s.starts_with("tui")), "{all:?}");
}

#[test]
fn the_human_acts_whoever_the_environment_names() {
    let held = "Call client id:k3f9 started:2026-09-29T0800Z owner:agent2\n";
    let fx = fixture(held);
    let mut env = ops::Env::new(&fx.found, &fx.config, &fx.clock);
    env.actor =
        Actor::named("agent1").unwrap_or_else(|e| panic!("{}", e.message()));
    run_inline(&env, None, &words("do id:k3f9"), &Places::new())
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let data =
        fs::read_to_string(fx.dir.path().join("todo.txt")).unwrap_or_default();
    assert!(data.starts_with("x 2026-09-29 Call client"), "{data}");
}
