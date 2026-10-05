//! The `:` line: the commands of the screen itself, and every command
//! of the command line run as one write or shown as a read.

use lsty_cli::Inline;
use lsty_core::errs::{self, Code, Error, quote};
use lsty_core::list::SortField;
use lsty_core::theme;

use crate::line::Line;
use crate::messages::Tone;
use crate::model::Model;
use crate::msg::Cmd;
use crate::styles::Styles;
use crate::words::split_words;

/// How a hint follows the message of an error.
const HINT_SEP: &str = " — ";

/// The commands of the screen itself; the others run through the
/// command line parser.
pub(crate) const OWN_COMMANDS: [&str; 9] =
    ["q", "wq", "x", "w", "sort", "theme", "set", "undo", "redo"];

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

impl Own {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "q" | "wq" | "x" => Self::Quit,
            "w" => Self::Keep,
            "sort" => Self::Sort,
            "theme" => Self::Theme,
            "set" => Self::Set,
            "undo" => Self::Undo,
            "redo" => Self::Redo,
            _ => return None,
        })
    }
}

fn usage(message: &str) -> Error {
    Error::new(Code::Usage, message)
}

impl Model {
    /// `command_line`: opens the prompt of the `:` line.
    pub(crate) fn open_command_line(&mut self) {
        self.start_line("");
    }

    fn start_line(&mut self, text: &str) {
        self.line = Some(Line::command(text));
    }

    /// Whether the open prompt is the one of the `:` line.
    pub(crate) fn on_command_line(&self) -> bool {
        self.line.as_ref().is_some_and(Line::is_command)
    }

    /// The text typed in the line that is open.
    fn typed_text(&self) -> String {
        self.line
            .as_ref()
            .map(|line| line.field().text())
            .unwrap_or_default()
    }

    /// `execute` on the `:` line: runs what is typed. The prompt closes
    /// unless the line failed.
    pub(crate) fn execute_line(&mut self) -> Vec<Cmd> {
        let text = self.typed_text();
        let cmds = self.run_line();
        self.remember_command(&text);
        cmds
    }

    fn run_line(&mut self) -> Vec<Cmd> {
        let Some(text) = self.line.as_ref().map(|l| l.field().text()) else {
            return Vec::new();
        };
        let Some(words) = split_words(&text) else {
            self.fail_line(&usage("unclosed quote"));
            return Vec::new();
        };
        let Some(first) = words.first() else {
            self.close_input();
            return Vec::new();
        };
        if let Some(own) = Own::of(&first.to_lowercase()) {
            return self.run_own(own, words.get(1..).unwrap_or(&[]), &text);
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
        let result = self
            .attempt(|env| lsty_cli::run_inline(env, home.as_deref(), words));
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
            return Err(Error::new(
                Code::InvalidValue,
                format!(
                    ":sort: {} is not one of {}",
                    quote(field),
                    SortField::NAMES.join(", ")
                ),
            ));
        };
        self.sort = sort;
        self.reshow();
        Ok(())
    }

    /// `:theme NAME`: switches the colours at once.
    fn theme_line(&mut self, args: &[String]) -> errs::Result<()> {
        let [name] = args else {
            return Err(usage(":theme takes one name"));
        };
        let (themes, cwd) = (&self.deps.themes, &self.deps.cwd);
        let no_config = self.deps.config.no_config;
        if !theme::names(themes, cwd, no_config).contains(name) {
            return Err(Error::new(
                Code::InvalidValue,
                format!(":theme: no theme {}", quote(name)),
            ));
        }
        let (theme, warnings) = theme::load(name, themes, cwd, no_config);
        self.deps.theme = theme;
        self.deps.config.theme.clone_from(name);
        self.styles = Styles::new(&self.deps.theme, self.deps.profile);
        if let Some(w) = warnings.first() {
            self.message = Some(format!("{}: {}", w.source, w.message));
        }
        Ok(())
    }
}
