//! How many colours a terminal shows, and the nearest colour it has.

use super::style::Colour;
use crate::case::lower;

/// How many colours a terminal shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Profile {
    /// The terminal's 16 colours.
    Ansi,
    /// The 256-colour palette.
    Ansi256,
    /// 24-bit colour.
    TrueColor,
}

/// Terminals that show 24-bit colour, by a `-` separated word of their
/// `TERM` (`xterm-kitty`, `st-256color`, not `nsterm`).
const TRUE_COLOR_TERMS: [&str; 8] = [
    "alacritty",
    "contour",
    "foot",
    "ghostty",
    "kitty",
    "rio",
    "st",
    "wezterm",
];

impl Profile {
    /// The profile that `TERM` and `COLORTERM` describe. A terminal that
    /// says nothing about colour counts as showing all of it, because the
    /// caller already decided that colour is wanted.
    pub fn detect(term: &str, colorterm: &str) -> Self {
        if term == "dumb" {
            return Self::TrueColor;
        }
        let multiplexer =
            term.starts_with("tmux") || term.starts_with("screen");
        if term.split('-').any(|word| TRUE_COLOR_TERMS.contains(&word)) {
            return Self::TrueColor;
        }
        let lowered = lower(colorterm);
        let says_true =
            matches!(lowered.as_str(), "truecolor" | "24bit" | "yes" | "true");
        if (says_true && !multiplexer) || term.ends_with("direct") {
            return Self::TrueColor;
        }
        if term.is_empty() {
            return Self::TrueColor;
        }
        if multiplexer || term.ends_with("256color") {
            return Self::Ansi256;
        }
        Self::Ansi
    }

    /// The colour this profile shows in place of `colour`.
    pub fn reduce(self, colour: Colour) -> Colour {
        match (self, colour) {
            (Self::TrueColor, _)
            | (_, Colour::Index(0..=15))
            | (Self::Ansi256, Colour::Index(_)) => colour,
            (Self::Ansi256, Colour::Rgb(red, green, blue)) => {
                Colour::Index(nearest_256(red, green, blue))
            }
            (Self::Ansi, Colour::Index(n)) => Colour::Index(to_16(n)),
            (Self::Ansi, Colour::Rgb(red, green, blue)) => {
                Colour::Index(to_16(nearest_256(red, green, blue)))
            }
        }
    }
}

fn to_16(index: u8) -> u8 {
    TO_16.get(usize::from(index)).copied().unwrap_or_default()
}

/// The 16-colour palette entry that stands for each 256-colour entry.
const TO_16: [u8; 256] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 4, 4, 4, 12, 12,
    2, 6, 4, 4, 12, 12, 2, 2, 6, 4, 12, 12, 2, 2, 2, 6, 12, 12, 10, 10, 10,
    10, 14, 12, 10, 10, 10, 10, 10, 14, 1, 5, 4, 4, 12, 12, 3, 8, 4, 4, 12,
    12, 2, 2, 6, 4, 12, 12, 2, 2, 2, 6, 12, 12, 10, 10, 10, 10, 14, 12, 10,
    10, 10, 10, 10, 14, 1, 1, 5, 4, 12, 12, 1, 1, 5, 4, 12, 12, 3, 3, 8, 4,
    12, 12, 2, 2, 2, 6, 12, 12, 10, 10, 10, 10, 14, 12, 10, 10, 10, 10, 10,
    14, 1, 1, 1, 5, 12, 12, 1, 1, 1, 5, 12, 12, 1, 1, 1, 5, 12, 12, 3, 3, 3,
    7, 12, 12, 10, 10, 10, 10, 14, 12, 10, 10, 10, 10, 10, 14, 9, 9, 9, 9, 13,
    12, 9, 9, 9, 9, 13, 12, 9, 9, 9, 9, 13, 12, 9, 9, 9, 9, 13, 12, 11, 11,
    11, 11, 7, 12, 10, 10, 10, 10, 10, 14, 9, 9, 9, 9, 9, 13, 9, 9, 9, 9, 9,
    13, 9, 9, 9, 9, 9, 13, 9, 9, 9, 9, 9, 13, 9, 9, 9, 9, 9, 13, 11, 11, 11,
    11, 11, 15, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 8, 8, 7, 7, 7, 7, 7, 7, 15, 15,
    15, 15, 15, 15,
];

/// The value of level `q` (`0..=5`) of a channel in the 6x6x6 colour cube:
/// `0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff`.
const fn cube_value(q: u8) -> u8 {
    if q == 0 { 0 } else { 55 + 40 * q }
}

/// A colour with each channel in `0.0..=1.0`.
#[derive(Clone, Copy)]
struct Channels {
    red: f64,
    green: f64,
    blue: f64,
}

impl Channels {
    fn of(red: u8, green: u8, blue: u8) -> Self {
        let scale = |v: u8| f64::from(u16::from(v) * 257) / 65535.0;
        Self {
            red: scale(red),
            green: scale(green),
            blue: scale(blue),
        }
    }

    fn grey(level: u8) -> Self {
        let value = f64::from(level) / 255.0;
        Self {
            red: value,
            green: value,
            blue: value,
        }
    }

    fn cube(red: u8, green: u8, blue: u8) -> Self {
        let level = |q: u8| f64::from(cube_value(q)) / 255.0;
        Self {
            red: level(red),
            green: level(green),
            blue: level(blue),
        }
    }
}

/// Which of the 6 levels of the colour cube a channel (`0.0..=255.0`)
/// falls in.
fn cube_level(value: f64) -> u8 {
    [48.0, 115.0, 155.0, 195.0, 235.0]
        .iter()
        .map(|&edge| u8::from(value >= edge))
        .sum()
}

/// `value` with its fraction dropped; values here are `0.0..=255.0`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is between 0 and 255"
)]
const fn truncate(value: f64) -> u8 {
    value as u8
}

/// The 256-colour palette entry nearest to a 24-bit colour: the closer of
/// the nearest entry of the colour cube and the nearest grey, by distance
/// in `HSLuv`.
fn nearest_256(red: u8, green: u8, blue: u8) -> u8 {
    let colour = Channels::of(red, green, blue);
    let (scaled_red, scaled_green, scaled_blue) = (
        colour.red * 255.0,
        colour.green * 255.0,
        colour.blue * 255.0,
    );
    let (qr, qg, qb) = (
        cube_level(scaled_red),
        cube_level(scaled_green),
        cube_level(scaled_blue),
    );
    let cube_index = 16 + 36 * qr + 6 * qg + qb;
    if cube_value(qr) == truncate(scaled_red)
        && cube_value(qg) == truncate(scaled_green)
        && cube_value(qb) == truncate(scaled_blue)
    {
        return cube_index;
    }
    let average =
        truncate((scaled_red + scaled_green + scaled_blue).trunc() / 3.0);
    let grey_step = if average > 238 {
        23
    } else {
        average.saturating_sub(3) / 10
    };
    let grey_level = 8 + 10 * grey_step;
    let to_cube = hsluv_distance(colour, Channels::cube(qr, qg, qb));
    let to_grey = hsluv_distance(colour, Channels::grey(grey_level));
    if to_cube <= to_grey {
        cube_index
    } else {
        232 + grey_step
    }
}

/// The white of the `HSLuv` colour space.
const WHITE: [f64; 3] = [0.950_455_927_051_67, 1.0, 1.089_057_750_759_878];

/// Hue in degrees, saturation and lightness in `0.0..=1.0`.
struct Hsluv {
    hue: f64,
    saturation: f64,
    lightness: f64,
}

/// The Euclidean distance between two colours in `HSLuv`, with the hue
/// divided by 100.
fn hsluv_distance(first: Channels, second: Channels) -> f64 {
    let (one, two) = (hsluv(first), hsluv(second));
    let hue = (one.hue - two.hue) / 100.0;
    let saturation = one.saturation - two.saturation;
    let lightness = one.lightness - two.lightness;
    (hue * hue + saturation * saturation + lightness * lightness).sqrt()
}

fn linearize(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The chromaticity of [`WHITE`].
const WHITE_UV: (f64, f64) = chromaticity(WHITE[0], WHITE[1], WHITE[2]);

/// The CIE Luv coordinates of an sRGB colour, against [`WHITE`].
fn luv(colour: Channels) -> (f64, f64, f64) {
    let (lin_red, lin_green, lin_blue) = (
        linearize(colour.red),
        linearize(colour.green),
        linearize(colour.blue),
    );
    let x = 0.412_390_799_265_959_5 * lin_red
        + 0.357_584_339_383_877_96 * lin_green
        + 0.180_480_788_401_834_3 * lin_blue;
    let y = 0.212_639_005_871_510_36 * lin_red
        + 0.715_168_678_767_755_9 * lin_green
        + 0.072_192_315_360_733_71 * lin_blue;
    let z = 0.019_330_818_715_591_85 * lin_red
        + 0.119_194_779_794_625_99 * lin_green
        + 0.950_532_152_249_660_6 * lin_blue;
    let lightness = if y <= 216.0 / 24389.0 {
        y * (24389.0 / 27.0) / 100.0
    } else {
        1.16 * y.cbrt() - 0.16
    };
    let (u_prime, v_prime) = chromaticity(x, y, z);
    (
        lightness,
        13.0 * lightness * (u_prime - WHITE_UV.0),
        13.0 * lightness * (v_prime - WHITE_UV.1),
    )
}

const fn chromaticity(x: f64, y: f64, z: f64) -> (f64, f64) {
    let denominator = x + 15.0 * y + 3.0 * z;
    if denominator == 0.0 {
        (0.0, 0.0)
    } else {
        (4.0 * x / denominator, 9.0 * y / denominator)
    }
}

fn hsluv(colour: Channels) -> Hsluv {
    let (lightness, u, v) = luv(colour);
    let hue = if (v - u).abs() > 1e-4 && u.abs() > 1e-4 {
        (57.295_779_513_082_32 * v.atan2(u) + 360.0) % 360.0
    } else {
        0.0
    };
    let chroma = (u * u + v * v).sqrt() * 100.0;
    let percent = lightness * 100.0;
    let saturation = if (0.000_000_01..=99.999_999_9).contains(&percent) {
        chroma / max_chroma(percent, hue) * 100.0
    } else {
        0.0
    };
    Hsluv {
        hue,
        saturation: (saturation / 100.0).clamp(0.0, 1.0),
        lightness: (percent / 100.0).clamp(0.0, 1.0),
    }
}

/// The matrix that turns CIE XYZ into linear sRGB.
const XYZ_TO_RGB: [[f64; 3]; 3] = [
    [
        3.240_969_941_904_521_3,
        -1.537_383_177_570_093_5,
        -0.498_610_760_293_003_3,
    ],
    [
        -0.969_243_636_280_879_8,
        1.875_967_501_507_720_6,
        0.041_555_057_407_175_61,
    ],
    [
        0.055_630_079_696_993_61,
        -0.203_976_958_888_976_57,
        1.056_971_514_242_878_6,
    ],
];

/// The greatest chroma that stays inside sRGB at a lightness and hue.
fn max_chroma(lightness: f64, hue: f64) -> f64 {
    let angle = hue / 360.0 * std::f64::consts::PI * 2.0;
    let mut shortest = f64::MAX;
    for (slope, intercept) in bounds(lightness) {
        let length = intercept / (angle.sin() - slope * angle.cos());
        if length > 0.0 && length < shortest {
            shortest = length;
        }
    }
    shortest
}

/// The six lines (slope and intercept) that bound sRGB at a lightness.
fn bounds(lightness: f64) -> impl Iterator<Item = (f64, f64)> {
    const KAPPA: f64 = 903.296_296_296_296_3;
    const EPSILON: f64 = 0.008_856_451_679_035_631;
    let cubed = (lightness + 16.0).powf(3.0) / 1_560_896.0;
    let sub2 = if cubed > EPSILON {
        cubed
    } else {
        lightness / KAPPA
    };
    XYZ_TO_RGB.iter().flat_map(move |row| {
        [0.0, 1.0].map(move |k| {
            let top1 = (284_517.0 * row[0] - 94_839.0 * row[2]) * sub2;
            let top2 =
                (838_422.0 * row[2] + 769_860.0 * row[1] + 731_718.0 * row[0])
                    * lightness
                    * sub2
                    - 769_860.0 * k * lightness;
            let bottom = (632_260.0 * row[2] - 126_452.0 * row[1]) * sub2
                + 126_452.0 * k;
            (top1 / bottom, top2 / bottom)
        })
    })
}

#[cfg(test)]
mod table;
#[cfg(test)]
mod tests;
