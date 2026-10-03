//! `lsty help --agent`: a compact guide for LLM agents. The matrix and
//! the exit codes are generated; the other parts are fixed text.

use crate::help::{exit_lines, retry_text};
use crate::matrix::Row;

/// The widest line of the guide, in bytes.
const GUIDE_WIDTH: usize = 80;

/// The parts of the guide before the matrix, in order.
const INTRO: [&[&str]; 9] = [
    &[
        "# Identity",
        "Set LSTY_OWNER, one name per worker. Do not use --human or",
        "--no-config.",
    ],
    &[
        "# The file",
        "Every result names file and found_by, the step that chose the file.",
        "On no_file, never retry with --create: report file and found_by to",
        "the human.",
    ],
    &[
        "# References",
        "Act by id: only. `lsty id NR --expect TEXT` gives an item an id.",
    ],
    &["# Output", "Use --json. today is the date lsty is using."],
    &[
        "# Getting work",
        "`next --start` claims the next ready item; a retry resumes the same",
        "claim. Read the item's files before starting. If next returns your",
        "claim blocked, work on the blocker or stop the claim.",
    ],
    &[
        "# Adding",
        "`add --if-absent --id=KEY TEXT` for an add that may be retried.",
    ],
    &[
        "# Dates",
        "Write new dates as relative input (+3d, fri). Retry with the ISO date",
        "from the result.",
    ],
    &[
        "# Claims",
        "                       nobody        yours         other's",
        "  start                claims        no-op         claimed",
        "  do                   wrong_state   done          claimed",
        "  stop, del, replace   allowed       allowed       claimed",
        "  other changes        allowed       allowed       warning",
        "Use --takeover only when told to.",
    ],
    &[
        "# Semantics",
        "A command is all-or-nothing. A no-op exits 0. A retried `del id:…` is",
        "no_item.",
    ],
];

/// The guide, one string per line.
pub(crate) fn agent_guide(rows: &[Row]) -> Vec<String> {
    let mut lines: Vec<String> = INTRO
        .iter()
        .flat_map(|part| part.iter().map(|l| (*l).to_owned()))
        .collect();
    lines.push("# The matrix".to_owned());
    lines.push("command: reference; what a retry does".to_owned());
    lines.extend(matrix_lines(rows));
    lines.push("# Exit codes".to_owned());
    lines.extend(
        exit_lines()
            .iter()
            .flat_map(|line| wrap("", "     ", line.trim(), GUIDE_WIDTH)),
    );
    lines
}

fn matrix_lines(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .flat_map(|row| {
            let text = format!(
                "{}: {}; {}",
                row.command,
                row.agent_ref,
                retry_text(&row.retry)
            );
            wrap("  ", "      ", &text, GUIDE_WIDTH)
        })
        .collect()
}

/// Breaks `text` into lines of at most `width` bytes at its spaces. The
/// first line starts with `first`, the others with `rest`; a word longer
/// than a line stands alone.
fn wrap(first: &str, rest: &str, text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = first.to_owned();
    for word in text.split_whitespace() {
        if line == first || line == rest {
            line.push_str(word);
        } else if line.len() + 1 + word.len() > width {
            lines.push(std::mem::replace(&mut line, format!("{rest}{word}")));
        } else {
            line.push(' ');
            line.push_str(word);
        }
    }
    lines.push(line);
    lines
}
