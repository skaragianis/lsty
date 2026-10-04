//! The command line: options, arguments and the first command.

use std::collections::BTreeMap;

use lsty_core::errs::{Code, Error, Field, HELP_HINT, Result, quote};
use lsty_core::list::SortField;

use crate::commands::{self, Command};
use crate::tty::ColorMode;

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

/// Declares [`Opt`], one variant per long option with its name and how it
/// takes a value. The variants are in alphabetical order of the names.
macro_rules! options {
    ($($variant:ident => ($name:literal, $takes:ident);)*) => {
        /// A long option.
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
        )]
        pub(crate) enum Opt {
            $(
                #[doc = concat!("`--", $name, "`")]
                $variant,
            )*
        }

        impl Opt {
            /// Every option, in alphabetical order of the names.
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// The name after the dashes.
            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            const fn takes(self) -> Takes {
                match self {
                    $(Self::$variant => Takes::$takes,)*
                }
            }
        }
    };
}

options! {
    After => ("after", Value);
    Agent => ("agent", Nothing);
    Archive => ("archive", Nothing);
    Before => ("before", Value);
    Color => ("color", Value);
    Create => ("create", Nothing);
    Date => ("date", Nothing);
    Expect => ("expect", Value);
    File => ("file", Value);
    Filter => ("filter", Value);
    Help => ("help", Nothing);
    Human => ("human", Nothing);
    Id => ("id", Attached);
    IfAbsent => ("if-absent", Nothing);
    IgnoreBlocked => ("ignore-blocked", Nothing);
    Json => ("json", Nothing);
    Later => ("later", Nothing);
    NoArchive => ("no-archive", Nothing);
    NoConfig => ("no-config", Nothing);
    NoDate => ("no-date", Nothing);
    Owner => ("owner", Value);
    Pri => ("pri", Value);
    Raw => ("raw", Nothing);
    Sort => ("sort", Value);
    Start => ("start", Nothing);
    Takeover => ("takeover", Nothing);
    Top => ("top", Nothing);
    Version => ("version", Nothing);
}

impl Opt {
    /// The option called `name`.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|o| o.name() == name)
    }
}

const SHORT: [(&str, Opt); 2] = [("-h", Opt::Help), ("-V", Opt::Version)];

/// The values `--sort` accepts.
pub(crate) const SORT_FIELDS: [&str; 6] = SortField::NAMES;

const EXCLUSIVE: [[Opt; 2]; 4] = [
    [Opt::Archive, Opt::NoArchive],
    [Opt::Date, Opt::NoDate],
    [Opt::Owner, Opt::Human],
    [Opt::Json, Opt::Raw],
];
const POSITIONS: [Opt; 3] = [Opt::Top, Opt::After, Opt::Before];

/// The options of a command line: name to values, `""` for a flag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Opts(BTreeMap<Opt, Vec<String>>);

impl Opts {
    /// Whether the option was given.
    pub(crate) fn has(&self, opt: Opt) -> bool {
        self.0.contains_key(&opt)
    }

    /// The first value of the option; `Some("")` for a flag.
    pub(crate) fn first(&self, opt: Opt) -> Option<&str> {
        self.0.get(&opt)?.first().map(String::as_str)
    }

    /// Every value of the option, in order.
    pub(crate) fn all(&self, opt: Opt) -> &[String] {
        self.0.get(&opt).map_or(&[], Vec::as_slice)
    }

    /// The options given, in alphabetical order of their names.
    pub(crate) fn given(&self) -> impl Iterator<Item = Opt> {
        self.0.keys().copied()
    }
}

/// A command line split into command, arguments and options.
#[derive(Clone, Debug)]
pub(crate) struct Parsed {
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
pub(crate) fn usage(message: impl Into<String>) -> Error {
    Error::new(Code::Usage, message).with_hint(HELP_HINT)
}

/// The usage error of a command that lacks an argument.
pub(crate) fn needs(synopsis: &str) -> Error {
    Error::new(Code::Usage, "missing argument")
        .with_hint(format!("usage: lsty {synopsis}"))
}

/// The usage error of a command that has an argument too many.
pub(crate) fn too_many(synopsis: &str) -> Error {
    Error::new(Code::Usage, "too many arguments")
        .with_hint(format!("usage: lsty {synopsis}"))
}

/// A usage error takes the synopsis of `command` as its hint.
pub(crate) fn with_synopsis(err: Error, command: &Command) -> Error {
    if err.code() == Code::Usage {
        err.with_hint(format!("usage: lsty {}", command.synopsis))
    } else {
        err
    }
}

/// Like [`with_synopsis`], but keeps a hint the error has of its own.
pub(crate) fn with_generic_synopsis(err: Error, command: &Command) -> Error {
    if err.hint() == Some(HELP_HINT) {
        with_synopsis(err, command)
    } else {
        err
    }
}

/// Splits `tokens` into a [`Parsed`]. On error it returns what it read
/// with the first error.
pub(crate) fn parse(tokens: &[String]) -> (Parsed, Result<()>) {
    let mut scan = Scanner::default();
    scan.scan(tokens);
    let color = scan
        .opts
        .first(Opt::Color)
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
            Some(&(_, opt)) => self.add(opt, String::new()),
            None => self.fail(format!("unknown option {token}")),
        }
    }

    /// Reads a long option; returns how many following tokens it took.
    fn long(&mut self, body: &str, next: Option<&String>) -> usize {
        let (name, value) = match body.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (body, None),
        };
        let Some(opt) = Opt::from_name(name) else {
            self.fail(format!("unknown option --{name}"));
            return 0;
        };
        match (opt.takes(), value) {
            (Takes::Nothing, Some(_)) => {
                self.fail(format!("option --{name} takes no value"));
            }
            (Takes::Value, None) => {
                let Some(next) = next else {
                    self.fail(format!("option --{name} needs a value"));
                    return 0;
                };
                self.add(opt, next.clone());
                return 1;
            }
            (_, value) => self.add(opt, value.unwrap_or("").to_owned()),
        }
        0
    }

    fn add(&mut self, opt: Opt, value: String) {
        if self.opts.has(opt) && opt != Opt::Expect {
            self.fail(format!("option --{} given twice", opt.name()));
            return;
        }
        self.opts.0.entry(opt).or_default().push(value);
    }
}

/// Checks the options against `command`.
pub(crate) fn check(command: &Command, opts: &Opts) -> Result<()> {
    check_taken(command, opts)?;
    check_common(opts)
}

/// The checks that need no command: conflicts and accepted values.
fn check_common(opts: &Opts) -> Result<()> {
    check_conflicts(opts)?;
    check_one_of(opts, Opt::Color, &ColorMode::NAMES)?;
    check_one_of(opts, Opt::Sort, &SORT_FIELDS)
}

/// Fails on the first option, in alphabetical order, that `command` does
/// not take.
pub(crate) fn check_taken(command: &Command, opts: &Opts) -> Result<()> {
    for opt in opts.given() {
        let lacks_start = command.name == "next"
            && opt == Opt::Create
            && !opts.has(Opt::Start);
        if !command.takes(opt) || lacks_start {
            return Err(usage(format!(
                "{} does not take --{}",
                command.name,
                opt.name()
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

fn conflict(opts: &Opts, among: &[Opt]) -> Result<()> {
    let mut given = among.iter().filter(|&&o| opts.has(o));
    match (given.next(), given.next()) {
        (Some(a), Some(b)) => Err(usage(format!(
            "--{} and --{} cannot be used together",
            a.name(),
            b.name()
        ))),
        _ => Ok(()),
    }
}

fn check_one_of(opts: &Opts, opt: Opt, accepted: &[&str]) -> Result<()> {
    let name = opt.name();
    let Some(value) = opts.first(opt) else {
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
