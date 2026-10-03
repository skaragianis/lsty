/// A colour: a place in the terminal's palette or a 24-bit value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    /// A number of the terminal's palette, `0` to `255`; `0` to `15` are
    /// the terminal's own 16 colours.
    Index(u8),
    /// Red, green and blue.
    Rgb(u8, u8, u8),
}

/// How a slot looks.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per attribute a theme file can set"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// The foreground colour.
    pub fg: Option<Colour>,
    /// The background colour.
    pub bg: Option<Colour>,
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underline: bool,
    /// Struck-through text.
    pub strike: bool,
    /// Faint text.
    pub faint: bool,
}

impl Style {
    /// A style that only sets the foreground.
    pub const fn fg(colour: Colour) -> Self {
        Self {
            fg: Some(colour),
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            faint: false,
        }
    }
}
