//! The programs the process starts for `edit`, `open` and `help`: the
//! editor, the system's opener and the pager.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use lsty_cli::spawn::{exec_failure, exit_text, lossy};
use lsty_core::program::find;

/// The program that opens a path on this system.
const fn launcher() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// An error when `program` would be found through a relative `PATH`
/// entry: the current folder is never searched.
fn refuse_relative(program: &OsStr) -> Result<(), String> {
    let path = crate::var("PATH").unwrap_or_default();
    refuse_in(&crate::cwd(), program, &path)
}

/// An error when `program` is found first in a relative folder of
/// `path`.
fn refuse_in(cwd: &Path, program: &OsStr, path: &OsStr) -> Result<(), String> {
    match find(cwd, path, program).refusal(program) {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

/// Runs the editor `words` (the program, its arguments and the file) and
/// waits for it. A non-zero status is an error.
pub(crate) fn run_editor(words: &[OsString]) -> Result<(), String> {
    let name = words
        .first()
        .map_or_else(String::new, |program| lossy(program.as_bytes()));
    edit_on_terminal(words)
        .map_err(|why| failed(&format!("editor {name}"), &why))
}

/// Runs the editor `words` on the terminal and waits for it. The error
/// is why it failed, without the name of the program.
pub(crate) fn edit_on_terminal(words: &[OsString]) -> Result<(), String> {
    let Some((program, args)) = words.split_first() else {
        return Err("no editor".to_owned());
    };
    refuse_relative(program)?;
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| exec_failure(program, e))?;
    if status.success() {
        return Ok(());
    }
    Err(exit_text(status))
}

/// Starts the system's opener for `path` and does not wait for it.
pub(crate) fn start_opener(path: &Path) -> Result<(), String> {
    open_reason(path).map_err(|why| {
        let name = lossy(path.as_os_str().as_bytes());
        failed(&format!("open {name}"), &why)
    })
}

/// Starts the system's opener for `path` and does not wait for it. The
/// error is why it could not be started.
pub(crate) fn open_reason(path: &Path) -> Result<(), String> {
    let program = launcher();
    refuse_relative(OsStr::new(program))?;
    Command::new(program)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| exec_failure(OsStr::new(program), e))
}

/// Shows `text` in the pager `words`. An error means the pager could not
/// be started; quitting it early is no failure.
pub(crate) fn run_pager(words: &[OsString], text: &[u8]) -> io::Result<()> {
    let Some((program, args)) = words.split_first() else {
        return Err(io::Error::other("no pager"));
    };
    refuse_relative(program).map_err(io::Error::other)?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| io::Error::other(exec_failure(program, e)))?;
    if let Some(mut stdin) = child.stdin.take() {
        // The reader may quit before it has read everything.
        let _quit_early = stdin.write_all(text);
    }
    let _ended = child.wait();
    Ok(())
}

fn failed(what: &str, why: &str) -> String {
    format!("{what}: {why}")
}

#[cfg(test)]
mod tests;
