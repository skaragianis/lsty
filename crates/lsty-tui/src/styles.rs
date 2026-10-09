//! A theme as the styles of the terminal library.

use lsty_core::theme::{Colour, Profile, Slot, Style as ThemeStyle, Theme};
use ratatui::style::{Color, Modifier, Style};

/// The style of every slot, for one terminal.
#[derive(Clone, Debug)]
pub(crate) struct Styles {
    slots: [Style; Slot::ALL.len()],
    fills: [Style; Slot::ALL.len()],
}

fn colour(profile: Profile, c: Colour) -> Color {
    match profile.reduce(c) {
        Colour::Index(n) => Color::Indexed(n),
        Colour::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn modifiers(style: ThemeStyle) -> Modifier {
    [
        (style.bold, Modifier::BOLD),
        (style.italic, Modifier::ITALIC),
        (style.underline, Modifier::UNDERLINED),
        (style.strike, Modifier::CROSSED_OUT),
        (style.faint, Modifier::DIM),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(Modifier::empty(), |all, (_, one)| all | one)
}

fn paint(fg: Option<Color>, bg: Option<Color>, add: Modifier) -> Style {
    let mut style = Style::default().add_modifier(add);
    if let Some(c) = fg {
        style = style.fg(c);
    }
    if let Some(c) = bg {
        style = style.bg(c);
    }
    style
}

fn text_style(profile: Profile, style: ThemeStyle) -> Style {
    paint(
        style.fg.map(|c| colour(profile, c)),
        style.bg.map(|c| colour(profile, c)),
        modifiers(style),
    )
}

/// What a slot paints when it is used as a background: its own
/// background, or else its foreground.
fn fill_style(profile: Profile, style: ThemeStyle) -> Style {
    let bg = style.bg.or(style.fg).map(|c| colour(profile, c));
    paint(None, bg, Modifier::empty())
}

impl Styles {
    /// The styles of `theme` on a terminal that shows `profile`.
    pub(crate) fn new(theme: &Theme, profile: Profile) -> Self {
        let slots = Slot::ALL.map(|s| text_style(profile, theme.style(s)));
        let fills = Slot::ALL.map(|s| fill_style(profile, theme.style(s)));
        Self { slots, fills }
    }

    /// The style of text in `slot`; no slot is no style.
    pub(crate) fn get(&self, slot: Option<Slot>) -> Style {
        slot.map_or_else(Style::default, |s| self.slots[s.index()])
    }

    /// The style that fills a row with the colour of `slot`.
    pub(crate) fn fill(&self, slot: Slot) -> Style {
        self.fills[slot.index()]
    }
}
