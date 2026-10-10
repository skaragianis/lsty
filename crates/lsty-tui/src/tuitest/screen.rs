//! The screen as plain text and as colour layers.

use std::collections::HashMap;

use ratatui::{
    buffer::{Buffer, Cell},
    style::Color,
};
use unicode_width::UnicodeWidthStr;

/// The mark of a cell without a colour from the legend.
const ABSENT: char = '.';

/// The screen as plain text: one line per row, trailing spaces removed.
pub(super) fn frame_lines(buf: &Buffer) -> Vec<String> {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            let mut line = String::new();
            let mut skip = 0;
            for x in 0..area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let symbol = buf[(x, y)].symbol();
                line.push_str(symbol);
                skip = symbol.width().saturating_sub(1);
            }
            line.trim_end_matches(' ').to_owned()
        })
        .collect()
}

/// A grid with the letter of the colour that `pick` takes of each cell.
pub(super) fn layer_lines(
    buf: &Buffer,
    letters: &HashMap<u8, char>,
    pick: fn(&Cell) -> Color,
) -> Vec<String> {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| letter_of(pick(&buf[(x, y)]), letters))
                .collect()
        })
        .collect()
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "only a palette number has a letter"
)]
fn letter_of(colour: Color, letters: &HashMap<u8, char>) -> char {
    match colour {
        Color::Indexed(n) => letters.get(&n).copied().unwrap_or(ABSENT),
        _ => ABSENT,
    }
}

/// The lines as text, each with its newline.
pub(super) fn join(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
