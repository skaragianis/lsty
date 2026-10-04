//! What goes into the model and what comes out of it.

use std::time::Duration;

use lsty_core::keys::Stroke;

use crate::live::{Load, Reading};
use crate::watch::Notice;

/// Something that happened.
#[derive(Debug, PartialEq, Eq)]
pub enum Msg {
    /// The terminal has a new size, in columns and rows.
    Resize(u16, u16),
    /// A key was pressed.
    Key(Stroke),
    /// Text was pasted.
    Paste(String),
    /// The clock reached a new minute.
    Minute,
    /// The debounce after a change of the files has passed.
    Reload,
    /// The watch has something to tell.
    Notice(Notice),
    /// A reading of the files is done.
    Loaded(Box<Reading>),
}

/// Something for the caller to do.
#[derive(Debug, PartialEq, Eq)]
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
    /// Read the files without holding up the keys, then hand the model
    /// the [`Msg::Loaded`] that [`Load::read`] makes.
    Load(Box<Load>),
}
