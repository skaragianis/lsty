//! Following a line from one reading of a file to the next.
//!
//! A line diff maps the lines that stayed. A line that moved follows
//! only when its text is unique on both sides, and every other line
//! counts as changed. Lines are compared as bytes, without their line
//! ending. Both functions are pure: the caller passes the lines it read
//! under its own lock.
//!
//! Callers keep a line number or an id across reloads and ask here where
//! it went; they never hold a task of an old snapshot.

mod diff;
#[cfg(any(test, feature = "fuzzing"))]
pub mod fuzzing;
mod remap;

pub use remap::{follow, remap};

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod remap_tests;
