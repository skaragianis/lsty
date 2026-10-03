//! The options of `config.toml`, where the file is, and how it is read.
//!
//! A config problem is never an error: the bad entry is ignored, its
//! default is used and a [`Warning`] says so. Nothing here reads the
//! process: the caller passes the variables and the home folder.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::errs::quote;
use crate::todotxt::Priority;

mod load;
mod options;
mod paths;

#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod paths_tests;

pub use load::load;
pub(crate) use load::{Table, range_error, syntax_message, utf8_text};
pub use options::{is_option, set};
pub use paths::{Paths, find_paths};

/// The name of the file every warning of this module belongs to.
const SOURCE: &str = "config.toml";

/// How line numbers are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineNumbers {
    /// The current line absolute, the others relative.
    Hybrid,
    /// Every line absolute.
    Absolute,
    /// Every line relative to the current one.
    Relative,
    /// No numbers.
    Off,
}

impl LineNumbers {
    /// The accepted spellings, in the order warnings list them.
    pub const NAMES: [&'static str; 4] =
        ["hybrid", "absolute", "relative", "off"];

    /// The spelling in `config.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Absolute => "absolute",
            Self::Relative => "relative",
            Self::Off => "off",
        }
    }
}

/// A kind of item the list can hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hidden {
    /// Items with a threshold date in the future.
    Later,
    /// Items with open blockers.
    Blocked,
    /// Completed items.
    Done,
    /// Items in the done file.
    Archive,
}

impl Hidden {
    /// The accepted spellings, in the order warnings list them.
    pub const NAMES: [&'static str; 4] =
        ["later", "blocked", "done", "archive"];

    /// The spelling in `config.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Later => "later",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Archive => "archive",
        }
    }
}

/// A saved filter: a name and its query string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    /// The name, as listed in `[filters]`.
    pub name: String,
    /// The query string; not validated here.
    pub query: String,
}

/// Every option of `config.toml`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per option, as the file has them"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `theme`: a theme name; checked where themes are loaded.
    pub theme: String,
    /// `leader`: one key in key notation; checked where keys are read.
    pub leader: String,
    /// `include_creation`: write a creation date on new items.
    pub include_creation: bool,
    /// `auto_archive`: move completed items to the done file.
    pub auto_archive: bool,
    /// `keep_priority`: keep the priority of a completed item.
    pub keep_priority: bool,
    /// `default_priority`: the priority of new items, if any (`A`-`Z`).
    pub default_priority: Option<Priority>,
    /// `priority_range`: the last letter of the priority range (`A`-`Z`).
    pub priority_range: Priority,
    /// `upcoming_days`: 1 or more.
    pub upcoming_days: u64,
    /// `hidden_items`: without repeats, in the order given.
    pub hidden_items: Vec<Hidden>,
    /// `show_sidebar`.
    pub show_sidebar: bool,
    /// `sidebar_min_width`.
    pub sidebar_min_width: u64,
    /// `show_detail`.
    pub show_detail: bool,
    /// `detail_min_width`.
    pub detail_min_width: u64,
    /// `line_numbers`.
    pub line_numbers: LineNumbers,
    /// `lock_timeout`: more than zero.
    pub lock_timeout: Duration,
    /// `claim_stale_after`: more than zero.
    pub claim_stale_after: Duration,
    /// `start_filter`; `None` if unset or empty.
    pub start_filter: Option<String>,
    /// `paste_command`; `None` if unset or empty.
    pub paste_command: Option<String>,
    /// `default_file`; `None` if unset or empty.
    pub default_file: Option<String>,
    /// `[hints]`: mode name to action names; checked where hints are used.
    pub hints: BTreeMap<String, Vec<String>>,
    /// `[filters]`, in the order the file lists them.
    pub filters: Vec<Filter>,
    /// Set when `--no-config` left the user's files unread.
    pub no_config: bool,
}

impl Default for Config {
    /// The built-in value of every option.
    fn default() -> Self {
        Self {
            theme: "catppuccin-mocha".to_owned(),
            leader: "<Space>".to_owned(),
            include_creation: true,
            auto_archive: false,
            keep_priority: true,
            default_priority: None,
            priority_range: Priority::D,
            upcoming_days: 7,
            hidden_items: vec![Hidden::Later, Hidden::Archive],
            show_sidebar: true,
            sidebar_min_width: 80,
            show_detail: true,
            detail_min_width: 120,
            line_numbers: LineNumbers::Hybrid,
            lock_timeout: Duration::from_secs(2),
            claim_stale_after: Duration::from_hours(4),
            start_filter: None,
            paste_command: None,
            default_file: None,
            hints: BTreeMap::new(),
            filters: Vec::new(),
            no_config: false,
        }
    }
}

/// A value as `config.toml` or `:set` gives it to an option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// `true` or `false`.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A string.
    Str(String),
    /// A list of strings.
    List(Vec<String>),
    /// A value of a kind no option accepts: a float, a date, a table, a
    /// list holding anything but strings.
    Other,
}

/// A config problem that was ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// The file the warning is about.
    pub source: String,
    /// What is wrong and what was done about it.
    pub message: String,
}

impl Warning {
    fn new(message: String) -> Self {
        Self {
            source: SOURCE.to_owned(),
            message,
        }
    }
}

impl fmt::Display for Warning {
    /// The line the CLI prints on stderr:
    /// `lsty: warning: config.toml: auto_archive: expected bool`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lsty: warning: {}: {}", self.source, self.message)
    }
}
