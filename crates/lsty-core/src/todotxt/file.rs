use super::line::{Eol, Line};
use super::parse::{self, BOM};
use crate::errs::Nr;

/// A parsed todo.txt file that keeps every byte of its input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct File {
    /// The lines, in order. The NR of a line is its index plus one. Only
    /// the last line may have [`Eol::None`]: a line before it would run
    /// into the next when the file is written.
    pub lines: Vec<Line>,
    /// Whether the file starts with a UTF-8 byte order mark.
    pub bom: bool,
}

impl File {
    /// Splits `data` at `\n`, keeping a leading byte order mark and each
    /// line ending, so [`File::to_bytes`] returns `data` unchanged.
    pub fn parse(data: &[u8]) -> Self {
        parse::parse_file(data)
    }

    /// Writes the file back out: the byte order mark if there is one,
    /// then each line's bytes and ending.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.bom {
            out.extend_from_slice(&BOM);
        }
        for line in &self.lines {
            out.extend_from_slice(line.raw());
            out.extend_from_slice(line.eol().as_bytes());
        }
        out
    }

    /// Whether [`File::to_bytes`] of both files gives the same bytes,
    /// found without writing either out.
    pub fn same_bytes(&self, other: &Self) -> bool {
        let mut ours = self.pieces();
        let mut theirs = other.pieces();
        let mut here: &[u8] = &[];
        let mut there: &[u8] = &[];
        loop {
            here = next_piece(here, &mut ours);
            there = next_piece(there, &mut theirs);
            if here.is_empty() || there.is_empty() {
                return here.is_empty() && there.is_empty();
            }
            let len = here.len().min(there.len());
            let ((here_head, here_rest), (there_head, there_rest)) =
                (here.split_at(len), there.split_at(len));
            if here_head != there_head {
                return false;
            }
            (here, there) = (here_rest, there_rest);
        }
    }

    /// The pieces [`File::to_bytes`] joins, in order.
    fn pieces(&self) -> impl Iterator<Item = &[u8]> {
        let bom: Option<&[u8]> = self.bom.then_some(&BOM);
        bom.into_iter().chain(
            self.lines
                .iter()
                .flat_map(|line| [line.raw(), line.eol().as_bytes()]),
        )
    }

    /// The line with item number `nr`.
    pub fn line(&self, nr: Nr) -> Option<&Line> {
        self.lines.get(nr.index())
    }

    /// Adds `raw`, which holds no line break, as the last line. Every line
    /// ends as the file's first line does (`\n` in an empty file); when
    /// the old last line had no ending it gains one and the new line gets
    /// none, so the file still ends without a newline.
    pub fn append_line(&mut self, raw: impl AsRef<[u8]>) {
        let ending = match self.lines.first().map(Line::eol) {
            Some(Eol::CrLf) => Eol::CrLf,
            Some(Eol::Lf | Eol::None) | None => Eol::Lf,
        };
        let mut line = Line::parse(raw);
        line.set_eol(ending);
        if let Some(last) = self.lines.last_mut()
            && last.eol() == Eol::None
        {
            last.set_eol(ending);
            line.set_eol(Eol::None);
        }
        self.lines.push(line);
    }

    /// Empties the line with item number `nr` and keeps its ending, so no
    /// later line moves. Returns `false` when there is no such line.
    pub fn blank_line(&mut self, nr: Nr) -> bool {
        let Some(slot) = self.lines.get_mut(nr.index()) else {
            return false;
        };
        let mut line = Line::parse(Vec::new());
        line.set_eol(slot.eol());
        *slot = line;
        true
    }
}

/// `rest` when it holds bytes, else the next piece of `pieces` that does;
/// empty at the end.
fn next_piece<'a>(
    rest: &'a [u8],
    pieces: &mut impl Iterator<Item = &'a [u8]>,
) -> &'a [u8] {
    if rest.is_empty() {
        pieces.find(|p| !p.is_empty()).unwrap_or_default()
    } else {
        rest
    }
}
