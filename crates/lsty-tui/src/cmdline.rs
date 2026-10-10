//! The `:` line: the commands of the screen itself, and every command
//! of the command line run as one write or shown as a read.

use lsty_cli::{Inline, run_inline};
use lsty_core::case::lower;
use lsty_core::errs::{self, Code, Error, quote};
use lsty_core::list::SortField;
use lsty_core::theme;

use crate::line::{Line, LineKind};
use crate::messages::Tone;
use crate::model::Model;
use crate::msg::Cmd;
use crate::words::split_words;

/// How a hint follows the message of an error.
const HINT_SEP: &str = " — ";

/// A command of the screen itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Own {
    Quit,
    Keep,
    Sort,
    Theme,
    Set,
    Undo,
    Redo,
}

/// The commands of the screen itself; the others run through the
/// command line parser.
const OWN: [(&str, Own); 9] = [
    ("q", Own::Quit),
    ("wq", Own::Quit),
    ("x", Own::Quit),
    ("w", Own::Keep),
    ("sort", Own::Sort),
    ("theme", Own::Theme),
    ("set", Own::Set),
    ("undo", Own::Undo),
    ("redo", Own::Redo),
];

/// The names of the commands of the screen itself.
pub(crate) fn own_names() -> impl Iterator<Item = &'static str> {
    OWN.iter().map(|(name, _)| *name)
}

impl Own {
    fn of(name: &str) -> Option<Self> {
        OWN.iter().find(|(n, _)| *n == name).map(|(_, own)| *own)
    }
}

pub(crate) fn usage(message: &str) -> Error {
    Error::new(Code::Usage, message)
}

/// The refusal of a value of the colon line's `command`.
pub(crate) fn invalid(command: &str, problem: &str) -> Error {
    Error::new(Code::InvalidValue, format!(":{command}: {problem}"))
}

impl Model {
    /// `command_line`: opens the prompt of the `:` line.
    pub(crate) fn open_command_line(&mut self) {
        self.start_line("");
    }

    fn start_line(&mut self, text: &str) {
        self.line = Some(Line::new(LineKind::Command, text));
    }

    /// Whether the open prompt is the one of the `:` line.
    pub(crate) fn on_command_line(&self) -> bool {
        self.line
            .as_ref()
            .is_some_and(|l| matches!(l.kind, LineKind::Command))
    }

    /// `execute` on the `:` line: runs what is typed. The prompt closes
    /// unless the line failed.
    pub(crate) fn execute_line(&mut self) -> Vec<Cmd> {
        let text = self
            .line
            .as_ref()
            .map(|l| l.field.text())
            .unwrap_or_default();
        let cmds = self.run_line(&text);
        self.remember_command(&text);
        cmds
    }

    fn run_line(&mut self, text: &str) -> Vec<Cmd> {
        let Some(words) = split_words(text) else {
            self.fail_line(&usage("unclosed quote"));
            return Vec::new();
        };
        let Some(first) = words.first() else {
            self.close_input();
            return Vec::new();
        };
        if let Some(own) = Own::of(&lower(first)) {
            return self.run_own(own, words.get(1..).unwrap_or(&[]), text);
        }
        self.run_cli(&words);
        Vec::new()
    }

    /// Runs a command of the screen itself.
    fn run_own(&mut self, own: Own, args: &[String], text: &str) -> Vec<Cmd> {
        let done = match own {
            Own::Quit => {
                self.close_input();
                return vec![Cmd::Quit];
            }
            Own::Keep => Ok(()),
            Own::Sort => self.sort_line(args),
            Own::Theme => self.theme_line(args),
            Own::Set => self.set_line(args),
            Own::Undo | Own::Redo => {
                self.rewind_line(own == Own::Undo, text);
                return Vec::new();
            }
        };
        match done {
            Ok(()) => {
                if self.on_command_line() {
                    self.close_input();
                }
            }
            Err(e) => self.fail_line(&e),
        }
        Vec::new()
    }

    /// Runs a command of the command line as one write, or shows what
    /// a read printed.
    fn run_cli(&mut self, words: &[String]) {
        let home = self.deps.home.clone();
        let result =
            self.attempt(|env| run_inline(env, home.as_deref(), words));
        match result {
            Err(e) => self.fail_line(&e),
            Ok((inline, clock)) => {
                match inline {
                    Inline::Wrote(outcome) => self.adopt(*outcome, clock),
                    Inline::Read(lines) => {
                        self.open_output(&words.join(" "), &lines);
                    }
                }
                self.close_input();
            }
        }
    }

    /// Shows an error above the prompt, which stays open with its text.
    fn fail_line(&mut self, e: &Error) {
        self.fail(e);
        let kept =
            matches!(e.code(), Code::LockTimeout | Code::ExpectMismatch);
        if let Some(hint) = e.hint().filter(|_| !kept) {
            self.message = Some(format!("{}{HINT_SEP}{hint}", e.message()));
        }
    }

    /// `:undo` and `:redo`: the keys' own steps. A step that is refused
    /// leaves the line open with its text.
    fn rewind_line(&mut self, undo: bool, text: &str) {
        self.close_input();
        if undo {
            self.undo_last();
        } else {
            self.redo_last();
        }
        if self.tone == Tone::Alert {
            self.start_line(text);
        }
    }

    /// `:sort [FIELD]`: orders the view; without a field, as the file.
    fn sort_line(&mut self, args: &[String]) -> errs::Result<()> {
        let field = match args {
            [] => "manual",
            [one] => one.as_str(),
            _ => return Err(usage(":sort takes one field")),
        };
        let Some(sort) = SortField::parse(field) else {
            let names = SortField::NAMES.join(", ");
            let why = format!("{} is not one of {names}", quote(field));
            return Err(invalid("sort", &why));
        };
        self.sort = sort;
        self.reshow();
        Ok(())
    }

    /// `:theme NAME`: switches the colours at once. Without a name it
    /// opens the theme picker.
    fn theme_line(&mut self, args: &[String]) -> errs::Result<()> {
        let name = match args {
            [] => {
                self.open_themes();
                return Ok(());
            }
            [name] => name,
            _ => return Err(usage(":theme takes one name")),
        };
        let (themes, cwd) = (&self.deps.themes, &self.deps.cwd);
        let no_config = self.deps.config.no_config;
        if !theme::names(themes, cwd, no_config).contains(name) {
            return Err(invalid(
                "theme",
                &format!("no theme {}", quote(name)),
            ));
        }
        self.show_theme(name);
        self.deps.config.theme.clone_from(name);
        Ok(())
    }
}
