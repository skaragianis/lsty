//! The terminal interface of lsty.
//!
//! The screen is a [`Model`]: [`Model::update`] takes a [`Msg`] and
//! returns the [`Cmd`]s that the caller carries out, and [`Model::view`]
//! draws the screen into a buffer. Nothing here reads the process. The
//! terminal comes in through [`Screen`], the files to read and the
//! warnings through [`Start`], the clock and everything else through
//! [`Deps`], so the golden-frame driver of the `harness` feature can run
//! the model without a terminal.

mod actions;
mod candidates;
mod chips;
mod cmdline;
mod cmdpool;
mod columns;
mod complete;
mod counts;
mod deps;
mod detail;
mod draw;
mod editview;
mod field;
mod filters;
mod focus;
mod grants;
mod help;
mod hints;
mod history;
mod hit;
mod insert;
mod layout;
mod line;
mod live;
mod messages;
mod model;
mod motion;
mod mouse;
mod moves;
mod msg;
mod open;
mod output;
mod panel;
mod peek;
mod picker;
mod popup;
mod prompt;
mod reldate;
mod rows;
mod run;
mod search;
mod segments;
mod session;
mod set;
mod setform;
mod setshow;
mod sidebar;
mod signals;
mod sorting;
mod status;
mod sticky;
mod store;
mod styles;
mod suspend;
mod terminal;
mod text;
mod track;
mod view;
mod visual;
mod watch;
mod words;
mod wrap;
mod yank;

#[cfg(feature = "harness")]
pub mod tuitest;

#[cfg(test)]
mod actions_tests;
#[cfg(test)]
mod cmdline_tests;
#[cfg(test)]
mod columns_tests;
#[cfg(test)]
mod complete_tests;
#[cfg(test)]
mod field_tests;
#[cfg(test)]
mod filtering_tests;
#[cfg(test)]
mod hints_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod insert_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod mouse_tests;
#[cfg(test)]
mod moving_tests;
#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod panels_tests;
#[cfg(test)]
mod pinned_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod reldate_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod signals_tests;
#[cfg(test)]
mod terminal_tests;
#[cfg(test)]
mod test_util;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod track_tests;
#[cfg(test)]
mod visual_tests;
#[cfg(test)]
mod watch_tests;
#[cfg(test)]
mod yank_tests;

pub use deps::{Deps, EditorRun, Now, Opener, Start};
pub use live::{Load, Reading};
pub use model::Model;
pub use msg::{Button, Cmd, Gesture, Mouse, Msg};
pub use run::{Feed, Input, Screen, report, run};
pub use terminal::{CrosstermScreen, restore_after_panic};
pub use watch::{FolderWatcher, Notice, Sink, Watch, Watcher, watch_folders};
