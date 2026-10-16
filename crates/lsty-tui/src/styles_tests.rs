use lsty_core::theme::Style as ThemeStyle;
use ratatui::style::Modifier;

use crate::styles::modifiers;

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
