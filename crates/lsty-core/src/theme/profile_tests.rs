use super::{Colour, Profile};

/// `TERM`, `COLORTERM`, and the profile expected.
const DETECT: [(&str, &str, Profile); 14] = [
    ("", "", Profile::TrueColor),
    ("dumb", "", Profile::TrueColor),
    ("xterm", "", Profile::Ansi),
    ("xterm-256color", "", Profile::Ansi256),
    ("xterm-256color", "truecolor", Profile::TrueColor),
    ("xterm", "24bit", Profile::TrueColor),
    ("screen-256color", "truecolor", Profile::Ansi256),
    ("screen", "", Profile::Ansi256),
    ("tmux-256color", "", Profile::Ansi256),
    ("linux", "", Profile::Ansi),
    ("xterm-kitty", "", Profile::TrueColor),
    ("alacritty", "", Profile::TrueColor),
    ("xterm-direct", "", Profile::TrueColor),
    ("vt100", "YES", Profile::TrueColor),
];

#[test]
fn the_profile_follows_term_and_colorterm() {
    for (term, colorterm, want) in DETECT {
        assert_eq!(
            Profile::detect(term, colorterm),
            want,
            "{term:?} {colorterm:?}"
        );
    }
}

/// An RGB colour, then its place in the 256-colour palette and in the
/// 16 colours.
type ReducedRow = ((u8, u8, u8), u8, u8);

const REDUCED: [ReducedRow; 12] = [
    ((255, 0, 0), 196, 9),
    ((0, 0, 0), 16, 0),
    ((255, 255, 255), 231, 15),
    ((0x80, 0x80, 0x80), 244, 7),
    ((0xcb, 0xa6, 0xf7), 183, 12),
    ((0x1e, 0x1e, 0x2e), 235, 0),
    ((0xf3, 0x8b, 0xa8), 211, 9),
    ((0xa6, 0xe3, 0xa1), 151, 10),
    ((0x10, 0x20, 0x30), 17, 4),
    ((0x5f, 0x87, 0xaf), 67, 4),
    ((0x08, 0x08, 0x08), 232, 0),
    ((0xee, 0xee, 0xee), 255, 15),
];

#[test]
fn colours_are_reduced_to_the_nearest_the_terminal_has() {
    for ((red, green, blue), in256, in16) in REDUCED {
        let colour = Colour::Rgb(red, green, blue);
        assert_eq!(
            Profile::Ansi256.reduce(colour),
            Colour::Index(in256),
            "{red} {green} {blue} in 256"
        );
        assert_eq!(
            Profile::Ansi.reduce(colour),
            Colour::Index(in16),
            "{red} {green} {blue} in 16"
        );
        assert_eq!(Profile::TrueColor.reduce(colour), colour);
    }
}

#[test]
fn palette_numbers_are_kept_or_mapped_to_the_16_colours() {
    assert_eq!(
        Profile::Ansi256.reduce(Colour::Index(200)),
        Colour::Index(200)
    );
    assert_eq!(Profile::Ansi.reduce(Colour::Index(200)), Colour::Index(9));
    assert_eq!(Profile::Ansi.reduce(Colour::Index(28)), Colour::Index(2));
    assert_eq!(Profile::Ansi.reduce(Colour::Index(7)), Colour::Index(7));
    assert_eq!(
        Profile::TrueColor.reduce(Colour::Index(200)),
        Colour::Index(200)
    );
}
