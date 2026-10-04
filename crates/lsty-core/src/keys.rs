//! Key bindings: the key notation, the actions with their default keys,
//! `keys.toml` merged over the defaults, and the matcher that turns key
//! presses into an action and a count.
//!
//! Keys and bindings are plain data; nothing here knows a terminal
//! library. A problem in `keys.toml` is never an error: it becomes a
//! [`Warning`](crate::config::Warning) and the default stays.

mod actions;
mod defaults;
mod load;
mod matcher;
mod notation;

#[cfg(test)]
mod actions_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod matcher_tests;
#[cfg(test)]
mod notation_tests;

pub use actions::{Action, Category, Kind, Mode};
pub use load::{Keymap, load};
pub use matcher::{Fed, Matcher, Outcome};
pub use notation::{FunctionKey, Key, Mods, Named, Stroke, lower, parse_keys};
