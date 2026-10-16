use std::{fs, path::Path};

use lsty_core::theme::{self, Colour, DEFAULT_NAME, FALLBACK, Slot, Theme};

use super::{EXAMPLE, GUIDE_WIDTH, themes_page};

/// The lines under `heading`, up to the next blank line.
fn section<'a>(page: &'a [String], heading: &str) -> Vec<&'a str> {
    page.iter()
        .skip_while(|l| *l != heading)
        .skip(1)
        .take_while(|l| !l.is_empty())
        .map(String::as_str)
        .collect()
}

/// Loads a theme file with `text` called `name`, and its warnings.
fn load_file(name: &str, text: &str) -> (Theme, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
    let (theme, warnings) =
        theme::load(name, dir.path(), Path::new("/"), false);
    (theme, warnings.iter().map(|w| w.message.clone()).collect())
}

#[test]
fn every_slot_is_listed_once_with_what_it_paints() {
    let page = themes_page();
    let lines = section(&page, "slots:");
    assert_eq!(lines.len(), Slot::ALL.len());
    for slot in Slot::ALL {
        let head = format!("  {} ", slot.name());
        let found: Vec<_> =
            lines.iter().filter(|l| l.starts_with(&head)).collect();
        assert_eq!(found.len(), 1, "{}", slot.name());
        assert!(found[0].ends_with(slot.about()), "{}", slot.name());
    }
}

#[test]
fn the_page_says_what_a_theme_is_and_where_a_name_falls_back() {
    let page = themes_page();
    let text = page
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for want in [
        "A theme gives each part of the screen a look.".to_owned(),
        format!("starts from {FALLBACK}."),
        format!("falls back to {DEFAULT_NAME} with a warning."),
    ] {
        assert!(text.contains(&want), "{want}");
    }
}

#[test]
fn every_built_in_theme_is_listed_once() {
    let page = themes_page();
    let listed: Vec<&str> = section(&page, "built-in themes:")
        .iter()
        .flat_map(|l| l.split_whitespace())
        .collect();
    let built_in = theme::names(Path::new(""), Path::new(""), true);
    assert_eq!(listed, built_in);
    assert!(built_in.iter().any(|n| n == FALLBACK));
}

#[test]
fn the_example_loads_without_a_warning_and_sets_what_it_says() {
    let text = EXAMPLE.join("\n") + "\n";
    let (theme, warnings) = load_file("mine", &text);
    assert_eq!(warnings, Vec::<String>::new());
    let orange = Some(Colour::Rgb(255, 136, 0));
    assert_eq!(theme.style(Slot::Project).fg, orange);
    let alert = theme.style(Slot::PriorityA);
    assert_eq!((alert.fg, alert.bold), (orange, true));
    let overdue = theme.style(Slot::DueOverdue);
    assert_eq!(
        (overdue.fg, overdue.underline),
        (Some(Colour::Rgb(255, 0, 0)), true)
    );
    assert_eq!(theme.style(Slot::Muted).fg, Some(Colour::Index(244)));
    assert_eq!(
        theme.style(Slot::Text).fg,
        Some(Colour::Rgb(0xd8, 0xde, 0xe9))
    );
    let (ansi, _) = theme::load(FALLBACK, Path::new(""), Path::new(""), true);
    assert_eq!(theme.style(Slot::Link), ansi.style(Slot::Link));
}

#[test]
fn lines_fit_the_guide_width() {
    for line in themes_page() {
        assert!(line.len() <= GUIDE_WIDTH, "{line}");
    }
}
