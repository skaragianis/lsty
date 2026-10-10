//! What an operation needs from its caller.

use std::time::Duration;

use super::ids::{IdSource, OsIds};
use crate::{
    config::Config,
    dates::Clock,
    discover::Found,
    errs::{Code, Error, Field, Result},
    fsio::{Hooks, OsHooks},
    list::{self, Params, Snapshot},
    lock::Cancel,
};

/// Who is acting: the human, or an agent with a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Actor {
    /// A person at the keyboard.
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
        if owner.contains(|c: char| c == ':' || c.is_whitespace()) {
            return Err(Error::new(
                Code::InvalidValue,
                "an agent name has no whitespace or colon",
            )
            .with_field("field", Field::Str("owner".to_owned())));
        }
        Ok(Self::Agent(Owner(owner.to_owned())))
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

/// The name of an agent: the value of its `owner:` tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner(String);

impl Owner {
    /// The name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A flag that can override a setting of the configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Toggle {
    /// Follow the configuration.
    #[default]
    Config,
    /// On, whatever the configuration says.
    On,
    /// Off, whatever the configuration says.
    Off,
}

impl Toggle {
    /// Whether the setting is on, given what the configuration says.
    pub(crate) const fn resolve(self, config: bool) -> bool {
        match self {
            Self::Config => config,
            Self::On => true,
            Self::Off => false,
        }
    }
}

/// Everything an operation reads from outside the list: the files, the
/// configuration, the clock and how to wait for the lock.
#[derive(Clone)]
pub struct Env<'a> {
    /// The todo and done files.
    pub found: &'a Found,
    /// The configuration.
    pub config: &'a Config,
    /// The one reading of time for this operation.
    pub clock: &'a Clock,
    /// Who is acting.
    pub actor: Actor,
    /// Whether a missing todo file is an empty one (`--create`).
    pub create: bool,
    /// How long to wait for the lock. It starts as `lock_timeout` of the
    /// configuration, and this field is what the operations use.
    pub timeout: Duration,
    /// Ends the wait for the lock.
    pub cancel: Cancel,
    /// Whether an operation keeps what an undo needs (every line before
    /// and after). It costs a copy of each written file, so it is off
    /// unless the caller undoes.
    pub record_undo: bool,
    /// The points of a write that tests replace.
    pub hooks: &'a dyn Hooks,
    /// Where generated ids come from.
    pub ids: &'a dyn IdSource,
}

impl Env<'_> {
    /// What building a list needs: the clock and when a claim is stale.
    pub(crate) fn params(&self) -> Params {
        Params::new(self.clock.clone(), self.config)
    }

    /// Reads both files under shared locks.
    ///
    /// # Errors
    ///
    /// Whatever reading the lists fails with.
    pub fn load(&self) -> Result<Snapshot> {
        list::load(self.found, self.timeout, &self.cancel, &self.params())
    }
}

impl<'a> Env<'a> {
    /// An environment for the human, with the lock timeout of the
    /// configuration, no `--create` and the real file system and random ids.
    pub fn new(
        found: &'a Found,
        config: &'a Config,
        clock: &'a Clock,
    ) -> Self {
        Self {
            found,
            config,
            clock,
            actor: Actor::Human,
            create: false,
            timeout: config.lock_timeout,
            record_undo: false,
            cancel: Cancel::new(),
            hooks: &OsHooks,
            ids: &OsIds,
        }
    }
}
