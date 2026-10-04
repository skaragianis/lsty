//! The letters that stand for slots in the colour layers.

use std::collections::HashMap;

use lsty_core::theme::{Colour, Slot, Theme};

/// The slots of the layers, each with its letter.
const LEGEND: [(Slot, char); 29] = [
    (Slot::PriorityA, 'A'),
    (Slot::PriorityB, 'B'),
    (Slot::PriorityC, 'C'),
    (Slot::PriorityOther, 'P'),
    (Slot::Project, 'p'),
    (Slot::Context, 'c'),
    (Slot::Link, 'l'),
    (Slot::Text, 't'),
    (Slot::Muted, 'm'),
    (Slot::DueOverdue, 'O'),
    (Slot::DueToday, 'T'),
    (Slot::DueSoon, 'S'),
    (Slot::Done, 'd'),
    (Slot::Doing, 'g'),
    (Slot::Blocked, 'b'),
    (Slot::ModeNormal, 'N'),
    (Slot::ModeInsert, 'I'),
    (Slot::ModeVisual, 'V'),
    (Slot::ModeCommand, 'M'),
    (Slot::PanelBorder, 'e'),
    (Slot::PanelBorderActive, 'E'),
    (Slot::Selection, 's'),
    (Slot::Cursorline, 'u'),
    (Slot::Chip, 'h'),
    (Slot::StatusBg, 'G'),
    (Slot::HintKey, 'k'),
    (Slot::HintDesc, 'y'),
    (Slot::Warning, 'w'),
    (Slot::Error, 'x'),
];

/// Maps the palette numbers of a theme to the letters of the legend.
pub(super) fn letters(theme: &Theme) -> HashMap<u8, char> {
    LEGEND
        .iter()
        .filter_map(|&(slot, letter)| match theme.style(slot).fg {
            Some(Colour::Index(n)) => Some((n, letter)),
            Some(Colour::Rgb(..)) | None => None,
        })
        .collect()
}
