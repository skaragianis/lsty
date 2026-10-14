use std::{fs, path::Path};

use lsty_core::{
    config::{self, Config, Paths},
    keys::{self, Action, Mode, about, defaults, key_names, parse_keys},
};

use super::{EXAMPLE, GUIDE_WIDTH, keys_page};

/// The lines of mode `mode`'s section.
fn section(page: &[String], mode: Mode) -> Vec<&str> {
    page.iter()
        .skip_while(|l| *l != mode.as_str())
        .skip(1)
        .take_while(|l| !l.is_empty())
        .map(String::as_str)
        .collect()
}

#[test]
fn every_default_binding_is_listed_once_in_its_mode() {
    let page = keys_page();
    for mode in Mode::ALL {
        let lines = section(&page, mode);
        for &(action, written) in defaults(mode) {
            let head = format!("    {action} ");
            let found: Vec<&&str> =
                lines.iter().filter(|l| l.starts_with(&head)).collect();
            assert_eq!(found.len(), 1, "{} {action}", mode.as_str());
            let keys = format!("  {}  ", written.join(", "));
            assert!(found[0].contains(&keys), "{} {action}", mode.as_str());
            let first = about(mode, action).split(' ').next().unwrap();
            assert!(found[0].contains(first));
        }
    }
    for action in Action::ALL {
        assert!(page.iter().any(|l| l.contains(&format!(" {action} "))));
    }
}

#[test]
fn actions_are_listed_in_the_order_of_the_table() {
    let page = keys_page();
    for mode in Mode::ALL {
        let mut last = None;
        for line in section(&page, mode) {
            let Some(name) = line.strip_prefix("    ") else {
                last = None;
                continue;
            };
            let name = name.split(' ').next().unwrap();
            let at = Action::ALL.iter().position(|a| a.to_string() == name);
            if at.is_some() {
                assert!(last < at, "{} {name}", mode.as_str());
                last = at;
            }
        }
    }
}

#[test]
fn lines_fit_the_guide_width() {
    for line in keys_page() {
        assert!(line.len() <= GUIDE_WIDTH, "{line}");
    }
}

#[test]
fn the_notation_lists_every_key_name_and_reads() {
    let page = keys_page().join("\n");
    for name in key_names() {
        assert!(page.contains(&format!("<{name}>")), "{name}");
    }
    for written in ["<C-d>", "<A-x>", "<S-Tab>", "<C-A-x>", "<lt>", "<gt>"] {
        assert!(parse_keys(written, None).is_ok(), "{written}");
    }
    let leader = parse_keys(&Config::default().leader, None).unwrap()[0];
    assert!(parse_keys("<leader>d", Some(leader)).is_ok());
}

/// The lines of the example from `heading` to the next blank line, as
/// the file they show.
fn example_file(heading: &str) -> String {
    let lines: Vec<&str> = EXAMPLE
        .iter()
        .skip_while(|l| **l != heading)
        .skip(1)
        .take_while(|l| !l.is_empty())
        .map(|l| l.trim_start())
        .collect();
    lines.join("\n") + "\n"
}

#[test]
fn the_example_loads_without_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let keys_file = dir.path().join("keys.toml");
    let config_file = dir.path().join("config.toml");
    fs::write(&keys_file, example_file("keys.toml:")).unwrap();
    fs::write(&config_file, example_file("config.toml:")).unwrap();
    let (_, warnings) = keys::load(
        &keys_file,
        Path::new("/"),
        false,
        &Config::default().leader,
    );
    assert_eq!(warnings, vec![]);
    let paths = Paths {
        config_file,
        keys_file,
        themes_dir: dir.path().join("themes"),
        state_dir: dir.path().join("state"),
        explicit: true,
    };
    let (cfg, warnings) = config::load(&paths, Path::new("/"), false);
    assert_eq!(warnings, vec![]);
    for name in &cfg.hints["normal"] {
        assert!(Action::from_name(name).is_some(), "{name}");
    }
}
