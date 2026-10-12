//! Starts the terminal interface with the process's terminal, clock and
//! environment.

use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt as _, path::Path};

use jiff::Timestamp;
use lsty_cli::TuiArgs;
use lsty_core::{
    clip::{Clipboard, OsRunner},
    config::{self, Config},
    dates::Clock,
    errs::{Error, lossy},
    keys,
    ops::OsIds,
    program::lookup,
    theme::{self, Profile},
};
use lsty_tui::{CrosstermScreen, Deps, Now, Start, report, watch_folders};

use crate::{Process, var};

#[expect(
    clippy::disallowed_methods,
    reason = "the binary is the one place that reads the process"
)]
fn real_now() -> Timestamp {
    Timestamp::now()
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

/// The clipboard: `paste_command` and `copy_command`, or the first
/// known programs found in `PATH`, and the tmux buffer inside tmux.
fn clipboard(config: &Config) -> Clipboard<OsRunner> {
    let path = var("PATH").unwrap_or_default();
    let find = |name: &OsStr| lookup(Path::new(""), &path, name);
    let tmux = var("TMUX").is_some_and(|t| !t.is_empty());
    let paste = config.paste_command.as_deref().unwrap_or_default();
    let copy = config.copy_command.as_deref().unwrap_or_default();
    Clipboard::with(OsRunner, paste, copy, tmux, &find)
}

fn inputs(args: TuiArgs, process: &Process) -> Result<(Start, Deps), Error> {
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
    let clipboard = Box::new(clipboard(&config));
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
    };
    Ok((
        Start {
            found,
            warnings,
            watch: Some(Box::new(watch_folders)),
            state_dir: Some(paths.state_dir),
        },
        deps,
    ))
}

/// Runs the terminal interface and returns its exit code.
pub(crate) fn run(args: TuiArgs, process: &Process) -> u8 {
    let (start, deps) = match inputs(args, process) {
        Ok(pair) => pair,
        Err(e) => return report(&e, &mut io::stderr()),
    };
    lsty_tui::run(start, deps, CrosstermScreen::open, &mut io::stderr())
}
