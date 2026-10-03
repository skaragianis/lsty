//! The command line of lsty: the argument parser, the command table, the
//! dispatch of `lsty X`, and how results and errors are printed.
//!
//! Nothing here reads the process: [`run`] takes everything it needs
//! through an [`Env`].

pub mod commands;
pub mod dispatch;
pub mod edit;
pub mod env;
pub mod expect;
pub mod identity;
pub mod item_json;
pub mod mutate;
pub mod output;
pub mod parse;
pub mod read;
pub mod show;
pub mod tty;

pub use dispatch::run;
pub use env::{Env, TuiArgs};

#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tty_tests;
