//! Which terminal features are on.

use lsty_core::errs::Vars;

use crate::env::Stream;

/// How `--color` was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorMode {
    /// Colour when the stream is a terminal and the environment allows it.
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

impl ColorMode {
    /// The spellings `--color` accepts.
    pub(crate) const NAMES: [&'static str; 3] = ["auto", "always", "never"];

    /// The mode a `--color` value names.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

/// The terminals of one run, after `LSTY_NONINTERACTIVE` and `TERM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Terminals {
    is_tty: [bool; 3],
    dumb: bool,
    no_color: bool,
}

impl Terminals {
    /// Reads `LSTY_NONINTERACTIVE`, `TERM` and `NO_COLOR` through `vars`.
    /// `LSTY_NONINTERACTIVE=1` makes every stream a non-terminal.
    pub(crate) fn new(is_tty: [bool; 3], vars: &Vars<'_>) -> Self {
        let non_interactive =
            vars("LSTY_NONINTERACTIVE").is_some_and(|v| v == "1");
        Self {
            is_tty: if non_interactive { [false; 3] } else { is_tty },
            dumb: vars("TERM").is_some_and(|v| v == "dumb"),
            no_color: vars("NO_COLOR").is_some_and(|v| !v.is_empty()),
        }
    }

    /// Whether `stream` is a terminal.
    pub(crate) const fn is_tty(self, stream: Stream) -> bool {
        let [stdin, stdout, stderr] = self.is_tty;
        match stream {
            Stream::Stdin => stdin,
            Stream::Stdout => stdout,
            Stream::Stderr => stderr,
        }
    }

    /// Whether the TUI can run: terminals in and out, and no `TERM=dumb`.
    pub(crate) fn can_run_tui(self) -> bool {
        self.has_terminals() && !self.dumb
    }

    /// Whether stdin and stdout are terminals, which `edit` and `open`
    /// need.
    pub(crate) fn has_terminals(self) -> bool {
        self.is_tty(Stream::Stdin) && self.is_tty(Stream::Stdout)
    }

    /// Whether the help is paged.
    pub(crate) fn pages_help(self) -> bool {
        self.is_tty(Stream::Stdout) && !self.dumb
    }

    /// Whether output to `stream` is coloured.
    pub(crate) fn color_on(self, mode: ColorMode, stream: Stream) -> bool {
        match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                self.is_tty(stream) && !self.dumb && !self.no_color
            }
        }
    }
}
