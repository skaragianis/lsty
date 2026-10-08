//! The clock, the fuzzy-date grammar and date arithmetic.
//!
//! A [`Clock`] is one reading of time for one action. [`resolve`] turns
//! `fri`, `+3d` or `tomorrow` into a [`Date`], which also does the
//! arithmetic recurrence uses.

mod clock;
mod date;
mod duration;
mod fuzzy;

pub use clock::Clock;
pub use date::Date;
pub use duration::{format_elapsed, parse_duration};
pub(crate) use fuzzy::Fuzzy;
pub use fuzzy::{Resolved, resolve};

#[cfg(test)]
mod clock_tests;
#[cfg(test)]
mod date_tests;
#[cfg(test)]
mod duration_tests;
#[cfg(test)]
mod fuzzy_tests;
