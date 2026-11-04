use std::path::Path;

use lsty_core::theme::{self, Profile, Slot, Style as ThemeStyle};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

use crate::{
    model::Model,
    msg::Msg,
    styles::{Styles, modifiers},
    test_util::{alone, deps, keys, started, workdir},
};

const PROFILES: [Profile; 3] =
    [Profile::Ansi, Profile::Ansi256, Profile::TrueColor];

/// The styles of every built-in theme at `profile`.
fn built_in(profile: Profile) -> Vec<(String, Styles)> {
    let none = Path::new("");
    theme::names(none, none, true)
        .into_iter()
        .map(|name| {
            let (theme, _) = theme::load(&name, none, none, true, profile);
            let styles = Styles::new(&theme, profile);
            (name, styles)
        })
        .collect()
}

#[test]
fn each_attribute_sets_its_own_modifier() {
    let with = |set: fn(&mut ThemeStyle)| {
        let mut style = ThemeStyle::default();
        set(&mut style);
        style
    };
    let rows = [
        (with(|s| s.bold = true), Modifier::BOLD),
        (with(|s| s.italic = true), Modifier::ITALIC),
        (with(|s| s.underline = true), Modifier::UNDERLINED),
        (with(|s| s.strike = true), Modifier::CROSSED_OUT),
        (with(|s| s.faint = true), Modifier::DIM),
        (with(|_| ()), Modifier::empty()),
    ];
    for (style, want) in rows {
        assert_eq!(modifiers(style), want, "{style:?}");
    }
}

/// The row under the cursor and a selected row look different from each
/// other and from a plain row, by a fill the terminal gets.
#[test]
fn the_cursor_row_and_a_selected_row_are_drawn_in_every_profile() {
    for profile in PROFILES {
        for (name, styles) in built_in(profile) {
            let (cursor, chosen) =
                (styles.fill(Slot::Cursorline), styles.fill(Slot::Selection));
            let plain = Style::default();
            let tag = format!("{name} {profile:?}");
            assert_ne!(cursor, plain, "{tag}");
            assert_ne!(chosen, plain, "{tag}");
            assert_ne!(cursor, chosen, "{tag}");
        }
    }
}

/// On screen at 16 colours: the row under the cursor is bold, a selected
/// one underlined, the others neither, and no row or line has a background.
#[test]
fn the_rows_at_16_colours_are_marked_without_a_background() {
    let dir = workdir("A\nB\nC\n");
    let mut with = deps(dir.path(), alone());
    with.profile = Profile::Ansi;
    let mut model = started(with);
    model.update(Msg::Resize(40, 8));
    model.show_theme("tokyo-night-storm");
    let area = Rect::new(0, 0, 40, 8);
    let row = |model: &Model, y: u16| {
        let mut buf = Buffer::empty(area);
        model.draw(&mut buf);
        let cell = &buf[(10, y)];
        assert_eq!(cell.bg, Color::Reset, "row {y}");
        cell.modifier
    };
    assert!(row(&model, 1).contains(Modifier::BOLD));
    assert_eq!(row(&model, 2), Modifier::empty());
    keys(&mut model, "Vj");
    assert!(row(&model, 1).contains(Modifier::UNDERLINED));
    assert!(row(&model, 2).contains(Modifier::UNDERLINED));
    assert_eq!(row(&model, 3), Modifier::empty());
}
