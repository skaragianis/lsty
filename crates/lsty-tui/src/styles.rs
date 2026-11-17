//! A theme as the styles of the terminal library.

use lsty_core::theme::{Colour, Profile, Slot, Style as ThemeStyle, Theme};
use ratatui::style::{Color, Modifier, Style};

/// The style of every slot, for one terminal.
#[derive(Clone, Debug)]
pub(crate) struct Styles {
    slots: [Style; Slot::ALL.len()],
    fills: [Style; Slot::ALL.len()],
    /// The styles of the highlighted row.
    cursor: Option<Box<Self>>,
}

fn colour(profile: Profile, c: Colour) -> Color {
    match profile.reduce(c) {
        Colour::Index(n) => Color::Indexed(n),
        Colour::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub(crate) fn modifiers(style: ThemeStyle) -> Modifier {
    [
        (style.bold, Modifier::BOLD),
        (style.italic, Modifier::ITALIC),
        (style.underline, Modifier::UNDERLINED),
        (style.strike, Modifier::CROSSED_OUT),
        (style.faint, Modifier::DIM),
    ]
    .into_iter()
    .filter_map(|(on, one)| on.then_some(one))
    .collect()
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
/// background, or else its foreground, and its attributes.
fn fill_style(profile: Profile, style: ThemeStyle) -> Style {
    let bg = style.bg.or(style.fg).map(|c| colour(profile, c));
    paint(None, bg, modifiers(style))
}

impl Styles {
    /// The styles of `theme` on a terminal that shows `profile`.
    pub(crate) fn new(theme: &Theme, profile: Profile) -> Self {
        let on_row = Self::build(theme, profile, Theme::cursor_style);
        Self {
            cursor: Some(Box::new(on_row)),
            ..Self::build(theme, profile, Theme::style)
        }
    }

    fn build(
        theme: &Theme,
        profile: Profile,
        look: fn(&Theme, Slot) -> ThemeStyle,
    ) -> Self {
        let slots = Slot::ALL.map(|s| text_style(profile, look(theme, s)));
        let fills = Slot::ALL.map(|s| fill_style(profile, theme.style(s)));
        Self {
            slots,
            fills,
            cursor: None,
        }
    }

    /// The styles for a row: those of the highlighted row when `current`.
    pub(crate) fn row(&self, current: bool) -> &Self {
        self.cursor.as_deref().filter(|_| current).unwrap_or(self)
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
