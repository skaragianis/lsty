use super::{
    line::Line,
    parse::{self, BOM},
};
use crate::errs::Nr;

/// A parsed todo.txt file that keeps every byte of its input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct File {
    /// The lines, in order. The NR of a line is its index plus one. Only
    /// the last line may lack a line ending: a line before it would run
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
            out.extend_from_slice(line.eol.as_bytes());
        }
        out
    }

    /// The line with item number `nr`.
    pub(crate) fn line(&self, nr: Nr) -> Option<&Line> {
        self.lines.get(nr.index())
    }
}
