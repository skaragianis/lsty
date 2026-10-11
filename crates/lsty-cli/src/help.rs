//! `lsty help`: the text generated from the command table, the options
//! and the safety matrix, and how it reaches the reader.

use lsty_core::{
    errs::{self, Code, Result},
    links,
};

mod keys;
mod themes;

use crate::{
    commands::{Bare, COMMANDS, Command, find_in},
    dispatch::{USAGE_TEXT, unknown_command},
    env::Env,
    matrix::{MATRIX, Retry, RetryKind, Row},
    output::Output,
    parse::{Opt, usage},
    tty::Terminals,
};

/// What each exit status means.
const EXIT_MEANINGS: [(u8, &str); 5] = [
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

type Page = fn() -> Vec<String>;

/// The pages that are no command: the name `lsty help` takes, what the
/// overview says of it, and the page.
const TOPICS: [(&str, &str, Page); 2] = [
    ("keys", "the actions and keys of keys.toml", keys::keys_page),
    ("themes", themes::ABOUT, themes::themes_page),
];

/// The lines of the overview that point at the topics.
fn topic_lines() -> Vec<String> {
    TOPICS
        .iter()
        .map(|(name, about, _)| {
            format!("       {:28}{about}", format!("lsty help {name}"))
        })
        .collect()
}

/// The widest synopsis that sets the column of the commands; a longer
/// one has its sentence on the next line.
const SYNOPSIS_WIDTH: usize = 24;

/// What a query accepts, for `lsty help` and the commands that filter.
/// The `is:` words come from the parser.
fn query_lines() -> Vec<String> {
    let states: Vec<String> = lsty_core::query::state_words()
        .map(|word| format!("is:{word}"))
        .collect();
    let mut lines: Vec<String> = [
        "Terms are separated by spaces and all must match.",
        "  word, \"two words\"    text in the item; a capital: case-sensitive",
        "  +project, @context   a name",
        "  key:value            a tag; key: alone, any value",
        "  pri:B                pri:A-C pri:<=B; pri: alone, any priority",
        "  due:<fri             also t, started, created, completed; due: alone,",
        "                       any date; < <= > >= =; today, fri, +3d, -7d,",
        "                       2026-10-01",
        "  a|b                  either",
        "  -TERM                not (the whole term); an argument needs -- first",
        "not, or and and are plain words: use a|b or --filter -a.",
    ]
    .map(str::to_owned)
    .into();
    let list = format!("States: {}", states.join(" "));
    lines.extend(wrap("", "  ", &list, GUIDE_WIDTH));
    lines
}

/// The text of `lsty help`.
fn overview(table: &[Command]) -> Vec<String> {
    let mut lines: Vec<String> =
        USAGE_TEXT.lines().map(str::to_owned).collect();
    lines.extend(topic_lines());
    lines.extend([String::new(), "commands:".to_owned()]);
    lines.extend(command_lines(table));
    lines.extend([String::new(), "options:".to_owned()]);
    lines.extend(option_lines(Opt::ALL));
    lines.extend([String::new(), "queries:".to_owned()]);
    lines.extend(query_lines());
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

fn option_lines(opts: &[Opt]) -> Vec<String> {
    let docs: Vec<_> = opts
        .iter()
        .map(|o| (format!("--{}{}", o.name(), o.tail()), o.about()))
        .collect();
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
fn commands_help(
    table: &[Command],
    rows: &[Row],
    names: &[String],
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }
        if let Some((.., page)) =
            TOPICS.iter().find(|(t, ..)| name.eq_ignore_ascii_case(t))
        {
            lines.extend(page());
            continue;
        }
        let command =
            find_in(table, name).ok_or_else(|| unknown_command(name))?;
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
    let taken: Vec<Opt> =
        Opt::ALL.iter().copied().filter(|&o| c.takes(o)).collect();
    lines.extend(option_lines(&taken));
    if c.takes(Opt::Filter) {
        lines.extend([String::new(), "queries:".to_owned()]);
        lines.extend(query_lines());
    }
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
pub(crate) fn help(bare: &mut Bare<'_, '_>) -> Result<Output> {
    if bare.parsed.opts.has(Opt::Agent) {
        if !bare.parsed.args.is_empty() {
            return Err(usage("help --agent takes no command"));
        }
        let lines = crate::agent_guide::agent_guide(MATRIX);
        return Ok(Output::lines(&lines));
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
pub(crate) fn command_page(
    env: &mut Env<'_>,
    command: &Command,
) -> Result<()> {
    page_lines(env, &command_help(command, MATRIX))
}

/// Writes the lines, through the pager when stdout is a terminal that
/// is not dumb. A pager that cannot be started leaves the text printed
/// plainly.
fn page_lines(env: &mut Env<'_>, lines: &[String]) -> Result<()> {
    let terminals = Terminals::new(env.is_tty, env.vars);
    if terminals.pages_help() && run_pager(env, lines) {
        return Ok(());
    }
    env.out(&lines.iter().map(String::as_bytes).collect::<Vec<_>>())
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

/// The widest line of the agent guide and of the query lines, in bytes.
pub(crate) const GUIDE_WIDTH: usize = 80;

/// Breaks `text` into lines of at most `width` bytes at its spaces. The
/// first line starts with `first`, the others with `rest`; a word longer
/// than a line stands alone.
pub(crate) fn wrap(
    first: &str,
    rest: &str,
    text: &str,
    width: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = first.to_owned();
    for word in text.split_whitespace() {
        if line == first {
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

#[cfg(test)]
mod help_tests;
