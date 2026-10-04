//! Starts the terminal interface with the process's terminal, clock and
//! environment.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_cli::TuiArgs;
use lsty_core::config;
use lsty_core::dates::Clock;
use lsty_core::errs::{Error, lossy};
use lsty_core::keys;
use lsty_core::theme::{self, Profile};
use lsty_tui::{CrosstermScreen, Deps, Now};
use std::os::unix::ffi::OsStrExt as _;

/// What the terminal interface takes from the process.
pub(crate) struct Process {
    pub(crate) cwd: PathBuf,
    pub(crate) home: Option<PathBuf>,
    pub(crate) zone: TimeZone,
    pub(crate) now: Timestamp,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the binary is the one place that reads the process"
)]
fn real_now() -> Timestamp {
    Timestamp::now()
}

fn var(name: &str) -> Option<OsString> {
    crate::var(name)
}

fn text(name: &str) -> String {
    var(name).map(|v| lossy(v.as_bytes())).unwrap_or_default()
}

/// The time of the moment: the real one, or the fixed one that
/// `LSTY_NOW` or `LSTY_TODAY` asks for.
fn clock(process: &Process) -> Result<Now, Error> {
    let clock = Clock::new(&var, process.now, process.zone.clone())?;
    let fixed = ["LSTY_NOW", "LSTY_TODAY"]
        .iter()
        .any(|name| var(name).is_some_and(|v| !v.is_empty()));
    if fixed {
        let at = clock.now();
        Ok(Box::new(move || at))
    } else {
        Ok(Box::new(real_now))
    }
}

fn deps(args: TuiArgs, process: &Process) -> Result<Deps, Error> {
    let TuiArgs { found, config } = args;
    let paths = config::find_paths(&var, process.home.as_deref());
    let (keymap, mut warnings) = keys::load(
        &paths.keys_file,
        &process.cwd,
        config.no_config,
        &config.leader,
    );
    let (theme, more) = theme::load(
        &config.theme,
        &paths.themes_dir,
        &process.cwd,
        config.no_config,
    );
    warnings.extend(more);
    Ok(Deps {
        found,
        config,
        keys: keymap,
        theme,
        profile: Profile::detect(&text("TERM"), &text("COLORTERM")),
        warnings,
        now: clock(process)?,
        zone: process.zone.clone(),
    })
}

/// Runs the terminal interface and returns its exit code.
pub(crate) fn run(args: TuiArgs, process: &Process) -> u8 {
    let deps = match deps(args, process) {
        Ok(deps) => deps,
        Err(e) => return lsty_tui::report(&e, &mut io::stderr()),
    };
    let open = || CrosstermScreen::open(io::stdout());
    lsty_tui::run(deps, open, &mut io::stderr())
}
