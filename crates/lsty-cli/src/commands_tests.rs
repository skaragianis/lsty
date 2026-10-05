use std::collections::BTreeSet;

use super::commands::{COMMANDS, Handler, Kind, find};
use super::parse::Opt;

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
fn json_is_taken_by_all_but_four() {
    for c in &COMMANDS {
        let refused = ["tui", "help", "edit", "open"].contains(&c.name);
        assert_eq!(c.takes(Opt::Json), !refused, "{}", c.name);
    }
}

#[test]
fn owner_and_human_are_taken_by_all_but_two() {
    for c in &COMMANDS {
        let refused = ["tui", "help"].contains(&c.name);
        assert_eq!(c.takes(Opt::Owner), !refused, "{}", c.name);
        assert_eq!(c.takes(Opt::Human), !refused, "{}", c.name);
    }
}

#[test]
fn create_is_taken_by_writes_and_next() {
    for c in &COMMANDS {
        let takes = c.kind == Kind::Write || c.name == "next";
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
        assert_eq!(c.tty, ["edit", "open"].contains(&c.name), "{}", c.name);
    }
}

#[test]
fn lookup_ignores_case() {
    assert_eq!(find("LsA").map(|c| c.name), Some("lsa"));
    assert_eq!(find("nope").map(|c| c.name), None);
    assert_eq!(find("").map(|c| c.name), None);
}

#[test]
fn only_tui_has_the_tui_handler_and_other_kinds_run_bare() {
    for c in &COMMANDS {
        let is_tui = matches!(c.handler, Handler::Tui);
        assert_eq!(is_tui, c.name == "tui", "{}", c.name);
        let list = matches!(c.handler, Handler::Read(_) | Handler::Write(_));
        assert_eq!(list, c.kind != Kind::Other, "{}", c.name);
    }
}
