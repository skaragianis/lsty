//! `lsty help --agent`: a compact guide for LLM agents. The matrix and
//! the exit codes are generated; the other parts are fixed text.

use crate::{
    help::{GUIDE_WIDTH, exit_lines, retry_text, wrap},
    matrix::Row,
};

/// The parts of the guide before the matrix, in order.
const INTRO: [&[&str]; 10] = [
    &[
        "# Identity",
        "Set LSTY_OWNER at the start of a run and use it for every call:",
        "agent-<first 4 characters of your session id>, or another id unique",
        "to this run. Never a bare model or product name. Do not use --human",
        "or --no-config.",
    ],
    &[
        "# The file",
        "On a no_file error, never retry with --create: report its file and",
        "found_by (the step that chose the file) to the human.",
    ],
    &[
        "# References",
        "Act by id: only. `lsty id NR --expect TEXT` gives an item an id.",
    ],
    &[
        "# Output",
        "Use --json. today is the date lsty is using. version changes when",
        "a field is removed, renamed or changed in type or meaning; a new",
        "field does not change it.",
    ],
    &[
        "# Getting work",
        "`next --start` claims the next ready item; a retry resumes the same",
        "claim. Read the item's files before starting: a relative file: path",
        "starts at the folder of the todo file (file:../docs/notes.md), ~ is",
        "the home folder, and files[].path is the full path. files[] holds",
        "URLs too: kind is file or url, and a url entry has url, no path. If",
        "next returns your claim blocked, work on the blocker or stop the",
        "claim. If next resumes an id you did not claim this run, another",
        "worker has your name: leave that item alone, switch to a new name",
        "and tell the human.",
        "If next returns no item and blocked_count, later_count or",
        "claimed_count is above 0, the queue is not finished: items wait on a",
        "prerequisite, a start date or another worker.",
        "`ID=$(lsty next --start --print-id)` prints only the id; add takes",
        "--print-id too.",
    ],
    &[
        "# Queries",
        "--filter and the terms of ls take: words, +project, @context, key:value,",
        "is:STATE (`lsty help` lists them). All terms must match. a|b matches",
        "either; -TERM leaves out the whole term, so write --filter '-TERM'.",
        "not, or and and are plain words, not operators: `+a not @b or @c`",
        "finds nothing. Write --filter '+a -@b' or --filter '+a @b|@c'.",
    ],
    &[
        "# Adding",
        "`add --if-absent --id=KEY TEXT` for an add that may be retried.",
        "Text between backticks is literal: no project, context, tag or file link",
        "is read from it.",
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
