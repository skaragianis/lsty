use std::fs;
use std::time::Duration;

use lsty_core::config::{Config, Hidden, LineNumbers};
use lsty_core::todotxt::Priority;

use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::output::Output;
use crate::setform::{Form, OPTIONS, Op, parse};
use crate::setshow::{duration_text, value_text};
use crate::test_util::{
    SAMPLE, alone, deps, keys, press, screen, started, workdir, workdir_model,
};
use crate::words::split_words;

/// A line and its words, or `None` for a quote left open.
type Split = (&'static str, Option<&'static [&'static str]>);

const WORDS: &[Split] = &[
    ("", Some(&[])),
    ("   ", Some(&[])),
    ("do 1 2", Some(&["do", "1", "2"])),
    ("do   1", Some(&["do", "1"])),
    (r#"add "Buy milk""#, Some(&["add", "Buy milk"])),
    ("add 'Buy milk'", Some(&["add", "Buy milk"])),
    (r#"add """#, Some(&["add", ""])),
    (r#"a"b c"d"#, Some(&["ab cd"])),
    (r#"add "say \"hi\"""#, Some(&["add", r#"say "hi""#])),
    (r#"add "a\\b""#, Some(&["add", r"a\b"])),
    (r#"add "a\nb""#, Some(&["add", r"a\nb"])),
    (r#"add 'a\b "c"'"#, Some(&["add", r#"a\b "c""#])),
    (r"add a\b", Some(&["add", r"a\b"])),
    (r#"add "open"#, None),
    ("add 'open", None),
    ("ls *", Some(&["ls", "*"])),
];

#[test]
fn words_are_cut_at_spaces_and_grouped_by_quotes() {
    for (line, want) in WORDS {
        let got = split_words(line);
        let want = want.map(|w| w.iter().map(|s| (*s).to_owned()).collect());
        assert_eq!(got, want, "line {line:?}");
    }
}

/// A word, then the name, the operation and the value it is read as.
type Reading = (&'static str, &'static str, Op, &'static str);

const FORMS: &[Reading] = &[
    ("auto_archive", "auto_archive", Op::On, ""),
    ("noauto_archive", "auto_archive", Op::Off, ""),
    ("nu", "nu", Op::On, ""),
    ("nonu", "nu", Op::Off, ""),
    ("nornu", "rnu", Op::Off, ""),
    ("priority_range=F", "priority_range", Op::Assign, "F"),
    ("hidden_items+=blocked", "hidden_items", Op::Add, "blocked"),
    ("hidden_items-=later", "hidden_items", Op::Remove, "later"),
    ("priority_range?", "priority_range", Op::Query, ""),
    ("nofrob", "nofrob", Op::On, ""),
    ("default_priority=", "default_priority", Op::Assign, ""),
    ("lock_timeout=1s=2", "lock_timeout", Op::Assign, "1s=2"),
];

#[test]
fn a_set_word_is_taken_apart() {
    for (word, name, op, value) in FORMS {
        let want = Form {
            name: (*name).to_owned(),
            op: *op,
            value: (*value).to_owned(),
        };
        assert_eq!(parse(word), want, "word {word:?}");
    }
}

const DURATIONS: &[(Duration, &str)] = &[
    (Duration::from_secs(2), "2s"),
    (Duration::from_secs(10), "10s"),
    (Duration::from_hours(4), "4h"),
    (Duration::from_mins(90), "1h30m"),
    (Duration::from_secs(90), "1m30s"),
    (Duration::from_millis(50), "50ms"),
    (Duration::from_micros(1500), "1.5ms"),
    (Duration::from_nanos(7), "7ns"),
    (Duration::from_secs(120), "2m"),
];

#[test]
fn a_duration_reads_short() {
    for (d, want) in DURATIONS {
        assert_eq!(duration_text(*d), *want, "{d:?}");
    }
}

fn line(model: &mut Model, text: &str) -> Vec<Cmd> {
    press(model, ":");
    press(model, text);
    keys(model, "<CR>")
}

#[test]
fn the_quit_commands_quit() {
    for text in ["q", "wq", "x"] {
        let (_dir, mut model) = workdir_model(SAMPLE, alone());
        assert_eq!(line(&mut model, text), vec![Cmd::Quit], "{text}");
    }
}

#[test]
fn w_writes_nothing_and_closes_the_line() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    assert_eq!(line(&mut model, "w"), Vec::<Cmd>::new());
    assert!(model.line.is_none());
    let data = fs::read_to_string(dir.path().join("todo.txt"));
    assert_eq!(data.as_deref().ok(), Some(SAMPLE));
}

fn status(model: &Model) -> String {
    screen(model, 80, 24).last().cloned().unwrap_or_default()
}

/// The line above the status line.
fn above(model: &Model) -> String {
    let lines = screen(model, 80, 24);
    lines
        .get(lines.len().saturating_sub(2))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn set_without_an_option_says_it_needs_one() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "set");
    assert!(model.line.is_some());
    assert!(
        above(&model).contains(":set needs an option")
            || status(&model).contains(":set needs an option"),
        "{}",
        status(&model)
    );
    assert!(!status(&model).contains(":set: needs"));
}

#[test]
fn a_set_word_that_is_refused_changes_nothing() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "set auto_archive priority_range=ZZ");
    assert!(!model.deps.config.auto_archive);
    assert!(model.line.is_some());
    assert!(
        status(&model).contains(":set auto_archive"),
        "{}",
        status(&model)
    );
}

#[test]
fn set_lasts_in_the_session_copy_of_the_options() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(
        &mut model,
        "set auto_archive priority_range=F lock_timeout=3s",
    );
    assert!(model.deps.config.auto_archive);
    assert_eq!(model.deps.config.priority_range.letter(), b'F');
    assert_eq!(model.deps.config.lock_timeout, Duration::from_secs(3));
    line(&mut model, "set lock_timeout?");
    assert!(status(&model).contains("lock_timeout=3s"));
}

#[test]
fn an_option_of_the_file_that_is_not_runtime_is_refused() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "set leader=x");
    assert!(
        above(&model).contains("leader cannot be set at run time"),
        "{}",
        above(&model)
    );
}

#[test]
fn undo_and_redo_run_from_the_line() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "do 1");
    line(&mut model, "undo");
    let data = fs::read_to_string(dir.path().join("todo.txt"));
    assert_eq!(data.as_deref().ok(), Some(SAMPLE));
    line(&mut model, "redo");
    let data =
        fs::read_to_string(dir.path().join("todo.txt")).unwrap_or_default();
    assert!(data.starts_with("x "), "{data}");
}

#[test]
fn nothing_to_undo_closes_the_line_with_a_message() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "undo");
    assert!(model.line.is_none());
    assert!(status(&model).contains("nothing to undo"));
}

#[test]
fn a_refused_undo_keeps_the_line_open_with_its_text() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "do 1");
    let todo = dir.path().join("todo.txt");
    fs::write(&todo, "x 2026-09-29 Changed outside\nBuy milk +home\n")
        .unwrap_or_else(|e| panic!("{e}"));
    line(&mut model, "undo");
    let text = model.line.as_ref().map(|l| l.field.text());
    assert_eq!(text.as_deref(), Some("undo"));
}

#[test]
fn a_read_opens_a_popup_that_scrolls_and_closes() {
    let (_dir, mut model) = workdir_model(
        "A +w\nB +w\nC +w\nD +w\nE +w\nF +w\nG +w\nH +w\n",
        alone(),
    );
    model.update(Msg::Resize(80, 8));
    line(&mut model, "ls +w");
    assert!(model.output.is_some());
    assert!(model.line.is_none());
    press(&mut model, "j");
    assert_eq!(model.output.as_ref().map(Output::top), Some(1));
    press(&mut model, "q");
    assert!(model.output.is_none());
}

/// The text before the word, the word, and what it may become.
type Pool = (&'static str, &'static str, &'static [&'static str]);

const POOLS: &[Pool] = &[
    ("add ", "+", &["+home", "+work"]),
    ("add ", "@", &["@phone"]),
    ("add ", "file:", &["file:notes.md", "file:todo.txt"]),
    ("sort due ", "", &[]),
    ("add ", "", &[]),
];

#[test]
fn the_line_completes_names_files_and_set_options() {
    let dir = workdir("(A) Call client +work @phone\nBuy milk +home\n");
    fs::write(dir.path().join("notes.md"), "").unwrap();
    let mut model = started(deps(dir.path(), alone()));
    for (before, word, want) in POOLS {
        let got = model.command_pool(word, before);
        assert_eq!(got, *want, "{before}{word}");
    }
    let dotted = model.command_pool("--", "\u{130}d ");
    assert_eq!(dotted, model.command_pool("--", "id "));
    assert_ne!(dotted, Vec::<String>::new());
    let set = model.command_pool("", "set ");
    assert!(set.iter().any(|n| n == "auto_archive"), "{set:?}");
    assert!(set.iter().any(|n| n == "noauto_archive"), "{set:?}");
    assert!(!set.iter().any(|n| n == "nolock_timeout"), "{set:?}");
}

/// An option and what `:set NAME?` says about it.
type Shown = (&'static str, &'static str);

const SHOWN: &[Shown] = &[
    ("nu", "nonu"),
    ("rnu", "nornu"),
    ("show_sidebar", "show_sidebar"),
    ("show_detail", "noshow_detail"),
    ("inline_projects", "inline_projects"),
    ("inline_contexts", "inline_contexts"),
    ("auto_archive", "auto_archive"),
    ("keep_priority", "keep_priority"),
    ("keep_adding", "nokeep_adding"),
    ("include_creation", "noinclude_creation"),
    ("priority_range", "priority_range=F"),
    ("default_priority", "default_priority=B"),
    ("upcoming_days", "upcoming_days=3"),
    ("line_numbers", "line_numbers=off"),
    ("claim_stale_after", "claim_stale_after=1h30m"),
    ("lock_timeout", "lock_timeout=1.5s"),
    ("sidebar_min_width", "sidebar_min_width=60"),
    ("detail_min_width", "detail_min_width=90"),
    ("hidden_items", "hidden_items=later,done"),
];

#[test]
fn set_shows_the_value_of_every_option() {
    let cfg = Config {
        show_sidebar: true,
        show_detail: false,
        inline_projects: true,
        inline_contexts: true,
        auto_archive: true,
        keep_priority: true,
        keep_adding: false,
        include_creation: false,
        priority_range: Priority::new(b'F').unwrap(),
        default_priority: Priority::new(b'B'),
        upcoming_days: 3,
        line_numbers: LineNumbers::Off,
        claim_stale_after: Duration::from_mins(90),
        lock_timeout: Duration::from_millis(1500),
        sidebar_min_width: 60,
        detail_min_width: 90,
        hidden_items: vec![Hidden::Later, Hidden::Done],
        ..Config::default()
    };
    assert!(SHOWN.iter().map(|s| s.0).eq(OPTIONS.iter().map(|o| o.0)));
    for (name, want) in SHOWN {
        assert_eq!(value_text(&cfg, name), *want, "{name}");
    }
    let none = Config {
        default_priority: None,
        hidden_items: Vec::new(),
        ..Config::default()
    };
    assert_eq!(value_text(&none, "default_priority"), "default_priority=");
    assert_eq!(value_text(&none, "hidden_items"), "hidden_items=");
}
