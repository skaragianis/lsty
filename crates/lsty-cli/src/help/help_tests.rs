use std::{collections::BTreeSet, ffi::OsString};

use super::{commands_help, overview};
use crate::{
    agent_guide::agent_guide,
    commands::{COMMANDS, Command, Handler},
    dispatch::USAGE_TEXT,
    matrix::{MATRIX, Retry, RetryKind, Row},
    parse::Opt,
    run_tests::Fake,
};

fn help_text(args: &[&str]) -> String {
    let ran = Fake::new().run(args);
    assert_eq!((ran.code, ran.err.as_str()), (0, ""), "{args:?}");
    ran.out
}

/// The lines of the `commands:` block of the overview.
fn command_block(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|l| *l != "commands:")
        .skip(1)
        .take_while(|l| !l.is_empty())
        .collect()
}

fn also(line: &str) -> Vec<String> {
    let Some((_, list)) = line.split_once("[also: ") else {
        return Vec::new();
    };
    let list = list.split_once(']').map_or(list, |(l, _)| l);
    list.split(", ").map(str::to_owned).collect()
}

/// The `--name` words of the text.
fn option_tokens(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("--") {
        let word: String = rest[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || *c == '-')
            .collect();
        if word.starts_with(|c: char| c.is_ascii_lowercase()) {
            found.insert(word);
        }
        rest = &rest[at + 2..];
    }
    found
}

#[test]
fn the_overview_lists_every_command_and_option_and_nothing_else() {
    let text = help_text(&["help"]);
    let mut names = Vec::new();
    let mut aliases = Vec::new();
    for line in command_block(&text) {
        if !line.starts_with("   ") {
            names.push(line.split_whitespace().next().unwrap().to_owned());
        }
        aliases.extend(also(line));
    }
    let want_names: Vec<_> =
        COMMANDS.iter().map(|c| c.name.to_owned()).collect();
    let want_aliases: Vec<_> = COMMANDS
        .iter()
        .flat_map(|c| c.aliases.iter().map(|a| (*a).to_owned()))
        .collect();
    assert_eq!(names, want_names, "27.6 commands in table order");
    assert_eq!(aliases, want_aliases, "27.6 aliases");
    assert!(Opt::ALL.is_sorted_by_key(|o| o.name()), "options by name");
    let want_options: BTreeSet<_> =
        Opt::ALL.iter().map(|o| o.name().to_owned()).collect();
    assert_eq!(option_tokens(&text), want_options, "27.6 options");
}

#[test]
fn the_overview_starts_with_the_usage_text_and_ends_with_the_environment() {
    let text = help_text(&["help"]);
    assert!(text.starts_with(&format!("{USAGE_TEXT}\n\ncommands:\n")));
    let last = text.lines().last().unwrap();
    assert!(last.starts_with("  NO_COLOR, TERM=dumb "), "{last}");
    assert!(text.contains("\n  3  the list disagrees with the call: "));
    assert!(text.contains("\n  0  done\n"));
}

#[test]
fn a_long_synopsis_has_its_sentence_on_the_next_line() {
    let text = overview(&COMMANDS).join("\n");
    assert!(
        text.contains(
            "\n  reorder REFS POS | --after REF | --before REF\n    "
        ),
        "{text}"
    );
    assert!(text.contains("\n  pri REFS PRI [REFS PRI…]  Sets the priority"));
}

#[test]
fn every_command_and_option_is_described() {
    for c in &COMMANDS {
        assert!(!c.about.is_empty(), "{} has no sentence", c.name);
        assert!(c.synopsis.starts_with(c.name), "{}", c.synopsis);
    }
}

#[test]
fn help_for_a_command_and_the_flag_agree() {
    let by_name = help_text(&["help", "do"]);
    let by_flag = help_text(&["do", "--help"]);
    assert!(by_name.starts_with("usage: lsty do REFS…\naliases: done\n"));
    for want in ["--ignore-blocked", "--takeover", "Retry: no-op"] {
        assert!(by_name.contains(want), "{want}");
    }
    assert_eq!(by_flag, by_name, "27.2");
    assert_eq!(help_text(&["--help", "do"]), by_name);
}

#[test]
fn several_commands_are_told_apart_by_a_blank_line() {
    let text = help_text(&["help", "up", "down"]);
    let blocks: Vec<_> = text
        .split("\n\n")
        .filter(|b| b.starts_with("usage:"))
        .collect();
    assert_eq!(blocks.len(), 2, "{text}");
    assert!(blocks[0].starts_with("usage: lsty up REFS [N]"));
}

#[test]
fn a_command_with_two_matrix_rows_names_each() {
    let text = help_text(&["help", "del"]);
    assert!(text.contains("\n  del REF:\n"), "{text}");
    assert!(text.contains("\n  del REF TERM:\n"));
    let text = help_text(&["help", "dep"]);
    assert!(text.contains("\n  dep, undep:") || !text.contains("\n  dep:\n"));
    assert!(text.contains("Agent reference: 2 × id:"));
}

#[test]
fn a_command_that_does_not_write_has_no_matrix() {
    let text = help_text(&["help", "ls"]);
    assert!(!text.contains("agents and retries"), "{text}");
    assert!(text.contains("--sort FIELD"));
    assert!(!text.contains("--takeover"));
}

#[test]
fn the_matrix_prints_effects_or_none() {
    let text = help_text(&["help", "pri"]);
    assert!(text.contains("  Side effects: none\n"), "{text}");
    let text = help_text(&["help", "start"]);
    assert!(
        text.contains("  Side effects:\n    - writes started: and owner:")
    );
    assert!(text.contains("  Checks: claim guard, blocked\n  Files: todo\n"));
}

fn frobnicate() -> (Command, Row) {
    let command = Command {
        name: "frobnicate",
        aliases: &[],
        writes: true,
        synopsis: "frobnicate REF",
        about: "Frobs.",
        opts: &[],
        handler: Handler::Tui,
    };
    let row = Row {
        command: "frobnicate NR",
        agent_ref: "id:",
        retry: Retry {
            kind: RetryKind::Repeats,
            when: "",
            unless: "",
        },
        checks: &[],
        files: &["todo"],
        effects: &["frobs"],
        renumbers: false,
        changes: false,
    };
    (command, row)
}

#[test]
fn a_new_matrix_row_appears_in_both_helps() {
    let (command, row) = frobnicate();
    let names = ["frobnicate".to_owned()];
    let help = commands_help(&[command], &[row], &names)
        .unwrap()
        .join("\n");
    assert!(help.contains("Retry: repeats"), "27.7 help:\n{help}");
    assert!(help.contains("- frobs"));
    let mut rows = MATRIX.to_vec();
    rows.push(row);
    let guide = agent_guide(&rows).join("\n");
    assert!(
        guide.contains("\n  frobnicate NR: id:; repeats\n"),
        "27.7 help agent:\n{guide}"
    );
}

#[test]
fn an_unknown_name_is_an_unknown_command() {
    let ran = Fake::new().run(&["help", "frob"]);
    assert_eq!(ran.code, 2);
    assert_eq!(
        ran.err,
        "lsty: unknown command \"frob\"\n\
         hint: run \"lsty help\" for the commands\n"
    );
    assert_eq!(ran.out, "");
    let ran = Fake::new().run(&["help", "a\x1bb"]);
    assert!(
        ran.err.contains("unknown command \"a\\x1bb\""),
        "{}",
        ran.err
    );
    let ran = Fake::new().run(&["help", "do", "frob"]);
    assert_eq!(ran.code, 2);
}

#[test]
fn help_takes_no_json_and_agent_takes_no_command() {
    let ran = Fake::new().run(&["help", "--json"]);
    assert_eq!(ran.code, 2);
    assert!(ran.out.contains("help does not take --json"));
    let ran = Fake::new().run(&["help", "--agent", "do"]);
    assert_eq!(ran.code, 2);
    assert!(ran.err.starts_with("lsty: help --agent takes no command\n"));
}

const PARTS: [&str; 11] = [
    "# Identity",
    "# The file",
    "# References",
    "# Output",
    "# Getting work",
    "# Adding",
    "# Dates",
    "# Claims",
    "# Semantics",
    "# The matrix",
    "# Exit codes",
];

#[test]
fn the_agent_guide_has_its_parts_in_order_within_its_size() {
    let text = help_text(&["help", "--agent"]);
    let lines: Vec<_> = text.lines().collect();
    assert!(lines.len() <= 120, "{} lines", lines.len());
    for line in &lines {
        assert!(line.len() <= 80, "{} bytes: {line}", line.len());
    }
    let mut at = None;
    for part in PARTS {
        let i = lines.iter().position(|l| *l == part);
        assert!(i.is_some() && i > at, "{part} at {i:?} after {at:?}");
        at = i;
    }
    for want in [
        "LSTY_OWNER",
        "--if-absent --id=",
        "found_by",
        "next --start",
    ] {
        assert!(text.contains(want), "{want}");
    }
}

#[test]
fn the_agent_guide_matrix_wraps_and_names_every_row() {
    let text = help_text(&["help", "--agent"]);
    assert!(text.contains(
        "\n  add: \u{2014}; repeats; no-op with --if-absent \
         (by --id=KEY always; by text on the\n      same day)\n"
    ));
    assert!(text.contains(
        "\n  id: NR + --expect, or id:; no-op; by NR after the lines \
         shifted: detected\n"
    ));
    for row in MATRIX {
        assert!(text.contains(&format!("\n  {}: ", row.command)), "{row:?}");
    }
    assert!(text.contains("\n1 the system failed: io\n"));
    assert!(text.contains("\n# Exit codes\n0 done\n"));
}

#[test]
fn help_needs_no_todo_file() {
    let fake = Fake::new().var("TODO_FILE", "/nowhere/missing/todo.txt");
    assert_eq!(fake.run(&["help"]).code, 0);
    assert_eq!(fake.run(&["help", "--agent"]).code, 0);
}

fn paged_fake() -> Fake {
    Fake::new().tty()
}

#[test]
fn on_a_terminal_the_pager_gets_the_text() {
    let fake = paged_fake().var("PAGER", "cat");
    let ran = fake.run(&["help"]);
    let want = format!("{}\n", overview(&COMMANDS).join("\n"));
    assert_eq!(ran.code, 0);
    assert_eq!(ran.out, "", "27.9 nothing is printed");
    assert_eq!(ran.paged, [(vec![OsString::from("cat")], want)]);
    let ran = fake.run(&["help", "do"]);
    assert!(ran.paged[0].1.starts_with("usage: lsty do REFS…\n"));
    let ran = fake.run(&["do", "--help"]);
    assert_eq!(ran.paged.len(), 1);
}

#[test]
fn the_pager_setting_is_split_at_spaces_and_defaults_to_less() {
    let ran = paged_fake().var("PAGER", "less  -R").run(&["help"]);
    assert_eq!(ran.paged[0].0, ["less", "-R"]);
    for setting in ["", "   "] {
        let ran = paged_fake().var("PAGER", setting).run(&["help"]);
        assert_eq!(ran.paged[0].0, ["less"], "{setting:?}");
    }
    let ran = paged_fake().run(&["help"]);
    assert_eq!(ran.paged[0].0, ["less"]);
}

#[test]
fn a_pager_that_cannot_start_leaves_the_text_printed() {
    let ran = paged_fake()
        .failing_pager()
        .var("PAGER", "nope")
        .run(&["help"]);
    let want = format!("{}\n", overview(&COMMANDS).join("\n"));
    assert_eq!(
        (ran.code, ran.out, ran.err.as_str()),
        (0, want, ""),
        "27.10"
    );
    assert_eq!(ran.paged.len(), 0);
}

#[test]
fn no_pager_without_a_terminal_or_on_a_dumb_one() {
    let ran = Fake::new().var("PAGER", "cat").run(&["help"]);
    assert!(ran.paged.is_empty() && ran.out.starts_with("lsty ("));
    let ran = paged_fake()
        .var("PAGER", "cat")
        .var("TERM", "dumb")
        .run(&["help"]);
    assert!(ran.paged.is_empty() && ran.out.starts_with("lsty ("));
    let ran = paged_fake().var("PAGER", "cat").run(&["help", "--agent"]);
    assert!(ran.paged.is_empty() && ran.out.starts_with("# Identity"));
}

#[test]
fn the_query_section_is_shown_where_a_filter_is_taken() {
    for args in [&["help"][..], &["help", "ls"], &["help", "next"]] {
        let text = help_text(args);
        assert!(text.contains("\nqueries:\n"), "{args:?}");
        assert!(text.contains("is:stale"), "{args:?}");
        assert!(text.contains("an argument needs -- first"), "{args:?}");
    }
    assert!(!help_text(&["help", "add"]).contains("queries:"));
    let guide = help_text(&["help", "--agent"]);
    assert!(guide.contains("# Queries\n"));
}
