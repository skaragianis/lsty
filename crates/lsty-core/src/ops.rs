//! What commands do with a list. For now: naming one item by a
//! reference such as `12` or `done:12`.

mod refs;

pub use refs::{Target, parse_one, resolve};

#[cfg(test)]
mod refs_tests;
