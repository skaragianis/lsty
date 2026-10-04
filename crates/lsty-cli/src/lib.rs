//! The command line of lsty: the argument parser, the command table, the
//! dispatch of `lsty X`, and how results and errors are printed.
//!
//! Nothing here reads the process: [`run`] takes everything it needs
//! through an [`Env`].

pub mod add;
mod agent_guide;
pub mod commands;
pub mod deps;
pub mod dispatch;
pub mod edit;
pub mod editor;
pub mod env;
pub mod expect;
pub mod help;
pub mod identity;
pub mod item_json;
pub mod lifecycle;
mod linkfiles;
pub mod matrix;
pub mod moves;
pub mod mutate;
pub mod next;
pub mod open;
pub mod output;
pub mod paint;
pub mod parse;
pub mod read;
pub mod reflist;
pub mod show;
pub mod spawn;
pub mod tty;

pub use dispatch::run;
pub use env::{Env, TuiArgs};
pub use spawn::{editor_command, exec_failure, exit_text};

#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod mutate_tests;
#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tty_tests;
