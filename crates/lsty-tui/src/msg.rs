//! What goes into the model and what comes out of it.

use std::time::Duration;

use lsty_core::keys::Stroke;

/// Something that happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    /// The terminal has a new size, in columns and rows.
    Resize(u16, u16),
    /// A key was pressed.
    Key(Stroke),
    /// The clock reached a new minute.
    Minute,
}

/// Something for the caller to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// End the program with exit code 0.
    Quit,
    /// Hand the model `msg` after `delay`.
    After {
        /// How long to wait.
        delay: Duration,
        /// What to hand back.
        msg: Msg,
    },
}
