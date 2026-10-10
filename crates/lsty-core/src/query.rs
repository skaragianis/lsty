//! The filter language: a parser and an evaluator.
//!
//! One language serves saved filters, the built-in narrowing filters,
//! `--filter` and the `TERM` arguments of the list commands. Terms are
//! combined with "and", `|` gives alternatives inside a term, and a
//! leading `-` negates the whole term. [`Query::parse`] and
//! [`Query::parse_args`] build a query, and [`Query::matches`] tests an
//! item of a snapshot against it.
//! Dates in a query are resolved when it is evaluated, against the
//! [`Context`] it is given.

mod atom;
mod builtin;
mod eval;
mod model;
mod parse;
mod word;

pub use builtin::DOING;
pub use model::{Category, Context, Query};
pub(crate) use word::contains;
pub use word::is_upper;

#[cfg(test)]
mod builtin_tests;
#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod test_util;
