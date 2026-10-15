use std::collections::BTreeSet;

use super::{
    commands::{COMMANDS, Handler, TUI, find},
    parse::Opt,
};

#[test]
fn a_name_is_found_with_the_simple_lower_case() {
    for (typed, want) in [("ID", "id"), ("\u{130}d", "id"), ("LS", "ls")] {
        assert_eq!(find(typed).map(|c| c.name), Some(want), "{typed}");
    }
}

#[test]
fn names_and_aliases_are_unique() {
    let mut seen = BTreeSet::new();
    for c in &COMMANDS {
        assert!(seen.insert(c.name), "{}", c.name);
        for a in c.aliases {
            assert!(seen.insert(a), "{a}");
        }
    }
}

#[test]
fn every_command_has_a_synopsis_that_starts_with_its_name() {
    for c in &COMMANDS {
        assert!(c.synopsis.starts_with(c.name), "{}", c.name);
    }
}

#[test]
fn json_is_taken_by_all_but_seven() {
    for c in &COMMANDS {
        let refused = [
            "tui",
            "help",
            "edit",
            "open",
            "shorthelp",
            "command",
            "listaddons",
        ]
        .contains(&c.name);
        assert_eq!(c.takes(Opt::Json), !refused, "{}", c.name);
    }
}

#[test]
fn owner_and_human_are_taken_by_all_but_five() {
    for c in &COMMANDS {
        let refused = ["tui", "help", "shorthelp", "command", "listaddons"]
            .contains(&c.name);
        assert_eq!(c.takes(Opt::Owner), !refused, "{}", c.name);
        assert_eq!(c.takes(Opt::Human), !refused, "{}", c.name);
    }
}

#[test]
fn create_is_taken_by_writes_and_next() {
    for c in &COMMANDS {
        let takes = c.writes || c.name == "next";
        assert_eq!(c.takes(Opt::Create), takes, "{}", c.name);
    }
}

#[test]
fn common_options_are_taken_by_every_command() {
    for c in &COMMANDS {
        for o in [
            Opt::File,
            Opt::NoConfig,
            Opt::Color,
            Opt::Help,
            Opt::Version,
        ] {
            assert!(c.takes(o), "{} {}", c.name, o.name());
        }
    }
}

#[test]
fn edit_and_open_need_a_terminal() {
    for c in &COMMANDS {
        let tty = matches!(c.handler, Handler::Tty(_));
        assert_eq!(tty, ["edit", "open"].contains(&c.name), "{}", c.name);
    }
}

#[test]
fn lookup_ignores_case() {
    assert_eq!(find("LsA").map(|c| c.name), Some("lsa"));
    assert_eq!(find("nope").map(|c| c.name), None);
    assert_eq!(find("").map(|c| c.name), None);
}

#[test]
fn only_tui_has_the_tui_handler() {
    assert_eq!(TUI.name, "tui");
    for c in &COMMANDS {
        let is_tui = matches!(c.handler, Handler::Tui);
        assert_eq!(is_tui, c.name == "tui", "{}", c.name);
    }
}
