//! What an operation needs from its caller.

use std::time::Duration;

use crate::config::Config;
use crate::dates::Clock;
use crate::discover::Found;
use crate::fsio::{Hooks, OsHooks};
use crate::lock::Cancel;

use super::ids::{IdSource, OsIds};

/// Who is acting. An empty owner is the human.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actor {
    /// The `owner:` an agent writes; empty for the human.
    pub owner: String,
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
            actor: Actor::default(),
            create: false,
            timeout: config.lock_timeout,
            cancel: Cancel::new(),
            hooks: &OsHooks,
            ids: &OsIds,
        }
    }
}
