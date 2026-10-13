//! The programs the process starts for `edit`, `open` and `help`: the
//! editor, the system's opener and the pager.

use std::{
    ffi::{OsStr, OsString},
    io::Write as _,
    os::unix::{ffi::OsStrExt as _, process::CommandExt as _},
    path::Path,
    process::{Command, Stdio},
};

use lsty_core::{
    errs::{lossy, lossy_path},
    links::HookResult,
    program::{self, exec_failure, exit_text},
};

/// The program that opens a path on this system.
const fn launcher() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// The command that starts `program`. A program found on `PATH` runs
/// from the file found, so no later search can pick another; one found
/// through a relative entry is an error: the current folder is never
/// searched.
fn command(program: &OsStr) -> Result<Command, String> {
    command_on(program, &crate::var("PATH").unwrap_or_default())
}

/// [`command`] with `path` as the search path.
fn command_on(program: &OsStr, path: &OsStr) -> Result<Command, String> {
    let mut command =
        Command::new(program::lookup(Path::new(""), path, program)?);
    command.arg0(program);
    Ok(command)
}

/// Runs the editor `words` (the program, its arguments and the file) and
/// waits for it. A non-zero status is an error.
pub(crate) fn run_editor(words: &[OsString]) -> HookResult {
    let name = words
        .first()
        .map_or_else(String::new, |program| lossy(program.as_bytes()));
    edit_on_terminal(words).map_err(|why| format!("editor {name}: {why}"))
}

/// Runs the editor `words` on the terminal and waits for it. The error
/// is why it failed, without the name of the program.
pub(crate) fn edit_on_terminal(words: &[OsString]) -> HookResult {
    let Some((program, args)) = words.split_first() else {
        return Err("no editor".to_owned());
    };
    let status = command(program)?
        .args(args)
        .status()
        .map_err(|e| exec_failure(program, &e))?;
    if status.success() {
        return Ok(());
    }
    Err(exit_text(status))
}

/// Starts the system's opener for `path` and does not wait for it.
pub(crate) fn start_opener(path: &Path) -> HookResult {
    open_reason(path)
        .map_err(|why| format!("open {}: {why}", lossy_path(path)))
}

/// Starts the system's opener for `path` and does not wait for it. The
/// error is why it could not be started.
pub(crate) fn open_reason(path: &Path) -> HookResult {
    let program = launcher();
    if !can_show(program, crate::var) {
        return Err("no display".to_owned());
    }
    command(OsStr::new(program))?
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| exec_failure(OsStr::new(program), &e))
}

/// Whether `program` can show something: `xdg-open` with no display and
/// no `BROWSER` hands the path to a text browser that has no terminal.
fn can_show(program: &str, var: impl Fn(&str) -> Option<OsString>) -> bool {
    program != "xdg-open"
        || ["DISPLAY", "WAYLAND_DISPLAY", "BROWSER"]
            .iter()
            .any(|name| var(name).is_some_and(|v| !v.is_empty()))
}

/// Shows `text` in the pager `words`. An error means the pager could not
/// be started; quitting it early is no failure.
pub(crate) fn run_pager(words: &[OsString], text: &[u8]) -> HookResult {
    let Some((program, args)) = words.split_first() else {
        return Err("no pager".to_owned());
    };
    let mut child = command(program)?
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| exec_failure(program, &e))?;
    if let Some(mut stdin) = child.stdin.take() {
        // The reader may quit before it has read everything.
        drop(stdin.write_all(text));
    }
    drop(child.wait());
    Ok(())
}

#[cfg(test)]
mod tests;
