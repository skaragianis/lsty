//! The command line: options, arguments and the first command.

use std::collections::BTreeMap;

use lsty_core::errs::{Code, Error, Field, Result, quote};
use lsty_core::list::SortField;

use crate::commands::{self, Command};
use crate::tty::ColorMode;

/// The hint of a usage error that has no synopsis to give.
pub const HELP_HINT: &str = "run \"lsty help\"";

/// How an option takes its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Takes {
    /// No value.
    Nothing,
    /// The next token, whatever it starts with, or `=value`.
    Value,
    /// `=value` only.
    Attached,
}

const OPTIONS: [(&str, Takes); 28] = [
    ("file", Takes::Value),
    ("color", Takes::Value),
    ("expect", Takes::Value),
    ("owner", Takes::Value),
    ("filter", Takes::Value),
    ("sort", Takes::Value),
    ("after", Takes::Value),
    ("before", Takes::Value),
    ("pri", Takes::Value),
    ("id", Takes::Attached),
    ("json", Takes::Nothing),
    ("raw", Takes::Nothing),
    ("no-config", Takes::Nothing),
    ("if-absent", Takes::Nothing),
    ("ignore-blocked", Takes::Nothing),
    ("takeover", Takes::Nothing),
    ("human", Takes::Nothing),
    ("start", Takes::Nothing),
    ("create", Takes::Nothing),
    ("top", Takes::Nothing),
    ("later", Takes::Nothing),
    ("archive", Takes::Nothing),
    ("no-archive", Takes::Nothing),
    ("date", Takes::Nothing),
    ("no-date", Takes::Nothing),
    ("agent", Takes::Nothing),
    ("help", Takes::Nothing),
    ("version", Takes::Nothing),
];

const SHORT: [(&str, &str); 2] = [("-h", "help"), ("-V", "version")];

/// The values `--sort` accepts.
pub const SORT_FIELDS: [&str; 6] = SortField::NAMES;

const EXCLUSIVE: [[&str; 2]; 4] = [
    ["archive", "no-archive"],
    ["date", "no-date"],
    ["owner", "human"],
    ["json", "raw"],
];
const POSITIONS: [&str; 3] = ["top", "after", "before"];

/// The options of a command line: name to values, `""` for a flag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opts(BTreeMap<&'static str, Vec<String>>);

impl Opts {
    /// Whether the option was given.
    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// The first value of the option; `Some("")` for a flag.
    pub fn first(&self, name: &str) -> Option<&str> {
        self.0.get(name)?.first().map(String::as_str)
    }

    /// Every value of the option, in order.
    pub fn all(&self, name: &str) -> &[String] {
        self.0.get(name).map_or(&[], Vec::as_slice)
    }

    /// The names given, in alphabetical order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        self.0.keys().copied()
    }
}

/// A command line split into command, arguments and options.
#[derive(Clone, Debug)]
pub struct Parsed {
    /// The command; `None` when the first argument is not one.
    pub command: Option<&'static Command>,
    /// The positional arguments, without the command.
    pub args: Vec<String>,
    /// The options.
    pub opts: Opts,
    /// `--color`; `auto` when it is not given.
    pub color: ColorMode,
}

impl Default for Parsed {
    fn default() -> Self {
        Self {
            command: None,
            args: Vec::new(),
            opts: Opts::default(),
            color: ColorMode::Auto,
        }
    }
}

/// A usage error with the generic hint.
pub fn usage(message: impl Into<String>) -> Error {
    Error::new(Code::Usage, message).with_hint(HELP_HINT)
}

/// The usage error of a command that lacks an argument.
pub fn needs(synopsis: &str) -> Error {
    Error::new(Code::Usage, "missing argument")
        .with_hint(format!("usage: lsty {synopsis}"))
}

/// The usage error of a command that has an argument too many.
pub fn too_many(synopsis: &str) -> Error {
    Error::new(Code::Usage, "too many arguments")
        .with_hint(format!("usage: lsty {synopsis}"))
}

/// A usage error takes the synopsis of `command` as its hint.
pub fn with_synopsis(err: Error, command: &Command) -> Error {
    if err.code() == Code::Usage {
        err.with_hint(format!("usage: lsty {}", command.synopsis))
    } else {
        err
    }
}

/// Like [`with_synopsis`], but keeps a hint the error has of its own.
pub fn with_generic_synopsis(err: Error, command: &Command) -> Error {
    if err.hint() == Some(HELP_HINT) {
        with_synopsis(err, command)
    } else {
        err
    }
}

/// Splits `tokens` into a [`Parsed`]. On error it returns what it read
/// with the first error.
pub fn parse(tokens: &[String]) -> (Parsed, Result<()>) {
    let mut scan = Scanner::default();
    scan.scan(tokens);
    let color = scan
        .opts
        .first("color")
        .and_then(ColorMode::parse)
        .unwrap_or(ColorMode::Auto);
    let mut parsed = Parsed {
        command: None,
        args: scan.args,
        opts: scan.opts,
        color,
    };
    let command = parsed.args.first().and_then(|a| commands::find(a));
    let mut result = scan.error.map_or(Ok(()), Err);
    if let Some(command) = command {
        parsed.command = Some(command);
        parsed.args.remove(0);
        result = result.and_then(|()| check(command, &parsed.opts));
        result = result.map_err(|e| with_synopsis(e, command));
    } else {
        result = result.and_then(|()| check_common(&parsed.opts));
    }
    (parsed, result)
}

#[derive(Default)]
struct Scanner {
    args: Vec<String>,
    opts: Opts,
    only_args: bool,
    error: Option<Error>,
}

impl Scanner {
    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(usage(message));
        }
    }

    fn scan(&mut self, tokens: &[String]) {
        let mut at = 0;
        while let Some(token) = tokens.get(at) {
            at += 1;
            if self.only_args || token.len() < 2 || !token.starts_with('-') {
                self.args.push(token.clone());
            } else if token == "--" {
                self.only_args = true;
            } else if let Some(body) = token.strip_prefix("--") {
                at += self.long(body, tokens.get(at));
            } else {
                self.short(token);
            }
        }
    }

    fn short(&mut self, token: &str) {
        match SHORT.iter().find(|(short, _)| *short == token) {
            Some((_, name)) => self.add(name, String::new()),
            None => self.fail(format!("unknown option {token}")),
        }
    }

    /// Reads a long option; returns how many following tokens it took.
    fn long(&mut self, body: &str, next: Option<&String>) -> usize {
        let (name, value) = match body.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (body, None),
        };
        let Some((name, takes)) = OPTIONS.iter().find(|(n, _)| *n == name)
        else {
            self.fail(format!("unknown option --{name}"));
            return 0;
        };
        match (takes, value) {
            (Takes::Nothing, Some(_)) => {
                self.fail(format!("option --{name} takes no value"));
            }
            (Takes::Value, None) => {
                let Some(next) = next else {
                    self.fail(format!("option --{name} needs a value"));
                    return 0;
                };
                self.add(name, next.clone());
                return 1;
            }
            (_, value) => self.add(name, value.unwrap_or("").to_owned()),
        }
        0
    }

    fn add(&mut self, name: &'static str, value: String) {
        if self.opts.has(name) && name != "expect" {
            self.fail(format!("option --{name} given twice"));
            return;
        }
        self.opts.0.entry(name).or_default().push(value);
    }
}

/// Checks the options against `command`.
pub fn check(command: &Command, opts: &Opts) -> Result<()> {
    check_taken(command, opts)?;
    check_common(opts)
}

/// The checks that need no command: conflicts and accepted values.
fn check_common(opts: &Opts) -> Result<()> {
    check_conflicts(opts)?;
    check_one_of(opts, "color", &ColorMode::NAMES)?;
    check_one_of(opts, "sort", &SORT_FIELDS)
}

/// Fails on the first option, in alphabetical order, that `command` does
/// not take.
pub fn check_taken(command: &Command, opts: &Opts) -> Result<()> {
    for name in opts.names() {
        let lacks_start =
            command.name == "next" && name == "create" && !opts.has("start");
        if !command.takes(name) || lacks_start {
            return Err(usage(format!(
                "{} does not take --{name}",
                command.name
            )));
        }
    }
    Ok(())
}

fn check_conflicts(opts: &Opts) -> Result<()> {
    for pair in EXCLUSIVE {
        conflict(opts, &pair)?;
    }
    conflict(opts, &POSITIONS)
}

fn conflict(opts: &Opts, names: &[&str]) -> Result<()> {
    let mut given = names.iter().filter(|n| opts.has(n));
    match (given.next(), given.next()) {
        (Some(a), Some(b)) => {
            Err(usage(format!("--{a} and --{b} cannot be used together")))
        }
        _ => Ok(()),
    }
}

fn check_one_of(opts: &Opts, name: &str, accepted: &[&str]) -> Result<()> {
    let Some(value) = opts.first(name) else {
        return Ok(());
    };
    if accepted.contains(&value) {
        return Ok(());
    }
    let message = format!(
        "--{name}: {} is not one of {}",
        quote(value),
        accepted.join(", ")
    );
    Err(Error::new(Code::InvalidValue, message)
        .with_field("field", Field::Str(format!("--{name}")))
        .with_field("value", Field::Str(value.to_owned()))
        .with_field(
            "accepted",
            Field::Strs(accepted.iter().map(|a| (*a).to_owned()).collect()),
        ))
}
