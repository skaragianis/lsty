//! Starts the terminal interface with the process's terminal, clock and
//! environment.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_cli::TuiArgs;
use lsty_core::clip::{Clipboard, SystemRunner};
use lsty_core::config;
use lsty_core::dates::Clock;
use lsty_core::errs::{Error, lossy};
use lsty_core::keys;
use lsty_core::ops::OsIds;
use lsty_core::program::find_in_path;
use lsty_core::theme::{self, Profile};
use lsty_tui::{CrosstermScreen, Deps, Now, Start};
use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::flag;

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
    if clock.is_fixed() {
        let at = clock.now();
        Ok(Box::new(move || at))
    } else {
        Ok(Box::new(real_now))
    }
}

/// The clipboard reader: `paste_command`, or the first known program
/// found in `PATH`.
fn clipboard(
    paste_command: Option<&str>,
    cwd: &Path,
) -> Clipboard<SystemRunner> {
    let path = var("PATH").unwrap_or_default();
    let find = |name: &str| find_in_path(cwd, &path, name);
    Clipboard::system(paste_command.unwrap_or_default(), &find)
}

fn deps(args: TuiArgs, process: &Process) -> Result<(Start, Deps), Error> {
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
    let clipboard =
        Box::new(clipboard(config.paste_command.as_deref(), &process.cwd));
    let deps = Deps {
        config,
        keys: keymap,
        theme,
        profile: Profile::detect(&text("TERM"), &text("COLORTERM")),
        ids: Box::new(OsIds),
        clipboard,
        now: clock(process)?,
        zone: process.zone.clone(),
        home: process.home.clone(),
        themes: paths.themes_dir,
        config_file: Some(paths.config_file),
        cwd: process.cwd.clone(),
        editor: var("EDITOR"),
        opener: Box::new(crate::hooks::open_reason),
        run_editor: Box::new(crate::hooks::edit_on_terminal),
        state_dir: Some(paths.state_dir),
        watch: Some(Box::new(lsty_tui::watch_folders)),
    };
    Ok((Start { found, warnings }, deps))
}

/// Runs the terminal interface and returns its exit code.
pub(crate) fn run(args: TuiArgs, process: &Process) -> u8 {
    let (start, deps) = match deps(args, process) {
        Ok(both) => both,
        Err(e) => return lsty_tui::report(&e, &mut io::stderr()),
    };
    let after = Arc::new(AtomicBool::new(true));
    end_the_process_by_signals_once(&after);
    // The first reading of the list comes before the screen, and a signal
    // then ends the process; only the open screen catches them.
    let open = || {
        after.store(false, Ordering::Relaxed);
        CrosstermScreen::open()
    };
    let code = lsty_tui::run(start, deps, open, &mut io::stderr());
    after.store(true, Ordering::Relaxed);
    code
}

/// The screen catches SIGHUP and SIGTERM while it is open, and the
/// handlers it leaves behind would swallow them for good. While `after`
/// is set they end the process as they do by default.
fn end_the_process_by_signals_once(after: &Arc<AtomicBool>) {
    for signal in [SIGHUP, SIGTERM] {
        // Without it a signal after the screen is ignored; nothing else
        // can be done about a failure.
        drop(flag::register_conditional_default(
            signal,
            Arc::clone(after),
        ));
    }
}
