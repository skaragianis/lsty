//! The programs the process starts for `edit`, `open` and `help`: the
//! editor, the system's opener and the pager.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use lsty_cli::spawn::{editor_command, exec_failure, exit_text, lossy};

/// The program that opens a path on this system.
const fn launcher() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Edits `path` in `editor` (`vi` when it is unset or empty) and waits
/// for it. A non-zero status is an error.
pub(crate) fn run_editor(
    editor: Option<&OsString>,
    path: &Path,
) -> io::Result<()> {
    let words = editor_command(editor.map(OsString::as_os_str), path);
    let Some((program, args)) = words.split_first() else {
        return Err(io::Error::other("no editor"));
    };
    let name = lossy(program.as_bytes());
    let status = Command::new(program).args(args).status().map_err(|e| {
        failed(&format!("editor {name}"), &exec_failure(program, e))
    })?;
    if status.success() {
        return Ok(());
    }
    Err(failed(&format!("editor {name}"), &exit_text(status)))
}

/// Starts the system's opener for `path` and does not wait for it.
pub(crate) fn start_opener(path: &Path) -> io::Result<()> {
    let program = launcher();
    Command::new(program)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| {
            let name = lossy(path.as_os_str().as_bytes());
            failed(
                &format!("open {name}"),
                &exec_failure(OsStr::new(program), e),
            )
        })
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

fn failed(what: &str, why: &str) -> io::Error {
    io::Error::other(format!("{what}: {why}"))
}

#[cfg(test)]
mod tests;
