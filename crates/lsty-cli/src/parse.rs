//! The command line: options, arguments and the first command.

use std::collections::BTreeMap;

use lsty_core::{
    errs::{Code, Error, Field, HELP_HINT, Result, quote},
    list::SortField,
};

use crate::{
    commands::{self, Command},
    tty::ColorMode,
};

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

/// Which commands take an option.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Scope {
    /// Every command.
    Always,
    /// The commands that print JSON.
    Json,
    /// The commands that act as someone.
    Identity,
    /// The commands that write.
    Create,
    /// Only the commands that list it.
    Own,
}

/// Declares [`Opt`], one variant per long option with its name, how it takes
/// a value, its usage tail, its sentence and which commands take it. The
/// variants are in alphabetical order of the names.
macro_rules! options {
    ($(
        $variant:ident => (
            $name:literal, $takes:ident, $tail:literal, $about:literal,
            $scope:ident
        );
    )*) => {
        /// A long option.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

            /// What follows the name in the usage text.
            pub(crate) const fn tail(self) -> &'static str {
                match self {
                    $(Self::$variant => $tail,)*
                }
            }

            /// What the option does.
            pub(crate) const fn about(self) -> &'static str {
                match self {
                    $(Self::$variant => $about,)*
                }
            }

            /// Which commands take the option.
            pub(crate) const fn scope(self) -> Scope {
                match self {
                    $(Self::$variant => Scope::$scope,)*
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
    After => ("after", Value, " REF", "place after this item", Own);
    Agent => ("agent", Nothing, "", "print the guide for LLM agents", Own);
    Archive => ("archive", Nothing, "", "move the done item to the done file", Own);
    Before => ("before", Value, " REF", "place before this item", Own);
    Blocks => ("blocks", Value, " REF", "make this item wait for the new one", Own);
    Color => ("color", Value, " MODE", "auto, always or never", Always);
    Create => ("create", Nothing, "", "create the todo file when it is missing", Create);
    Date => ("date", Nothing, "", "stamp the creation date", Own);
    Expect => ("expect", Value, " TEXT", "refuse unless the item holds TEXT", Own);
    File => ("file", Value, " PATH", "use this todo file", Always);
    Filter => ("filter", Value, " QUERY", "keep the items that match", Own);
    Help => ("help", Nothing, ", -h", "print this help", Always);
    Human => ("human", Nothing, "", "act as the human", Identity);
    Id => ("id", Attached, "[=ID]", "give the item an id, or name it for a retry", Own);
    IfAbsent => ("if-absent", Nothing, "", "add only when no open item has the text", Own);
    IgnoreBlocked => ("ignore-blocked", Nothing, "", "go on although the item is blocked", Own);
    Json => ("json", Nothing, "", "print the result as JSON", Json);
    NoArchive => ("no-archive", Nothing, "", "leave the done item in the todo file", Own);
    NoConfig => ("no-config", Nothing, "", "ignore config.toml", Always);
    NoDate => ("no-date", Nothing, "", "do not stamp the creation date", Own);
    Owner => ("owner", Value, " NAME", "act as this agent", Identity);
    Pri => ("pri", Value, " LETTER", "give the new item this priority", Own);
    PrintId => ("print-id", Nothing, "", "print only the id, one per line; the rest goes to stderr", Own);
    Raw => ("raw", Nothing, "", "print the lines as they are in the file, control characters included", Own);
    Sort => ("sort", Value, " FIELD", "order the items by this field", Own);
    Start => ("start", Nothing, "", "claim the item", Own);
    Takeover => ("takeover", Nothing, "", "act on an item that another actor holds", Own);
    Top => ("top", Nothing, "", "place at the top of the list", Own);
    Version => ("version", Nothing, ", -V", "print the version", Always);
}

impl Opt {
    /// The option called `name`.
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|o| o.name() == name)
    }
}

const SHORT: [(&str, Opt); 2] = [("-h", Opt::Help), ("-V", Opt::Version)];

const EXCLUSIVE: [[Opt; 2]; 5] = [
    [Opt::Archive, Opt::NoArchive],
    [Opt::Date, Opt::NoDate],
    [Opt::Owner, Opt::Human],
    [Opt::Json, Opt::Raw],
    [Opt::Json, Opt::PrintId],
];
const POSITIONS: [Opt; 3] = [Opt::Top, Opt::After, Opt::Before];

/// The options of a command line: option to values, `""` for a flag.
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

/// A usage error with the generic hint.
pub(crate) fn usage(message: impl Into<String>) -> Error {
    Error::new(Code::Usage, message).with_hint(HELP_HINT)
}

/// The usage error of a command that lacks an argument.
pub(crate) fn needs() -> Error {
    usage("missing argument")
}

/// The usage error of a command that has an argument too many.
pub(crate) fn too_many() -> Error {
    usage("too many arguments")
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
    let command = scan.args.first().and_then(|a| commands::find(a));
    let mut parsed = Parsed {
        command,
        args: scan.args,
        opts: scan.opts,
        color,
    };
    let result = scan
        .error
        .map_or(Ok(()), Err)
        .and_then(|()| {
            command.map_or(Ok(()), |c| check_taken(c, &parsed.opts))
        })
        .and_then(|()| check_common(&parsed.opts))
        .map_err(|e| command.into_iter().fold(e, with_synopsis));
    if command.is_some() {
        parsed.args.remove(0);
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

/// The checks that need no command: conflicts and accepted values.
fn check_common(opts: &Opts) -> Result<()> {
    check_conflicts(opts)?;
    check_one_of(opts, Opt::Color, &ColorMode::ALL.map(ColorMode::as_str))?;
    check_one_of(opts, Opt::Sort, &SortField::NAMES)
}

/// Fails on the first option, in alphabetical order, that `command` does
/// not take.
pub(crate) fn check_taken(command: &Command, opts: &Opts) -> Result<()> {
    for opt in opts.given() {
        let create_only_writing =
            opt == Opt::Create && !command.writes_with(opts);
        if !command.takes(opt) || create_only_writing {
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
    Err(Error::invalid_value(message, value, accepted)
        .with_field("field", Field::Str(format!("--{name}"))))
}
