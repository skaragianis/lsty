//! Commands run for a caller that shows the result itself, as the `:`
//! line of the TUI does: the human acts, nothing is printed, and a write
//! hands back what the operation did.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use lsty_core::errs::{Code, Error, Field, Result, quote};
use lsty_core::ops::{self, Actor, Outcome};

use crate::commands::{self, Base, Call, Command, Handler};
use crate::env::Env;
use crate::identity;
use crate::paint::Painter;
use crate::parse::{self, Opt, Opts};

/// The options that say how the process prints or who acts, which a
/// caller that does both itself refuses.
const REFUSED: [Opt; 10] = [
    Opt::Owner,
    Opt::Human,
    Opt::Json,
    Opt::Raw,
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
    let command = match parsed.command {
        Some(command) if command.inline() => command,
        Some(command) => return Err(unknown(command.name)),
        None => return Err(unknown_first(&parsed.args)),
    };
    ensure_identity(command, &parsed.opts)?;
    with_shell(env, home, |shell| {
        let mut call = Call {
            command,
            env: shell,
            parsed: &parsed,
            base: Base::Caller(env),
            actor: Actor::Human,
            paint: Painter::off(),
        };
        match command.handler {
            Handler::Read(handler) => {
                Ok(Inline::Read(handler(&mut call)?.lines))
            }
            Handler::Write(handler) => {
                Ok(Inline::Wrote(Box::new(handler(&mut call)?.outcome)))
            }
            Handler::Bare(_) | Handler::Tui => Err(unknown(command.name)),
        }
    })
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

/// The human always acts here, so only the rule that a start needs an
/// identity applies. `stdin_tty` is passed as true on purpose: the
/// refusal to write without a terminal belongs to the process, which the
/// caller has.
fn ensure_identity(command: &Command, opts: &Opts) -> Result<()> {
    let no_vars = |_: &str| None;
    identity::identify(command, opts, &no_vars, true).map(drop)
}

fn unknown_first(args: &[String]) -> Error {
    match args.first() {
        Some(name) => unknown(name),
        None => Error::new(Code::Usage, "missing command"),
    }
}

fn unknown(name: &str) -> Error {
    Error::new(
        Code::UnknownCommand,
        format!("unknown command {}", quote(name)),
    )
    .with_field("command", Field::Str(name.to_owned()))
}

const NO_PROGRAMS: &str = "not available in the TUI";

/// Runs `run` with the process's streams and hooks for a command that
/// uses none: reading gives nothing, writing is discarded and the
/// programs are refused.
fn with_shell<T>(
    base: &ops::Env<'_>,
    home: Option<&Path>,
    run: impl FnOnce(&mut Env<'_>) -> T,
) -> T {
    let (mut stdin, mut stdout, mut stderr) =
        (io::empty(), io::sink(), io::sink());
    let mut env = Env {
        args: Vec::new(),
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        vars: &|_| None,
        cwd: PathBuf::from("."),
        home: home.map(Path::to_path_buf),
        is_tty: [false; 3],
        now: base.clock.now(),
        zone: base.clock.zone().clone(),
        version: "",
        run_editor: &|_: &[OsString]| Err(NO_PROGRAMS.to_owned()),
        open_path: &|_: &Path| Err(NO_PROGRAMS.to_owned()),
        run_pager: &|_, _| Err(io::Error::other(NO_PROGRAMS)),
        run_tui: None,
    };
    run(&mut env)
}
