//! A field that wraps softly: where each character falls on the rows,
//! the rows as drawn, and moves between the rows.

use lsty_core::theme::Slot;
use unicode_width::UnicodeWidthChar;

use crate::field::Field;
use crate::text::Span;

/// Where a character, or the cursor before it, is on the wrapped rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

/// How a field falls on rows `w` cells wide: the cell of every
/// position, the end of the text included, and the number of rows.
#[derive(Clone, Debug)]
pub(crate) struct Layout {
    at: Vec<Cell>,
    pub(crate) rows: usize,
}

impl Layout {
    /// The cell of the cursor at position `p`.
    pub(crate) fn cell(&self, p: usize) -> Cell {
        self.at.get(p).copied().unwrap_or_default()
    }
}

/// The cells a character takes as drawn: control characters are drawn
/// as one-cell pictures.
fn cells_of(c: char) -> usize {
    if c.is_control() {
        1
    } else {
        c.width().unwrap_or(0)
    }
}

impl Field {
    /// Lays the text out on rows `w` cells wide.
    pub(crate) fn layout(&self, w: usize) -> Layout {
        let mut at = Vec::with_capacity(self.text.len() + 1);
        let (mut row, mut col) = (0, 0);
        for &c in &self.text {
            let cells = cells_of(c);
            if col > 0 && col + cells > w {
                row += 1;
                col = 0;
            }
            at.push(Cell { row, col });
            col += cells;
        }
        if col >= w {
            row += 1;
            col = 0;
        }
        at.push(Cell { row, col });
        Layout { at, rows: row + 1 }
    }

    /// The rows as drawn, the read-only characters muted.
    pub(crate) fn lines(&self, w: usize) -> Vec<Vec<Span<'static>>> {
        let layout = self.layout(w);
        let mut out: Vec<Vec<(String, Slot)>> = vec![Vec::new(); layout.rows];
        for (i, &c) in self.text.iter().enumerate() {
            let slot = if self.is_locked(i) {
                Slot::Muted
            } else {
                Slot::Text
            };
            let Some(row) = out.get_mut(layout.cell(i).row) else {
                continue;
            };
            match row.last_mut() {
                Some((text, last)) if *last == slot => text.push(c),
                Some(_) | None => row.push((c.to_string(), slot)),
            }
        }
        out.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(text, slot)| Span::new(text, Some(slot)))
                    .collect()
            })
            .collect()
    }

    /// Moves the cursor to the row above or below, aiming at the column
    /// it had when the vertical moves began. Rows where the cursor
    /// cannot rest are passed over; on the first or last row nothing
    /// happens.
    pub(crate) fn vertical(&mut self, w: usize, up: bool) {
        let layout = self.layout(w);
        let from = layout.cell(self.cursor);
        let col = *self.col.get_or_insert(from.col);
        let rows: Vec<usize> = if up {
            (0..from.row).rev().collect()
        } else {
            (from.row + 1..layout.rows).collect()
        };
        let found = rows
            .into_iter()
            .find_map(|row| self.nearest(&layout, row, col));
        if let Some(best) = found {
            self.cursor = best;
        }
    }

    /// The position on `row` closest to the wanted column where the
    /// cursor can rest.
    fn nearest(
        &self,
        layout: &Layout,
        row: usize,
        col: usize,
    ) -> Option<usize> {
        (0..=self.text.len())
            .filter(|&p| layout.cell(p).row == row && self.valid(p))
            .min_by_key(|&p| layout.cell(p).col.abs_diff(col))
    }
}
