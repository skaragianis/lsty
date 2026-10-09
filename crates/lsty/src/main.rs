//! The `lsty` command.

use std::cell::Cell;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_cli::Env;

mod hooks;
mod tui;

/// The parts of the process that `lsty-cli` takes from its `Env`.
struct Process {
    args: Vec<OsString>,
    cwd: PathBuf,
    home: Option<PathBuf>,
    now: Timestamp,
    zone: TimeZone,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the binary is the one place that reads the process"
)]
fn read_process() -> Process {
    Process {
        args: std::env::args_os().skip(1).collect(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        home: std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from),
        now: Timestamp::now(),
        zone: TimeZone::system(),
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the binary is the one place that reads the process"
)]
pub(crate) fn var(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// A stream that remembers a write that found no reader.
struct Watched<W> {
    inner: W,
    broken: Rc<Cell<bool>>,
}

impl<W: Write> Write for Watched<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf).inspect_err(|e| self.note(e))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().inspect_err(|e| self.note(e))
    }
}

impl<W> Watched<W> {
    fn note(&self, err: &io::Error) {
        if err.kind() == io::ErrorKind::BrokenPipe {
            self.broken.set(true);
        }
    }
}

/// The exit code of a process killed by `SIGPIPE`, which a shell reports
/// for a command whose output was closed early.
const EXIT_BROKEN_PIPE: u8 = 141;

fn terminals() -> [bool; 3] {
    [
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        io::stderr().is_terminal(),
    ]
}

/// Puts the terminal back before a panic is reported: a release build
/// aborts, so the screen is never dropped.
fn give_the_terminal_back_on_panic() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        lsty_tui::restore_after_panic();
        report(info);
    }));
}

fn main() -> ExitCode {
    give_the_terminal_back_on_panic();
    let process = read_process();
    let is_tty = terminals();
    let broken = Rc::new(Cell::new(false));
    let mut stdin = io::stdin();
    let mut stdout = Watched {
        inner: io::stdout(),
        broken: Rc::clone(&broken),
    };
    let mut stderr = Watched {
        inner: io::stderr(),
        broken: Rc::clone(&broken),
    };
    let context = tui::Process {
        cwd: process.cwd.clone(),
        home: process.home.clone(),
        zone: process.zone.clone(),
        now: process.now,
    };
    let run_tui = |args| tui::run(args, &context);
    let code = lsty_cli::run(Env {
        args: process.args,
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        vars: &var,
        cwd: process.cwd,
        home: process.home,
        is_tty,
        now: process.now,
        zone: process.zone,
        version: env!("CARGO_PKG_VERSION"),
        run_editor: &hooks::run_editor,
        open_path: &hooks::start_opener,
        run_pager: &hooks::run_pager,
        run_tui: Some(&run_tui),
    });
    // A failed flush has nowhere left to be reported.
    let flushed = stdout.flush().is_ok();
    if broken.get() {
        return ExitCode::from(EXIT_BROKEN_PIPE);
    }
    if !flushed && code == 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::from(code)
}
