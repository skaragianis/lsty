//! What goes into the model and what comes out of it.

use std::{ffi::OsString, time::Duration};

use lsty_core::{keys::Stroke, links::HookResult};
use ratatui::layout::Position;

use crate::{
    live::{Load, Reading},
    watch::Notice,
};

/// A button of the mouse; only the left one does anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    /// The left button.
    Left,
    /// Any other button.
    Other,
}

/// What the mouse did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gesture {
    /// A button went down.
    Down(Button),
    /// A button came up.
    Up,
    /// The mouse moved with a button held.
    Drag(Button),
    /// The wheel turned one notch, up when `true`.
    Wheel {
        /// Whether the notch was away from the user.
        up: bool,
    },
}

/// A mouse event at a cell, counted from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mouse {
    /// What happened.
    pub(crate) gesture: Gesture,
    /// The cell.
    pub(crate) at: Position,
    /// Whether Shift was held.
    pub(crate) shift: bool,
}

/// Something that happened.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Msg {
    /// The terminal has a new size, in columns and rows.
    Resize(u16, u16),
    /// A key was pressed.
    Key(Stroke),
    /// Text was pasted.
    Paste(String),
    /// The mouse did something.
    Mouse(Mouse),
    /// The terminal gained or lost focus.
    Focus(bool),
    /// The clock reached a new minute.
    Minute,
    /// The message that the count names has been up long enough.
    Expire(u64),
    /// The panel step that the count names was the last one for a while.
    Refilter(u64),
    /// The debounce after a change of the files has passed.
    Reload,
    /// The watch has something to tell.
    Notice(Notice),
    /// A reading of the files is done.
    Loaded(Box<Reading>),
    /// The editor that [`Cmd::Edit`] asked for has ended; the error says
    /// how it failed.
    Edited(HookResult),
}

/// Something for the caller to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Cmd {
    /// End the program with exit code 0.
    Quit,
    /// Hand the model `msg` after `delay`.
    After {
        /// How long to wait.
        delay: Duration,
        /// What to hand back.
        msg: Msg,
    },
    /// Put the text on the system clipboard, as the terminal takes it.
    Copy(Vec<u8>),
    /// Hand the terminal to the editor, given as the program, its
    /// arguments and the file, and wait for it; then hand the model
    /// [`Msg::Edited`].
    Edit(Vec<OsString>),
    /// Read the files without holding up the keys, then hand the model
    /// the [`Msg::Loaded`] that [`Load::read`] makes.
    Load(Box<Load>),
}
