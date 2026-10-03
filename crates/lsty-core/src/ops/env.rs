//! What an operation needs from its caller.

use std::time::Duration;

use crate::config::Config;
use crate::dates::Clock;
use crate::discover::Found;
use crate::errs::{Code, Error, Field, Result};
use crate::fsio::{Hooks, OsHooks};
use crate::lock::Cancel;
use crate::todotxt::Owner;

use super::ids::{IdSource, OsIds};

/// Who is acting: the human, or an agent with a name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Actor {
    /// A person at the keyboard.
    #[default]
    Human,
    /// An agent, which writes its name as `owner:`.
    Agent(Owner),
}

impl Actor {
    /// The actor called `owner`; the empty name is the human.
    ///
    /// # Errors
    ///
    /// `invalid_value` when the name has whitespace or a colon.
    pub fn named(owner: &str) -> Result<Self> {
        if owner.is_empty() {
            return Ok(Self::Human);
        }
        Owner::new(owner).map(Self::Agent).ok_or_else(|| {
            Error::new(
                Code::InvalidValue,
                "an agent name has no whitespace or colon",
            )
            .with_field("field", Field::Str("owner".to_owned()))
        })
    }

    /// The name written as `owner:`; empty for the human.
    pub fn name(&self) -> &str {
        match self {
            Self::Human => "",
            Self::Agent(owner) => owner.as_str(),
        }
    }

    /// Whether the actor is the human.
    pub const fn is_human(&self) -> bool {
        matches!(self, Self::Human)
    }
}

/// Everything an operation reads from outside the list: the files, the
/// configuration, the clock and how to wait for the lock.
pub struct Env<'a> {
    /// The todo and done files.
    pub files: &'a Found,
    /// The configuration.
    pub config: &'a Config,
    /// The one reading of time for this operation.
    pub clock: &'a Clock,
    /// Who is acting.
    pub actor: Actor,
    /// Whether a missing todo file is an empty one (`--create`).
    pub create: bool,
    /// How long to wait for the lock.
    pub timeout: Duration,
    /// Ends the wait for the lock.
    pub cancel: Cancel,
    /// The points of a write that tests replace.
    pub hooks: &'a dyn Hooks,
    /// Where generated ids come from.
    pub ids: &'a dyn IdSource,
}

impl<'a> Env<'a> {
    /// An environment for the human, with the lock timeout of the
    /// configuration, no `--create` and the real file system and random ids.
    pub fn new(
        files: &'a Found,
        config: &'a Config,
        clock: &'a Clock,
    ) -> Self {
        Self {
            files,
            config,
            clock,
            actor: Actor::Human,
            create: false,
            timeout: config.lock_timeout,
            cancel: Cancel::new(),
            hooks: &OsHooks,
            ids: &OsIds,
        }
    }
}
