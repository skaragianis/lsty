//! What a watch on the todo and done files reports, for the program
//! that supplies one.

use std::sync::Arc;

use crate::discover::Found;

/// What a watcher reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Something happened to one of the two files.
    Changed,
    /// The system's watch failed; the reason is its own text.
    Failed(String),
}

/// Where a watcher hands its notices. It is called from the system's
/// thread and must not block.
pub type Sink = Arc<dyn Fn(Notice) + Send + Sync>;

/// A watch on the two files.
pub trait Watcher {
    /// Adds the folders again, so that one deleted and made again, or one
    /// that appeared after the start, is watched from then on.
    ///
    /// # Errors
    ///
    /// The reason a folder cannot be watched.
    fn rewatch(&mut self) -> Result<(), String>;
}

/// Starts a watch on the files of `found`, handing its notices to the
/// sink.
pub type WatchFn<'a> =
    dyn Fn(&Found, Sink) -> Result<Box<dyn Watcher>, String> + 'a;

/// A watch starter the program owns.
pub type Watch = Box<WatchFn<'static>>;
