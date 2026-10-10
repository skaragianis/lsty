//! Commands run for a caller that shows the result itself, as the `:`
//! line of the TUI does: the human acts, nothing is printed, and a write
//! hands back what the operation did.

use std::path::Path;

use lsty_core::{
    errs::{Code, Error, Field, Result, quote},
    ops::{self, Actor, Outcome},
};

use crate::{
    commands::{self, Call, Handler},
    identity,
    paint::Painter,
    parse::{self, Opt, Opts},
};

/// The options that say how the process prints or who acts, which a
/// caller that does both itself refuses.
const REFUSED: [Opt; 11] = [
    Opt::Owner,
    Opt::Human,
    Opt::Json,
    Opt::Raw,
    Opt::PrintId,
    Opt::File,
    Opt::NoConfig,
    Opt::Color,
    Opt::Create,
    Opt::Help,
    Opt::Version,
];

/// What a command run inline did.
#[derive(Debug)]
pub enum Inline {
    /// A read: what it prints, one line each, as bytes.
    Read(Vec<Vec<u8>>),
    /// A write: what the operation did.
    Wrote(Box<Outcome>),
}

/// The commands that run inline, each with its aliases.
pub fn inline_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for command in &commands::COMMANDS {
        if command.inline() {
            names.push(command.name);
            names.extend(command.aliases.iter().copied());
        }
    }
    names
}

/// The options, as written, that the command called `name` takes when it
/// runs inline.
pub fn inline_options(name: &str) -> Vec<String> {
    let Some(command) = commands::find(name) else {
        return Vec::new();
    };
    if !command.inline() {
        return Vec::new();
    }
    command
        .opts
        .iter()
        .filter(|opt| !REFUSED.contains(opt))
        .map(|opt| format!("--{}", opt.name()))
        .collect()
}

/// Runs the command given as `words` as the human, with the files, the
/// configuration, the clock and the way to wait for the lock of `env`.
/// It prints nothing.
///
/// # Errors
///
/// A usage error for an option that is not available, or for what the
/// parser, the command or its operation refuses.
pub fn run_inline(
    env: &ops::Env<'_>,
    home: Option<&Path>,
    words: &[String],
) -> Result<Inline> {
    let (parsed, parsing) = parse::parse(words);
    refuse_options(&parsed.opts)?;
    parsing?;
    let Some(command) = parsed.command else {
        return Err(unknown_first(&parsed.args));
    };
    identity::refuse_start(&parsed.opts)?;
    let call = Call {
        command,
        home,
        parsed: &parsed,
        ops: ops::Env {
            actor: Actor::Human,
            ..env.clone()
        },
        paint: Painter::off(),
    };
    let result = match command.handler {
        Handler::Read(handler) => {
            handler(&call).map(|r| Inline::Read(r.lines))
        }
        Handler::Write(handler) => {
            handler(&call).map(|w| Inline::Wrote(Box::new(w.outcome)))
        }
        Handler::Tty(_) | Handler::Bare(_) | Handler::Tui => {
            Err(unknown(command.name))
        }
    };
    result.map_err(|e| parse::with_generic_synopsis(e, command))
}

fn refuse_options(opts: &Opts) -> Result<()> {
    match REFUSED.iter().find(|opt| opts.has(**opt)) {
        Some(opt) => Err(Error::new(
            Code::Usage,
            format!("--{} is not available in the TUI", opt.name()),
        )),
        None => Ok(()),
    }
}

fn unknown_first(args: &[String]) -> Error {
    match args.first() {
        Some(name) => unknown(name),
        None => Error::new(Code::Usage, "missing command"),
    }
}

pub(crate) fn unknown(name: &str) -> Error {
    Error::new(
        Code::UnknownCommand,
        format!("unknown command {}", quote(name)),
    )
    .with_field("command", Field::Str(name.to_owned()))
}
