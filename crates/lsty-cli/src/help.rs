//! `lsty help`: the text generated from the command table, the options
//! and the safety matrix, and how it reaches the reader.

use lsty_core::errs::{self, Code, Result};
use lsty_core::links;

use crate::commands::{Bare, COMMANDS, Command};
use crate::dispatch::{USAGE_TEXT, unknown_command};
use crate::env::Env;
use crate::matrix::{MATRIX, Retry, RetryKind, Row};
use crate::output::{Output, write_lines};
use crate::parse::{option_names, usage};
use crate::tty::Terminals;

/// An option as the help describes it: the name, how it is written and
/// what it does.
pub(crate) type OptionDoc = (&'static str, &'static str, &'static str);

/// Every option, so that no option goes without a description.
pub(crate) const OPTION_DOCS: [OptionDoc; 28] = [
    ("file", "--file PATH", "use this todo file"),
    ("color", "--color MODE", "auto, always or never"),
    (
        "expect",
        "--expect TEXT",
        "refuse unless the item holds TEXT",
    ),
    ("owner", "--owner NAME", "act as this agent"),
    ("filter", "--filter QUERY", "keep the items that match"),
    ("sort", "--sort FIELD", "order the items by this field"),
    ("after", "--after REF", "place after this item"),
    ("before", "--before REF", "place before this item"),
    ("pri", "--pri LETTER", "give the new item this priority"),
    (
        "id",
        "--id[=ID]",
        "give the item an id, or name it for a retry",
    ),
    ("json", "--json", "print the result as JSON"),
    ("raw", "--raw", "print the lines as they are in the file"),
    ("no-config", "--no-config", "ignore config.toml"),
    (
        "if-absent",
        "--if-absent",
        "add only when no open item has the text",
    ),
    (
        "ignore-blocked",
        "--ignore-blocked",
        "go on although the item is blocked",
    ),
    (
        "takeover",
        "--takeover",
        "act on an item that another actor holds",
    ),
    ("human", "--human", "act as the human"),
    ("start", "--start", "claim the item"),
    (
        "create",
        "--create",
        "create the todo file when it is missing",
    ),
    ("top", "--top", "place at the top of the list"),
    ("later", "--later", "include items that start later"),
    (
        "archive",
        "--archive",
        "move the done item to the done file",
    ),
    (
        "no-archive",
        "--no-archive",
        "leave the done item in the todo file",
    ),
    ("date", "--date", "stamp the creation date"),
    ("no-date", "--no-date", "do not stamp the creation date"),
    ("agent", "--agent", "print the guide for LLM agents"),
    ("help", "--help, -h", "print this help"),
    ("version", "--version, -V", "print the version"),
];

/// What each exit status means.
pub(crate) const EXIT_MEANINGS: [(u8, &str); 5] = [
    (0, "done"),
    (errs::EXIT_IO, "the system failed"),
    (errs::EXIT_USAGE, "the call is wrong"),
    (errs::EXIT_CONFLICT, "the list disagrees with the call"),
    (errs::EXIT_LOCK_TIMEOUT, "the list stayed locked"),
];

/// The environment variables lsty reads, and what each is for.
const ENVIRONMENT: [(&str, &str); 12] = [
    ("TODO_FILE, TODO_DIR", "the todo file, or its folder"),
    ("DONE_FILE", "the done file"),
    ("LSTY_CONFIG", "the path of config.toml"),
    ("LSTY_OWNER", "who is acting: one name per agent"),
    ("LSTY_TODAY", "override today (YYYY-MM-DD)"),
    ("LSTY_NOW", "override the time (YYYY-MM-DDTHHMMZ)"),
    (
        "LSTY_NONINTERACTIVE",
        "1 behaves as if there were no terminal",
    ),
    ("TZ", "the zone of today and of displayed times"),
    (
        "XDG_CONFIG_HOME, XDG_STATE_HOME",
        "config and state folders",
    ),
    ("EDITOR", "the editor of edit and of text file: links"),
    ("PAGER", "the pager of help on a terminal"),
    (
        "NO_COLOR, TERM=dumb",
        "no colour; dumb also turns off the TUI",
    ),
];

/// The widest synopsis that sets the column of the commands; a longer
/// one has its sentence on the next line.
const SYNOPSIS_WIDTH: usize = 24;

/// The text of `lsty help`.
pub(crate) fn overview(table: &[Command]) -> Vec<String> {
    let mut lines: Vec<String> =
        USAGE_TEXT.lines().map(str::to_owned).collect();
    lines.extend([String::new(), "commands:".to_owned()]);
    lines.extend(command_lines(table));
    lines.extend([String::new(), "options:".to_owned()]);
    lines.extend(option_lines(&option_names()));
    lines.extend([String::new(), "exit codes:".to_owned()]);
    lines.extend(exit_lines());
    lines.extend([String::new(), "environment:".to_owned()]);
    lines.extend(environment_lines());
    lines
}

fn command_lines(table: &[Command]) -> Vec<String> {
    let width = table
        .iter()
        .map(|c| c.synopsis.chars().count())
        .filter(|n| *n <= SYNOPSIS_WIDTH)
        .max()
        .unwrap_or(0);
    table.iter().flat_map(|c| command_entry(c, width)).collect()
}

fn command_entry(c: &Command, width: usize) -> Vec<String> {
    let mut about = c.about.to_owned();
    if !c.aliases.is_empty() {
        about = format!("{about} [also: {}]", c.aliases.join(", "));
    }
    if c.synopsis.chars().count() > width {
        return vec![
            format!("  {}", c.synopsis),
            format!("  {:width$}  {about}", ""),
        ];
    }
    vec![format!("  {:width$}  {about}", c.synopsis)]
}

/// How an option is written, and what it does.
fn doc_of(name: &str) -> (&'static str, &'static str) {
    OPTION_DOCS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map_or(("", ""), |(_, usage, about)| (*usage, *about))
}

fn option_lines(names: &[&str]) -> Vec<String> {
    let docs: Vec<_> = names.iter().map(|n| doc_of(n)).collect();
    let width = docs.iter().map(|d| d.0.len()).max().unwrap_or(0);
    docs.iter()
        .map(|(usage, about)| format!("  {usage:width$}  {about}"))
        .collect()
}

/// The lines of the exit codes, each with the error codes that end in it.
pub(crate) fn exit_lines() -> Vec<String> {
    EXIT_MEANINGS
        .iter()
        .map(|(status, meaning)| {
            let names: Vec<&str> = Code::ALL
                .iter()
                .filter(|code| code.exit() == *status)
                .map(|code| code.as_str())
                .collect();
            let mut line = format!("  {status}  {meaning}");
            if !names.is_empty() {
                line = format!("{line}: {}", names.join(", "));
            }
            line
        })
        .collect()
}

fn environment_lines() -> Vec<String> {
    let width = ENVIRONMENT.iter().map(|e| e.0.len()).max().unwrap_or(0);
    ENVIRONMENT
        .iter()
        .map(|(name, about)| format!("  {name:width$}  {about}"))
        .collect()
}

/// The help of each named command, a blank line between them.
///
/// # Errors
///
/// `unknown_command` for a name that is no command or alias.
pub(crate) fn commands_help(
    table: &[Command],
    rows: &[Row],
    names: &[String],
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let command = table
            .iter()
            .find(|c| c.answers_to(name))
            .ok_or_else(|| unknown_command(name))?;
        if i > 0 {
            lines.push(String::new());
        }
        lines.extend(command_help(command, rows));
    }
    Ok(lines)
}

fn command_help(c: &Command, rows: &[Row]) -> Vec<String> {
    let mut lines = vec![format!("usage: lsty {}", c.synopsis)];
    if !c.aliases.is_empty() {
        lines.push(format!("aliases: {}", c.aliases.join(", ")));
    }
    lines.extend([String::new(), c.about.to_owned(), String::new()]);
    lines.push("options:".to_owned());
    let taken: Vec<&str> = option_names()
        .into_iter()
        .filter(|name| c.takes(name))
        .collect();
    lines.extend(option_lines(&taken));
    let matrix = rows_for(rows, c.name);
    if !matrix.is_empty() {
        lines.extend([String::new(), "agents and retries:".to_owned()]);
        lines.extend(matrix_lines(&matrix));
    }
    lines
}

/// The rows of the matrix that describe the command `name`.
fn rows_for<'a>(rows: &'a [Row], name: &str) -> Vec<&'a Row> {
    rows.iter()
        .filter(|row| {
            row.command.split(", ").any(|part| {
                part.split(' ').next().is_some_and(|first| first == name)
            })
        })
        .collect()
}

fn matrix_lines(rows: &[&Row]) -> Vec<String> {
    let mut lines = Vec::new();
    for row in rows {
        if rows.len() > 1 {
            lines.push(format!("  {}:", row.command));
        }
        lines.extend([
            format!("  Agent reference: {}", row.agent_ref),
            format!("  Retry: {}", retry_text(&row.retry)),
            format!("  Checks: {}", list_text(row.checks)),
            format!("  Files: {}", list_text(row.files)),
        ]);
        lines.extend(effect_lines(row.effects));
    }
    lines
}

fn effect_lines(effects: &[&str]) -> Vec<String> {
    if effects.is_empty() {
        return vec!["  Side effects: none".to_owned()];
    }
    let mut lines = vec!["  Side effects:".to_owned()];
    lines.extend(effects.iter().map(|e| format!("    - {e}")));
    lines
}

fn list_text(items: &[&str]) -> String {
    if items.is_empty() {
        "none".to_owned()
    } else {
        items.join(", ")
    }
}

/// What a retry does, in a few words.
pub(crate) fn retry_text(retry: &Retry) -> String {
    let (when, unless) = (retry.when, retry.unless);
    let text = match retry.kind {
        RetryKind::Detected => return "detected".to_owned(),
        RetryKind::Repeats => "repeats".to_owned(),
        RetryKind::NoOp if when.is_empty() => "no-op".to_owned(),
        RetryKind::NoOp => format!("no-op ({when})"),
    };
    if unless.is_empty() {
        text
    } else {
        format!("{text}; {unless}")
    }
}

/// `lsty help [COMMAND…]` and `lsty help --agent`.
pub fn help(bare: &mut Bare<'_, '_>) -> Result<Output> {
    if bare.parsed.opts.has("agent") {
        if !bare.parsed.args.is_empty() {
            return Err(usage("help --agent takes no command"));
        }
        let lines = crate::agent_guide::agent_guide(MATRIX);
        return Ok(text_output(&lines));
    }
    let lines = match bare.parsed.args.as_slice() {
        [] => overview(&COMMANDS),
        names => commands_help(&COMMANDS, MATRIX, names)?,
    };
    page_lines(bare.env, &lines)?;
    Ok(Output::default())
}

/// The help of the command a `--help` option was given to.
///
/// # Errors
///
/// Only when the text cannot be written.
pub fn command_page(env: &mut Env<'_>, command: &Command) -> Result<()> {
    let lines = commands_help(&COMMANDS, MATRIX, &[command.name.to_owned()])?;
    page_lines(env, &lines)
}

fn text_output(lines: &[String]) -> Output {
    Output {
        fields: Vec::new(),
        lines: lines.iter().map(|l| l.as_bytes().to_vec()).collect(),
    }
}

/// Writes the lines, through the pager when stdout is a terminal that
/// is not dumb. A pager that cannot be started leaves the text printed
/// plainly.
fn page_lines(env: &mut Env<'_>, lines: &[String]) -> Result<()> {
    let terminals = Terminals::new(env.is_tty, env.vars);
    if terminals.pages_help() && run_pager(env, lines) {
        return Ok(());
    }
    write_lines(env.stdout, &text_output(lines).lines)
}

fn run_pager(env: &Env<'_>, lines: &[String]) -> bool {
    let mut words = (env.vars)("PAGER")
        .map(|v| links::split_words(&v))
        .unwrap_or_default();
    if words.is_empty() {
        words.push("less".into());
    }
    let text = format!("{}\n", lines.join("\n"));
    (env.run_pager)(&words, text.as_bytes()).is_ok()
}
