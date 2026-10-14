//! The command line of lsty: the argument parser, the command table, the
//! dispatch of `lsty X`, and how results and errors are printed.
//!
//! Nothing here reads the process: [`run`] takes everything it needs
//! through an [`Env`].
#![cfg_attr(
    test,
    expect(
        clippy::disallowed_methods,
        reason = "tests build paths of their own, which are UTF-8"
    )
)]

mod add;
mod agent_guide;
mod commands;
mod deps;
mod dispatch;
mod edit;
mod editor;
mod env;
mod expect;
mod help;
mod identity;
mod inline;
mod item_json;
mod jsonw;
mod lifecycle;
mod matrix;
mod moves;
mod mutate;
mod next;
mod open;
mod output;
mod paint;
mod parse;
mod read;
mod reflist;
mod show;
mod tty;

pub use dispatch::run;
pub use env::{Env, TuiArgs};
pub use inline::{
    Inline, inline_command, inline_commands, inline_names, inline_options,
    run_inline,
};

#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod jsonw_tests;
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
mod run_tests;
#[cfg(test)]
mod safe_tests;
#[cfg(test)]
mod tty_tests;
