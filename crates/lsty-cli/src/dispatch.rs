//! `lsty X`: the shared steps of a command, and what runs when the first
//! argument is not a command.

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use lsty_core::config::{self, Config, Warning};
use lsty_core::dates::Clock;
use lsty_core::discover::{self, FileState, Found, FoundBy};
use lsty_core::errs::{self, Code, Error, Field as ErrField, Result};
use lsty_core::ops::{self, Actor};
use lsty_core::theme::{self, Profile, Theme};

use crate::commands::{
    self, Bare, BareHandler, Call, Command, Handler, TUI_SYNOPSIS,
};
use crate::env::{Env, Stream, TuiArgs};
use crate::help;
use crate::identity;
use crate::output::{
    self, ENVELOPE_VERSION, Field, Json, Output, human_error, json_object,
};
use crate::paint::Painter;
use crate::parse::{self, Opt, Parsed};
use crate::tty::Terminals;

/// The text of `lsty -h` and of bare `lsty` without a terminal.
pub(crate) const USAGE_TEXT: &str = "\
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
    found: Option<Found>,
    /// The theme, once a stream is coloured and a command loaded it.
    theme: Option<Theme>,
    /// The painter of errors and warnings, once it is known.
    err_painter: Option<Painter>,
}

impl<'a> Session<'a> {
    fn new(env: Env<'a>) -> Self {
        let terminals = Terminals::new(env.is_tty, env.vars);
        let (parsed, parse_error) = parse_args(&env.args);
        Self {
            env,
            terminals,
            parsed,
            parse_error,
            found: None,
            theme: None,
            err_painter: None,
        }
    }

    fn has(&self, option: Opt) -> bool {
        self.parsed.opts.has(option)
    }

    fn execute(&mut self) -> u8 {
        if self.has(Opt::Version) {
            let line = format!("lsty {}", self.env.version);
            return self.print(&line);
        }
        if self.has(Opt::Help) {
            return self.print_help();
        }
        if let Some(err) = self.parse_error.take() {
            return self.fail(&err);
        }
        self.dispatch()
    }

    /// `--help`: the usage text, or the help of the command it was given
    /// to.
    fn print_help(&mut self) -> u8 {
        let Some(command) = self.parsed.command else {
            return self.print(USAGE_TEXT);
        };
        match help::command_page(&mut self.env, command) {
            Ok(()) => 0,
            Err(e) => self.fail(&e),
        }
    }

    fn print(&mut self, line: &str) -> u8 {
        exit_of(&self.env.out(&[line.as_bytes()]))
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
        let tui = commands::TUI;
        if let Err(e) = parse::check_taken(tui, &self.parsed.opts) {
            return self.fail(&parse::with_synopsis(e, tui));
        }
        if bare && !self.terminals.can_run_tui() {
            let wrote = self.env.err(&[USAGE_TEXT.as_bytes()]);
            if wrote.is_err() {
                return errs::EXIT_IO;
            }
        }
        self.open_tui(file)
    }

    fn run_command(&mut self, command: &'static Command) -> u8 {
        let result = match command.handler {
            Handler::Tui => return self.run_tui_command(),
            Handler::Bare(handler) => self.run_bare(handler),
            Handler::Read(handler) => {
                self.run_list(command, |c, _| handler(c))
            }
            Handler::Write(handler) => {
                self.run_list(command, |c, _| handler(c).map(|w| w.output))
            }
            Handler::Tty(handler) => self
                .check_terminals(command.name)
                .and_then(|()| self.run_list(command, handler)),
        };
        match result {
            Ok(output) => self.emit(&output),
            Err(e) => self.fail(&parse::with_generic_synopsis(e, command)),
        }
    }

    fn check_terminals(&self, name: &str) -> Result<()> {
        if !self.terminals.has_terminals() {
            return Err(no_tty(&format!("{name} needs a terminal")));
        }
        Ok(())
    }

    fn run_list(
        &mut self,
        command: &'static Command,
        handler: impl FnOnce(&Call<'_>, &Env<'_>) -> Result<Output>,
    ) -> Result<Output> {
        let (config, clock, actor, found) = self.prepare(command)?;
        let paint = self.output_painter(&config, &clock);
        let mut ops = ops::Env::new(&found, &config, &clock);
        ops.create = self.has(Opt::Create);
        ops.actor = actor;
        let call = Call {
            paint,
            command,
            home: self.env.home.as_deref(),
            parsed: &self.parsed,
            ops,
        };
        handler(&call, &self.env)
    }

    fn run_bare(&mut self, handler: BareHandler) -> Result<Output> {
        handler(&mut Bare {
            env: &mut self.env,
            parsed: &self.parsed,
        })
    }

    /// The clock. A refused `LSTY_NOW` or `LSTY_TODAY` is only reported
    /// by a command that needs the time.
    fn clock(&self) -> Result<Clock> {
        Clock::new(self.env.vars, self.env.now, self.env.zone.clone())
    }

    /// The shared steps 2 to 5: config, clock, who is acting, files,
    /// missing file.
    fn prepare(
        &mut self,
        command: &Command,
    ) -> Result<(Config, Clock, Actor, Found)> {
        let config = self.load_config()?;
        let clock = self.clock()?;
        let stdin_tty = self.terminals.is_tty(Stream::Stdin);
        let actor = identity::identify(
            command,
            &self.parsed.opts,
            self.env.vars,
            stdin_tty,
        )?;
        let explicit = self.explicit_file().map(str::to_owned);
        let found = self.find(explicit.as_deref(), &config)?;
        self.found = Some(found.clone());
        self.check_file(&found)?;
        Ok((config, clock, actor, found))
    }

    fn explicit_file(&self) -> Option<&str> {
        self.parsed.opts.first(Opt::File).filter(|f| !f.is_empty())
    }

    fn read_config(&self) -> (Config, Vec<Warning>) {
        let paths =
            config::find_paths(self.env.vars, self.env.home.as_deref());
        config::load(&paths, &self.env.cwd, self.has(Opt::NoConfig))
    }

    /// The config, with its warnings and the theme's printed.
    fn load_config(&mut self) -> Result<Config> {
        let (config, mut warnings) = self.read_config();
        warnings.extend(self.set_theme(&config));
        self.print_warnings(&warnings)?;
        Ok(config)
    }

    /// The config for the TUI, which loads its own theme: only the
    /// config's warnings are printed.
    fn load_tui_config(&mut self) -> Result<Config> {
        let (config, warnings) = self.read_config();
        self.print_warnings(&warnings)?;
        Ok(config)
    }

    fn print_warnings(&mut self, warnings: &[Warning]) -> Result<()> {
        if warnings.is_empty() {
            return Ok(());
        }
        let painter = self.error_painter().clone();
        let lines: Vec<Vec<u8>> =
            warnings.iter().map(|w| painter.warning_line(w)).collect();
        self.env.err(&lines)
    }

    fn color_on(&self, stream: Stream) -> bool {
        self.terminals.color_on(self.parsed.color, stream)
    }

    fn profile(&self) -> Profile {
        let var = |name| {
            (self.env.vars)(name)
                .map(|v| errs::lossy(v.as_bytes()))
                .unwrap_or_default()
        };
        Profile::detect(&var("TERM"), &var("COLORTERM"))
    }

    fn read_theme(&self, config: &Config) -> (Theme, Vec<Warning>) {
        let paths =
            config::find_paths(self.env.vars, self.env.home.as_deref());
        theme::load(
            &config.theme,
            &paths.themes_dir,
            &self.env.cwd,
            self.has(Opt::NoConfig),
        )
    }

    /// Loads the theme once, when stdout or stderr is coloured, and
    /// returns its warnings.
    fn set_theme(&mut self, config: &Config) -> Vec<Warning> {
        let err_color = self.color_on(Stream::Stderr);
        if !err_color && !self.color_on(Stream::Stdout) {
            return Vec::new();
        }
        let (theme, warnings) = self.read_theme(config);
        if err_color {
            self.err_painter = Some(Painter::new(&theme, self.profile()));
        }
        self.theme = Some(theme);
        warnings
    }

    /// The painter for errors and warnings. When no command loaded the
    /// theme yet, the config and the theme are read again, without their
    /// warnings.
    fn error_painter(&mut self) -> &Painter {
        if self.err_painter.is_none() {
            let painter = self.build_error_painter();
            self.err_painter = Some(painter);
        }
        self.err_painter.get_or_insert_with(Painter::off)
    }

    fn build_error_painter(&self) -> Painter {
        if !self.color_on(Stream::Stderr) {
            return Painter::off();
        }
        let (config, _) = self.read_config();
        let (theme, _) = self.read_theme(&config);
        Painter::new(&theme, self.profile())
    }

    /// The painter of a command's output: on stdout, or on stderr when
    /// `--print-id` moves that output there.
    fn output_painter(&self, config: &Config, clock: &Clock) -> Painter {
        let stream = if self.has(Opt::PrintId) {
            Stream::Stderr
        } else {
            Stream::Stdout
        };
        match &self.theme {
            Some(theme) if self.color_on(stream) => {
                Painter::new(theme, self.profile())
                    .with_dates(clock.today(), config.upcoming_days)
            }
            Some(_) | None => Painter::off(),
        }
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
        let create = self.has(Opt::Create);
        match found.state {
            FileState::Exists => Ok(()),
            FileState::DirMissing if create => {
                let dir = found.todo.parent().unwrap_or_else(|| Path::new(""));
                Err(Error::new(
                    Code::Io,
                    format!("folder {} does not exist", errs::lossy_path(dir)),
                ))
            }
            FileState::Missing | FileState::DirMissing if create => Ok(()),
            FileState::Missing | FileState::DirMissing => {
                let hint = no_file_hint(found.found_by);
                Err(hint.into_iter().fold(found.no_file(), Error::with_hint))
            }
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
        let config = self.load_tui_config()?;
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
        let today = self.clock().ok().map(|c| c.today().to_string());
        vec![
            Field::int("version", i64::from(ENVELOPE_VERSION)),
            Field::new("file", file),
            Field::new("done_file", done),
            Field::opt_string("today", today.as_deref()),
        ]
    }

    fn emit(&mut self, output: &Output) -> u8 {
        let written = if self.has(Opt::Json) {
            self.write_envelope(&output.fields)
        } else {
            self.env
                .out(&output.lines)
                .and_then(|()| self.env.err(&output.notes))
        };
        exit_of(&written)
    }

    fn fail(&mut self, err: &Error) -> u8 {
        let written = if self.has(Opt::Json) {
            self.write_envelope(&[Field::new(
                "error",
                Json::object(&output::error_fields(err)),
            )])
        } else {
            let lines = human_error(self.error_painter(), err);
            self.env.err(&lines)
        };
        match written {
            Ok(()) => err.exit(),
            Err(_) => errs::EXIT_IO,
        }
    }

    fn write_envelope(&mut self, fields: &[Field]) -> Result<()> {
        let mut all = self.header();
        all.extend_from_slice(fields);
        self.env.out(&[json_object(&all).into_bytes()])
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
                errs::quote(arg.as_bytes())
            ));
            Some(parsed.command.into_iter().fold(err, parse::with_synopsis))
        }
        (None, None) => None,
    };
    (parsed, error)
}

fn looks_like_file(cwd: &Path, arg: &str) -> bool {
    arg.as_bytes().ends_with(b".txt")
        || fs::metadata(cwd.join(arg)).is_ok_and(|m| !m.is_dir())
}

pub(crate) fn unknown_command(name: &str) -> Error {
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

/// Where to look when the file this step chose is missing; walking up
/// has nothing to add.
pub(crate) const fn no_file_hint(found_by: FoundBy) -> Option<&'static str> {
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
