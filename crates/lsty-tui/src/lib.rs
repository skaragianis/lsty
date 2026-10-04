//! The terminal interface of lsty.
//!
//! The screen is a [`Model`]: [`Model::update`] takes a [`Msg`] and
//! returns the [`Cmd`]s that the caller carries out, and [`Model::view`]
//! draws the screen into a buffer. Nothing here reads the process. The
//! terminal comes in through [`Screen`], the clock and everything else
//! through [`Deps`], so the golden-frame driver of the `harness` feature
//! can run the model without a terminal.

mod chips;
mod columns;
mod deps;
mod detail;
mod draw;
mod field;
mod filters;
mod focus;
mod hints;
mod layout;
mod model;
mod motion;
mod msg;
mod panel;
mod reldate;
mod rows;
mod run;
mod search;
mod sidebar;
mod signals;
mod status;
mod styles;
mod terminal;
mod text;
mod view;
mod watch;

#[cfg(feature = "harness")]
pub mod tuitest;

#[cfg(test)]
mod columns_tests;
#[cfg(test)]
mod filtering_tests;
#[cfg(test)]
mod hints_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod panels_tests;
#[cfg(test)]
mod reldate_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod signals_tests;
#[cfg(test)]
mod terminal_tests;
#[cfg(test)]
mod test_util;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod watch_tests;

pub use deps::{Deps, Now};
pub use model::Model;
pub use msg::{Cmd, Msg};
pub use run::{Input, Screen, report, run};
pub use terminal::CrosstermScreen;
pub use watch::{FolderWatcher, Notice, Sink, Watch, Watcher, watch_folders};
