//! `lsty X`: the shared steps of a command, and what runs when the first
//! argument is not a command.

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use lsty_core::config::{self, Config};
use lsty_core::dates::Clock;
use lsty_core::discover::{self, FileState, Found, FoundBy};
use lsty_core::errs::{self, Code, Error, Field as ErrField, Result};

use crate::commands::{
    self, Bare, Call, Command, Handler, Kind, TUI_SYNOPSIS,
};
use crate::env::{Env, TuiArgs};
use crate::output::{
    self, ENVELOPE_VERSION, Field, Json, Output, human_error, json_object,
    write_line, write_lines,
};
use crate::parse::{self, Parsed};
use crate::tty::Terminals;

/// The text of `lsty -h` and of bare `lsty` without a terminal.
pub const USAGE_TEXT: &str = "\
lsty (\"lusty\"): a todo list you and your agents share.

usage: lsty [FILE]                 open the list in the TUI
       lsty COMMAND [ARGS…]        run a command
       lsty help [COMMAND…]        describe the commands
       lsty help --agent           a guide for LLM agents";

/// Runs one lsty command line and returns the exit code. It never exits
/// and never reads the process except through `env`.
pub fn run(env: Env<'_>) -> u8 {
    let mut session = Session::new(env);
    session.execute()
}

struct Session<'a> {
    env: Env<'a>,
    terminals: Terminals,
    parsed: Parsed,
    parse_error: Option<Error>,
    clock: Option<Clock>,
    clock_error: Option<Error>,
    found: Option<Found>,
}

impl<'a> Session<'a> {
    fn new(env: Env<'a>) -> Self {
        let terminals = Terminals::new(env.is_tty, env.vars);
        let (parsed, parse_error) = parse_args(&env.args);
        let (clock, clock_error) =
            match Clock::new(env.vars, env.now, env.zone.clone()) {
                Ok(clock) => (Some(clock), None),
                Err(e) => (None, Some(e)),
            };
        Self {
            env,
            terminals,
            parsed,
            parse_error,
            clock,
            clock_error,
            found: None,
        }
    }

    fn has(&self, option: &str) -> bool {
        self.parsed.opts.has(option)
    }

    fn execute(&mut self) -> u8 {
        if self.has("version") {
            let line = format!("lsty {}", self.env.version);
            return self.print(&line);
        }
        if self.has("help") {
            return self.print(USAGE_TEXT);
        }
        if let Some(err) = self.parse_error.take() {
            return self.fail(&err);
        }
        self.dispatch()
    }

    fn print(&mut self, line: &str) -> u8 {
        exit_of(&write_line(self.env.stdout, line.as_bytes()))
    }

    fn dispatch(&mut self) -> u8 {
        if let Some(command) = self.parsed.command {
            return self.run_command(command);
        }
        let args = self.parsed.args.clone();
        match args.as_slice() {
            [] => self.open_checked(true, None),
            [one] if looks_like_file(&self.env.cwd, one) => {
                self.open_checked(false, Some(one))
            }
            [first, ..] => self.fail(&unknown_command(first)),
        }
    }

    fn open_checked(&mut self, bare: bool, file: Option<&str>) -> u8 {
        let Some(tui) = commands::find("tui") else {
            return self.fail(&Error::new(Code::Io, "tui is missing"));
        };
        if let Err(e) = parse::check_taken(tui, &self.parsed.opts) {
            return self.fail(&parse::with_synopsis(e, tui));
        }
        if bare && !self.terminals.can_run_tui() {
            let wrote = write_line(self.env.stderr, USAGE_TEXT.as_bytes());
            if wrote.is_err() {
                return errs::EXIT_IO;
            }
        }
        self.open_tui(file)
    }

    fn run_command(&mut self, command: &'static Command) -> u8 {
        if command.name == "tui" {
            return self.run_tui_command();
        }
        match self.setup_and_run(command) {
            Ok(output) => self.emit(&output),
            Err(e) => self.fail(&parse::with_generic_synopsis(e, command)),
        }
    }

    fn setup_and_run(&mut self, command: &'static Command) -> Result<Output> {
        if command.tty && !self.terminals.has_terminals() {
            return Err(no_tty(&format!("{} needs a terminal", command.name)));
        }
        if command.kind == Kind::Other {
            return self.run_bare(command);
        }
        let (config, clock, found) = self.prepare()?;
        let Some(Handler::List(handler)) = command.handler else {
            return Err(not_built(command));
        };
        let mut call = Call {
            command,
            create: self.has("create"),
            env: &mut self.env,
            parsed: &self.parsed,
            config,
            clock,
            found,
        };
        handler(&mut call)
    }

    fn run_bare(&mut self, command: &'static Command) -> Result<Output> {
        let Some(Handler::Bare(handler)) = command.handler else {
            return Err(not_built(command));
        };
        handler(&mut Bare {
            command,
            env: &mut self.env,
            parsed: &self.parsed,
        })
    }

    /// The shared steps 2 to 5: config, clock, files, missing file.
    fn prepare(&mut self) -> Result<(Config, Clock, Found)> {
        let config = self.load_config()?;
        let Some(clock) = self.clock.clone() else {
            return Err(self.clock_error.take().unwrap_or_else(|| {
                Error::new(Code::Io, "the clock is not available")
            }));
        };
        let explicit = self.explicit_file().map(str::to_owned);
        let found = self.find(explicit.as_deref(), &config)?;
        self.found = Some(found.clone());
        self.check_file(&found)?;
        Ok((config, clock, found))
    }

    fn explicit_file(&self) -> Option<&str> {
        self.parsed.opts.first("file").filter(|f| !f.is_empty())
    }

    fn read_config(&self) -> (Config, Vec<config::Warning>) {
        let paths =
            config::find_paths(self.env.vars, self.env.home.as_deref());
        config::load(&paths, &self.env.cwd, self.has("no-config"))
    }

    fn load_config(&mut self) -> Result<Config> {
        let (config, warnings) = self.read_config();
        let lines: Vec<Vec<u8>> = warnings
            .iter()
            .map(|w| w.to_string().into_bytes())
            .collect();
        write_lines(self.env.stderr, &lines)?;
        Ok(config)
    }

    fn find(&self, explicit: Option<&str>, config: &Config) -> Result<Found> {
        discover::find(&discover::Input {
            explicit: explicit.map(Path::new),
            vars: self.env.vars,
            cwd: &self.env.cwd,
            home: self.env.home.as_deref(),
            default_file: config.default_file.as_deref(),
        })
    }

    fn check_file(&self, found: &Found) -> Result<()> {
        let create = self.has("create");
        match found.state {
            FileState::Exists => Ok(()),
            FileState::DirMissing if create => {
                let dir = found.todo.parent().unwrap_or_else(|| Path::new(""));
                Err(Error::new(
                    Code::Io,
                    format!("folder {} does not exist", dir.display()),
                ))
            }
            FileState::Missing | FileState::DirMissing if create => Ok(()),
            FileState::Missing | FileState::DirMissing => Err(no_file(found)),
        }
    }

    fn run_tui_command(&mut self) -> u8 {
        if self.parsed.args.len() > 1 {
            let err = parse::usage("tui takes at most one file");
            let hint = format!("usage: lsty {TUI_SYNOPSIS}");
            return self.fail(&err.with_hint(hint));
        }
        let file = self.parsed.args.first().cloned();
        self.open_tui(file.as_deref())
    }

    fn open_tui(&mut self, explicit: Option<&str>) -> u8 {
        match self.try_open_tui(explicit) {
            Ok(code) => code,
            Err(e) => self.fail(&e),
        }
    }

    fn try_open_tui(&mut self, explicit: Option<&str>) -> Result<u8> {
        if !self.terminals.can_run_tui() {
            return Err(no_tty("the TUI needs a terminal"));
        }
        let Some(run_tui) = self.env.run_tui else {
            return Err(Error::new(Code::Io, "the TUI is not built yet"));
        };
        let config = self.load_config()?;
        let explicit = explicit
            .filter(|f| !f.is_empty())
            .or_else(|| self.explicit_file())
            .map(str::to_owned);
        let found = self.find(explicit.as_deref(), &config)?;
        Ok(run_tui(TuiArgs { found, config }))
    }

    fn header(&self) -> Vec<Field> {
        let path = |p: &Path| Json::bytes(p.as_os_str().as_bytes());
        let (file, done) = match &self.found {
            Some(f) => (path(&f.todo), path(&f.done)),
            None => (Json::null(), Json::null()),
        };
        let today = self.clock.as_ref().map(|c| c.today().to_string());
        vec![
            Field::int("version", i64::from(ENVELOPE_VERSION)),
            Field::new("file", file),
            Field::new("done_file", done),
            Field::opt_string("today", today.as_deref()),
        ]
    }

    fn emit(&mut self, output: &Output) -> u8 {
        let written = if self.has("json") {
            self.write_envelope(&output.fields)
        } else {
            write_lines(self.env.stdout, &output.lines)
        };
        exit_of(&written)
    }

    fn fail(&mut self, err: &Error) -> u8 {
        let written = if self.has("json") {
            self.write_envelope(&[Field::new(
                "error",
                Json::object(&output::error_fields(err)),
            )])
        } else {
            write_lines(self.env.stderr, &human_error(err))
        };
        match written {
            Ok(()) => err.exit(),
            Err(_) => errs::EXIT_IO,
        }
    }

    fn write_envelope(&mut self, fields: &[Field]) -> Result<()> {
        let mut all = self.header();
        all.extend_from_slice(fields);
        write_line(self.env.stdout, json_object(&all).as_bytes())
    }
}

fn exit_of(written: &Result<()>) -> u8 {
    match written {
        Ok(()) => 0,
        Err(_) => errs::EXIT_IO,
    }
}

/// Parses the arguments, with each invalid byte of a non-UTF-8 argument
/// read as U+FFFD, so that `--json`, `-V` and `-h` are still seen. A
/// non-UTF-8 argument is a usage error once the parse found no other.
fn parse_args(args: &[OsString]) -> (Parsed, Option<Error>) {
    let tokens: Vec<String> =
        args.iter().map(|a| errs::lossy(a.as_bytes())).collect();
    let (parsed, result) = parse::parse(&tokens);
    let bad = args.iter().find(|a| a.to_str().is_none());
    let error = match (result.err(), bad) {
        (Some(e), _) => Some(e),
        (None, Some(arg)) => {
            let err = parse::usage(format!(
                "argument {} is not valid UTF-8",
                errs::quote(&errs::lossy(arg.as_bytes()))
            ));
            Some(match parsed.command {
                Some(command) => parse::with_synopsis(err, command),
                None => err,
            })
        }
        (None, None) => None,
    };
    (parsed, error)
}

fn looks_like_file(cwd: &Path, arg: &str) -> bool {
    arg.as_bytes().ends_with(b".txt")
        || fs::metadata(cwd.join(arg)).is_ok_and(|m| !m.is_dir())
}

fn unknown_command(name: &str) -> Error {
    Error::new(
        Code::UnknownCommand,
        format!("unknown command {}", wrap_quotes(name)),
    )
    .with_hint("run \"lsty help\" for the commands")
    .with_field("command", ErrField::Str(name.to_owned()))
}

fn wrap_quotes(s: &str) -> String {
    format!("\"{s}\"")
}

fn no_tty(message: &str) -> Error {
    Error::new(Code::NoTty, message)
}

fn not_built(command: &Command) -> Error {
    Error::new(Code::Io, format!("{} is not built yet", command.name))
}

/// The `no_file` error for a todo file that is not there.
fn no_file(found: &Found) -> Error {
    let message = format!(
        "no todo file at {} (chosen by {})",
        found.todo.display(),
        found.found_by
    );
    let err = Error::new(Code::NoFile, message).with_field(
        "found_by",
        ErrField::Str(found.found_by.as_str().to_owned()),
    );
    match no_file_hint(found.found_by) {
        Some(hint) => err.with_hint(hint),
        None => err,
    }
}

/// Where the path came from, as advice.
pub fn no_file_hint(found_by: FoundBy) -> Option<&'static str> {
    match found_by {
        FoundBy::File => Some("check the path given with --file"),
        FoundBy::TodoFile => Some("check TODO_FILE"),
        FoundBy::TodoDir => Some("check TODO_DIR"),
        FoundBy::DefaultFile => Some("check default_file in config.toml"),
        FoundBy::Fallback => Some(
            "no todo.txt was found from here upwards; \
             set TODO_FILE or pass --file",
        ),
        FoundBy::WalkUp => None,
    }
}
