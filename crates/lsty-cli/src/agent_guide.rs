//! `lsty help --agent`: a compact guide for LLM agents. The retries and
//! the exit codes are generated; the other parts are fixed text.

use crate::{
    help::{GUIDE_WIDTH, exit_lines, retry_text, wrap},
    matrix::Row,
};

/// The fixed parts of the guide, in order; the retries follow them.
const INTRO: [&[&str]; 9] = [
    &[
        "# Identity",
        "Set LSTY_OWNER once per run: agent-<first 4 session-id characters> or",
        "another run-unique id. Use it for every call; never a bare model or",
        "product name. Never use --human or --no-config. A restart is a new run.",
    ],
    &[
        "# Calls",
        "Change the todo and done files only through lsty. Use --json and only id:",
        "references; `lsty id NR --expect TEXT` assigns an id. Commands are",
        "all-or-nothing; no-ops exit 0. today is lsty's date. version changes only",
        "on field removal, renaming, or a change of type or meaning.",
        "On no_file, never retry with --create: report file and found_by (the",
        "step that chose the file) to the human.",
    ],
    &[
        "# Getting work",
        "`next --start` claims the next ready item; a retry resumes the same claim.",
        "Read its files before working. If next returns",
        "  your blocked claim: work on its blocker or stop the claim.",
        "  a resumed claim you cannot have made this run: a name collision. Leave",
        "    it untouched, choose a new LSTY_OWNER and tell the human.",
        "  no item, but blocked_count, later_count or claimed_count above 0: the",
        "    queue is not finished; items await a prerequisite, a start date or",
        "    another worker.",
        "Without --json, --print-id makes next --start and add print only the id.",
    ],
    &[
        "# Files",
        "files[] holds links. kind file: raw (as written), path (full path), exists.",
        "kind url: only url (whole address). Relative file: links start at the",
        "todo file's folder (file:../notes/plan.md); ~ is home.",
    ],
    &[
        "# Queries",
        "ls terms and --filter accept words, +project, @context, key:value and",
        "is:STATE (see `lsty help`). All terms must match; a|b matches either;",
        "-TERM excludes what TERM matches (write --filter '-TERM'). not, or and",
        "and are literal words, not operators: write '+a -@b' or '+a @b|@c'.",
    ],
    &[
        "# Adding",
        "Retryable add: `add --if-absent --id=KEY TEXT`. Text between backticks",
        "is literal: no project, context, tag or link is read from it.",
    ],
    &[
        "# Dates",
        "Write new dates as relative input (+3d, fri). It may resolve differently",
        "on another day: retry with the result's ISO date or, if the result was",
        "lost, read the item first.",
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
        "# Retries",
        "commands: reference; retry (repeats: done again; detected: refused, exit 3)",
    ],
];

/// The guide, one string per line.
pub(crate) fn agent_guide(rows: &[Row]) -> Vec<String> {
    let mut lines: Vec<String> = INTRO
        .iter()
        .flat_map(|part| part.iter().map(|l| (*l).to_owned()))
        .collect();
    lines.extend(retry_lines(rows));
    lines.push("# Exit codes".to_owned());
    lines.extend(
        exit_lines()
            .iter()
            .flat_map(|line| wrap("", "     ", line.trim(), GUIDE_WIDTH)),
    );
    lines
}

/// One line for the commands that share a reference and a retry.
fn retry_lines(rows: &[Row]) -> Vec<String> {
    let mut groups: Vec<(String, String)> = Vec::new();
    for row in rows {
        let rule = format!("{}; {}", row.agent_ref, retry_text(&row.retry));
        match groups.iter_mut().find(|group| group.1 == rule) {
            Some(group) => group.0 = format!("{}, {}", group.0, row.command),
            None => groups.push((row.command.to_owned(), rule)),
        }
    }
    groups
        .iter()
        .flat_map(|(commands, rule)| {
            wrap("  ", "      ", &format!("{commands}: {rule}"), GUIDE_WIDTH)
        })
        .collect()
}
