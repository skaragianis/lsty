use std::{fs, time::Duration};

use lsty_core::{
    config::{Config, DetailPosition, Hidden, LineNumbers, is_option},
    todotxt::Priority,
};

use crate::{
    focus::Focus,
    model::Model,
    msg::{Cmd, Msg},
    output::Output,
    setform::{Form, OPTIONS, Op, parse},
    setshow::{duration_text, value_text},
    test_util::{
        SAMPLE, alone, deps, keys, press, screen, started, todo_of, workdir,
        workdir_model,
    },
    words::split_words,
};

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
    ("a\tb\u{a0}c  d", Some(&["a", "b\u{a0}c", "d"])),
];

#[test]
fn words_are_cut_at_spaces_and_tabs_and_grouped_by_quotes() {
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
    ("hidden_tasks+=blocked", "hidden_tasks", Op::Add, "blocked"),
    ("hidden_tasks-=later", "hidden_tasks", Op::Remove, "later"),
    ("priority_range?", "priority_range", Op::Query, ""),
    ("nofrob", "nofrob", Op::On, ""),
    ("default_priority=", "default_priority", Op::Assign, ""),
    ("lock_timeout=1s=2", "lock_timeout", Op::Assign, "1s=2"),
    ("=5", "=5", Op::On, ""),
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
    (Duration::from_hours(12), "12h"),
    (Duration::from_mins(10), "10m"),
    (Duration::from_mins(90), "1h30m"),
    (Duration::from_secs(90), "1m30s"),
    (Duration::from_millis(50), "50ms"),
    (Duration::from_millis(1500), "1.5s"),
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
    for text in [
        "q", "q!", "qa", "qa!", "wq", "wq!", "x", "x!", "quit", "quit!",
        "wqa", "wqa!", "xa", "xa!", "qall", "qall!", "quita", "quitall",
        "wqall", "xall", "exit", "exit!", "QUIT!",
    ] {
        let (_dir, mut model) = workdir_model(SAMPLE, alone());
        assert_eq!(line(&mut model, text), vec![Cmd::Quit], "{text}");
    }
}

#[test]
fn only_the_quit_names_take_a_bang() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    assert_eq!(line(&mut model, "w!"), Vec::<Cmd>::new());
    assert!(model.line.is_some(), "w! is not a command");
}

#[test]
fn w_writes_nothing_and_closes_the_line() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    assert_eq!(line(&mut model, "w"), Vec::<Cmd>::new());
    assert!(model.line.is_none());
    assert_eq!(todo_of(dir.path()), SAMPLE);
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
fn the_number_flags_refuse_a_value() {
    for name in ["nu", "rnu"] {
        let (_dir, mut model) = workdir_model(SAMPLE, alone());
        line(&mut model, &format!("set {name}=1"));
        let want = format!(":set: {name}: expected bool");
        assert_eq!(above(&model), want, "{name}");
    }
}

#[test]
fn set_shows_and_hides_the_panels_the_options_name() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "set show_sidebar");
    assert!(model.sidebar_on);
    assert!(!model.detail_on);
    line(&mut model, "set show_detail noshow_sidebar");
    assert!(!model.sidebar_on);
    assert!(model.detail_on);
}

#[test]
fn set_rebuilds_the_list_for_the_options_that_change_it() {
    let todo = "A\nx 2026-09-28 B\nC started:2026-09-29T0700Z owner:me\n";
    let (_dir, mut model) = workdir_model(todo, alone());
    assert_eq!(model.rows.len(), 3);
    line(&mut model, "set hidden_tasks=done");
    assert_eq!(model.rows.len(), 2);
    line(&mut model, "set hidden_tasks=");
    assert_eq!(model.rows.len(), 3);
    assert!(!status(&model).contains("stale"), "{}", status(&model));
    line(&mut model, "set claim_stale_after=1h");
    assert!(status(&model).contains("(1 stale)"), "{}", status(&model));
}

#[test]
fn set_builds_the_rows_again_for_the_days_and_the_ids() {
    let (_dir, mut model) = workdir_model("A id:x due:2026-10-08\n", alone());
    let (width, slot) = {
        let row = &model.rows[0];
        (row.text_width, row.date.slot)
    };
    line(&mut model, "set upcoming_days=9");
    assert_ne!(model.rows[0].date.slot, slot);
    line(&mut model, "set noshow_ids");
    assert_ne!(model.rows[0].text_width, width);
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
    assert_eq!(todo_of(dir.path()), SAMPLE);
    line(&mut model, "redo");
    let data = todo_of(dir.path());
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
fn the_line_completes_a_name_that_holds_the_typed_text() {
    let dir = workdir("(A) Call client +work @phone\nBuy milk +home\n");
    let mut model = started(deps(dir.path(), alone()));
    press(&mut model, ":add +ork");
    keys(&mut model, "<Tab>");
    let text = model.line.as_ref().map(|l| l.field.text());
    assert_eq!(text.as_deref(), Some("add +work"));
    keys(&mut model, "<Esc>");
    press(&mut model, ":theme ord");
    keys(&mut model, "<Tab>");
    let text = model.line.as_ref().map(|l| l.field.text());
    assert_eq!(text.as_deref(), Some("theme nord"));
}

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

#[test]
fn the_line_completes_for_a_prefix_of_a_command() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    for (short, full, word) in [("so", "sort", ""), ("ap", "append", "--")] {
        let pool = model.command_pool(word, &format!("{full} "));
        assert_ne!(pool, Vec::<String>::new(), "{full}");
        assert_eq!(model.command_pool(word, &format!("{short} ")), pool);
    }
    assert_eq!(model.command_pool("+", "s "), Vec::<String>::new());
}

/// An option and what `:set NAME?` says about it.
type Shown = (&'static str, &'static str);

const SHOWN: &[Shown] = &[
    ("nu", "nonu"),
    ("rnu", "nornu"),
    ("show_sidebar", "show_sidebar"),
    ("show_detail", "noshow_detail"),
    ("show_ids", "noshow_ids"),
    ("wrap", "wrap"),
    ("auto_archive", "auto_archive"),
    ("keep_priority", "keep_priority"),
    ("keep_adding", "keep_adding"),
    ("inherit_project", "noinherit_project"),
    ("inherit_context", "inherit_context"),
    ("mouse", "nomouse"),
    ("include_creation", "noinclude_creation"),
    ("priority_range", "priority_range=F"),
    ("default_priority", "default_priority=B"),
    ("upcoming_days", "upcoming_days=3"),
    ("line_numbers", "line_numbers=off"),
    ("detail_position", "detail_position=bottom"),
    ("claim_stale_after", "claim_stale_after=1h30m"),
    ("lock_timeout", "lock_timeout=1.5s"),
    ("sidebar_min_width", "sidebar_min_width=60"),
    ("detail_min_width", "detail_min_width=90"),
    ("hidden_tasks", "hidden_tasks=later,done"),
];

#[test]
fn set_shows_the_value_of_every_option() {
    let cfg = Config {
        show_sidebar: true,
        show_detail: false,
        show_ids: false,
        wrap: true,
        auto_archive: true,
        keep_priority: true,
        keep_adding: true,
        inherit_project: false,
        inherit_context: true,
        mouse: false,
        include_creation: false,
        priority_range: Priority::new(b'F').unwrap(),
        default_priority: Priority::new(b'B'),
        upcoming_days: 3,
        line_numbers: LineNumbers::Off,
        detail_position: DetailPosition::Bottom,
        claim_stale_after: Duration::from_mins(90),
        lock_timeout: Duration::from_millis(1500),
        sidebar_min_width: 60,
        detail_min_width: 90,
        hidden_tasks: vec![Hidden::Later, Hidden::Done],
        ..Config::default()
    };
    assert!(SHOWN.iter().map(|s| s.0).eq(OPTIONS.iter().map(|o| o.0)));
    for (name, want) in SHOWN {
        assert_eq!(value_text(&cfg, name), *want, "{name}");
    }
    let none = Config {
        default_priority: None,
        hidden_tasks: Vec::new(),
        ..Config::default()
    };
    assert_eq!(value_text(&none, "default_priority"), "default_priority=");
    assert_eq!(value_text(&none, "hidden_tasks"), "hidden_tasks=");
}

#[test]
fn the_line_counts_its_words_as_the_command_reads_them() {
    let dir = workdir("A\n");
    let mut model = started(deps(dir.path(), alone()));
    let sort = model.command_pool("", "sort ");
    assert_ne!(sort, Vec::<String>::new());
    assert_eq!(model.command_pool("", "\"sort\" "), sort);
    assert_eq!(model.command_pool("", "add \"open "), Vec::<String>::new());
}

#[test]
fn every_option_of_set_is_an_option_of_the_config() {
    for (name, _) in OPTIONS {
        let alias = name == "nu" || name == "rnu";
        assert!(alias || is_option(name), "{name}");
    }
}

const NUMBERED: &str = "(C) one\n(A) two\n\nthree +p\n(B) four\nx five\n";

/// A numbered list whose view hides the done task.
fn numbered() -> (tempfile::TempDir, Model) {
    let config = Config {
        hidden_tasks: vec![Hidden::Done],
        ..alone()
    };
    workdir_model(NUMBERED, config)
}

fn cursor_nr(model: &Model) -> Option<u32> {
    model.cursor_place().map(|(_, nr)| nr.get())
}

#[test]
fn a_number_on_the_line_puts_the_cursor_on_that_task() {
    let (_dir, mut model) = numbered();
    line(&mut model, "4");
    assert_eq!(cursor_nr(&model), Some(4));
    assert!(model.line.is_none());
    line(&mut model, "  2  ");
    assert_eq!(cursor_nr(&model), Some(2));
    line(&mut model, "004");
    assert_eq!(cursor_nr(&model), Some(4));
}

#[test]
fn a_number_lands_in_a_sorted_and_a_filtered_view() {
    let (_dir, mut model) = numbered();
    line(&mut model, "sort priority");
    line(&mut model, "1");
    assert_eq!(cursor_nr(&model), Some(1));
    assert_eq!(model.cursor, 2, "the row of (C) in priority order");
    line(&mut model, "5");
    assert_eq!(cursor_nr(&model), Some(5));
    assert_eq!(model.cursor, 1);
    keys(&mut model, "/o<CR>");
    assert_eq!(model.rows.len(), 3);
    line(&mut model, "2");
    assert_eq!(cursor_nr(&model), Some(2));
    line(&mut model, "5");
    assert_eq!(cursor_nr(&model), Some(5));
    line(&mut model, "4");
    assert_eq!(cursor_nr(&model), Some(5), "task 4 is filtered out");
}

#[test]
fn a_number_without_a_shown_task_leaves_the_cursor() {
    let (_dir, mut model) = numbered();
    line(&mut model, "2");
    let cases = [
        ("6", "task 6 is not shown"),
        ("3", "no task 3"),
        ("0", "no task 0"),
        ("7", "no task 7"),
        ("99999999999999999999999", "no task 99999999999999999999999"),
    ];
    for (typed, said) in cases {
        line(&mut model, typed);
        assert_eq!(cursor_nr(&model), Some(2), "{typed}");
        assert!(model.line.is_some(), "{typed}");
        let shown = format!("{} {}", above(&model), status(&model));
        assert!(shown.contains(said), "{typed}: {shown}");
        keys(&mut model, "<Esc>");
    }
}

#[test]
fn a_number_moves_the_list_cursor_from_a_panel() {
    let (_dir, mut model) = numbered();
    model.set_focus(Focus::Projects);
    line(&mut model, "5");
    assert_eq!(model.focus, Focus::List);
    assert_eq!(cursor_nr(&model), Some(5));
}

#[test]
fn a_number_with_other_words_is_still_a_command() {
    let (_dir, mut model) = numbered();
    line(&mut model, "4 5");
    assert_eq!(cursor_nr(&model), Some(1));
    assert!(model.line.is_some());
}

// The line and the search prompt edit as vim does: `<C-h>` deletes back,
// `<C-b>` goes to the start, `<C-e>` to the end, `<C-a>` does nothing.
#[test]
fn the_line_and_search_follow_vim_editing_keys() {
    let rows = [
        ("<C-h> deletes", "abc<C-h>", ("ab", 2)),
        ("<C-b> goes to the start", "abc<C-b>", ("abc", 0)),
        ("<C-e> goes to the end", "abc<C-b><C-e>", ("abc", 3)),
        ("<C-a> does not move", "abc<Left><C-a>", ("abc", 2)),
    ];
    for opener in [":", "/"] {
        for (name, typed, want) in rows {
            let (_dir, mut m) = numbered();
            keys(&mut m, &format!("{opener}{typed}"));
            let field = &m.line.as_ref().expect("the line is open").field;
            assert_eq!(
                (field.text().as_str(), field.cursor),
                want,
                "{opener} {name}"
            );
        }
    }
}

#[test]
fn a_line_that_cannot_run_is_refused() {
    let refused = [
        ("s", ":s is ambiguous: set, show, sort, start, stop"),
        ("und", ":und is ambiguous: undep, undo"),
        ("w!", "unknown command \"w!\""),
        ("ZZ", "unknown command \"ZZ\""),
        ("q now", ":quit takes no arguments"),
        ("e other.txt", ":quit takes no arguments"),
        ("w x", ":w takes no arguments"),
        ("undo x", ":undo takes no arguments"),
        ("red 2", ":redo takes no arguments"),
    ];
    for (text, says) in refused {
        let (dir, mut model) = workdir_model(SAMPLE, alone());
        line(&mut model, "prepe 1 a");
        line(&mut model, "prepe 1 b");
        line(&mut model, "undo");
        let before = todo_of(dir.path());
        assert_eq!(line(&mut model, text), Vec::<Cmd>::new(), "{text}");
        assert!(model.line.is_some(), "{text}");
        let shown = format!("{} {}", above(&model), status(&model));
        assert!(shown.contains(says), "{text}: {shown}");
        assert_eq!(todo_of(dir.path()), before, "{text}");
    }
}

#[test]
fn a_unique_prefix_runs_the_command() {
    for text in ["qu", "QU", "qu!", "e"] {
        let (_dir, mut model) = workdir_model(SAMPLE, alone());
        assert_eq!(line(&mut model, text), vec![Cmd::Quit], "{text}");
    }
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "hel");
    assert!(model.output.is_some());
}

#[test]
fn a_unique_prefix_of_an_inline_command_keeps_its_arguments() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    line(&mut model, "prepe 1 front");
    let data = todo_of(dir.path());
    assert!(data.starts_with("(A) front "), "{data}");
}
