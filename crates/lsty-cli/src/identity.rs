//! Who is acting: `--human`, `--owner`, `$LSTY_OWNER`, and the refusal of
//! a write that does not say.

use lsty_core::errs::{Code, Error, Field, Result, Vars};
use lsty_core::ops::Actor;

use crate::commands::{Command, Kind};
use crate::parse::{Opt, Opts};

/// The variable that names the agent.
const OWNER_VAR: &str = "LSTY_OWNER";

/// Decides the actor of one command and refuses a write that has no
/// identity and no terminal. `stdin_tty` is whether stdin is a terminal,
/// after `LSTY_NONINTERACTIVE`.
///
/// # Errors
///
/// `invalid_value` for a bad name, `no_identity` for a refused write.
pub fn identify(
    command: &Command,
    opts: &Opts,
    vars: &Vars<'_>,
    stdin_tty: bool,
) -> Result<Actor> {
    let actor = actor(opts, vars)?;
    require(command, opts, &actor, stdin_tty)?;
    Ok(actor)
}

fn actor(opts: &Opts, vars: &Vars<'_>) -> Result<Actor> {
    if opts.has(Opt::Human) {
        return Ok(Actor::Human);
    }
    if let Some(name) = opts.first(Opt::Owner) {
        if name.is_empty() {
            return Err(invalid_owner("an agent name is not empty"));
        }
        return Actor::named(name);
    }
    match vars(OWNER_VAR) {
        None => Ok(Actor::Human),
        Some(value) => match value.to_str() {
            Some(name) => Actor::named(name),
            None => Err(invalid_owner("an agent name must be valid UTF-8")),
        },
    }
}

fn invalid_owner(message: &str) -> Error {
    Error::new(Code::InvalidValue, message)
        .with_field("field", Field::Str("owner".to_owned()))
}

fn require(
    command: &Command,
    opts: &Opts,
    actor: &Actor,
    stdin_tty: bool,
) -> Result<()> {
    if actor.is_human() && starts_work(command, opts) {
        return Err(no_identity(
            "next --start needs an identity",
            "set LSTY_OWNER, one name per worker",
        ));
    }
    let silent = actor.is_human() && !opts.has(Opt::Human) && !stdin_tty;
    if command.kind == Kind::Write && silent {
        return Err(no_identity(
            "a write without a terminal must say who is acting",
            "set LSTY_OWNER, or pass --human",
        ));
    }
    Ok(())
}

/// Whether the command is `next --start`, which always needs a name.
fn starts_work(command: &Command, opts: &Opts) -> bool {
    command.name == "next" && opts.has(Opt::Start)
}

fn no_identity(message: &str, hint: &str) -> Error {
    Error::new(Code::NoIdentity, message).with_hint(hint)
}
