//! The programs the process starts for `edit`, `open` and `help`: the
//! editor, the system's opener and the pager.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use lsty_cli::spawn::{exec_failure, exit_text, lossy};

/// The program that opens a path on this system.
const fn launcher() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Whether `program`, a bare name, would be found first in a folder of
/// `path` that is not absolute (an empty entry is the current folder).
/// Such a program is refused rather than run; this is the refusal's
/// text.
fn found_in_relative_entry(program: &OsStr, path: &OsStr) -> Option<String> {
    use std::os::unix::fs::PermissionsExt as _;
    if program.is_empty() || program.as_bytes().contains(&b'/') {
        return None;
    }
    let found = |dir: &Path| {
        std::fs::metadata(dir.join(program))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let hit = path
        .as_bytes()
        .split(|&b| b == b':')
        .map(|entry| if entry.is_empty() { b"." } else { entry })
        .map(|entry| Path::new(OsStr::from_bytes(entry)))
        .find(|dir| found(dir))?;
    if hit.is_absolute() {
        return None;
    }
    let name = lsty_core::errs::quote_bytes(program.as_bytes());
    Some(format!(
        "exec: {name}: cannot run executable found relative to current \
         directory"
    ))
}

/// An error when `program` would be found through a relative `PATH`
/// entry: the current folder is never searched.
fn refuse_relative(program: &OsStr) -> Result<(), String> {
    let path = crate::var("PATH").unwrap_or_default();
    found_in_relative_entry(program, &path).map_or(Ok(()), Err)
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
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()?;
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
