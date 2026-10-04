//! Everything the model takes from the outside.

use std::path::PathBuf;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::config::{Config, Warning};
use lsty_core::discover::Found;
use lsty_core::keys::Keymap;
use lsty_core::theme::{Profile, Theme};

/// Reads the clock: the time of the moment it is called.
pub type Now = Box<dyn Fn() -> Timestamp>;

/// What the model is made of. The model reads nothing itself: not the
/// clock, not the environment, not the terminal.
pub struct Deps {
    /// The todo and done files.
    pub found: Found,
    /// The options.
    pub config: Config,
    /// The key bindings.
    pub keys: Keymap,
    /// The colours.
    pub theme: Theme,
    /// How many colours the terminal shows.
    pub profile: Profile,
    /// Problems found in the config, the keys and the theme; shown one
    /// after the other, each until a key is pressed.
    pub warnings: Vec<Warning>,
    /// The clock.
    pub now: Now,
    /// The local time zone.
    pub zone: TimeZone,
    /// The home folder, where a `file:` link that starts with `~` points.
    pub home: Option<PathBuf>,
}
