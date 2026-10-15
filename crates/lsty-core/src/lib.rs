//! The todo list model, file I/O and operations of lsty.
//!
//! Modules are listed in layer order, top first. A module may use only
//! modules above it; `tests/layering.rs` checks this.
#![cfg_attr(
    test,
    expect(
        clippy::disallowed_methods,
        reason = "tests build paths of their own, which are UTF-8"
    )
)]

use std::ffi::OsString;

/// Looks up an environment variable; the caller supplies it, nothing below
/// the binary reads the process's environment.
pub type Vars<'a> = dyn Fn(&str) -> Option<OsString> + 'a;

pub mod visible;

pub mod errs;

pub mod case;

pub mod todotxt;

pub mod dates;

pub mod fsio;

pub mod lock;

pub mod todosh;

pub mod discover;

pub mod config;

pub mod list;

pub mod query;

pub mod lineid;

pub mod links;

pub mod program;

pub mod clip;

pub mod ops;

pub mod keys;

pub mod theme;
