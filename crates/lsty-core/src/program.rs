//! Finding a program in a search path, as the system would run it, and
//! wording how starting it and its ending fail.

use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use rustix::fs::{Access, AtFlags, CWD, accessat};
use rustix::io::Errno;

use crate::errs::{self, quote_bytes};

/// The permission bits that let anyone run a file.
const EXEC_BITS: u32 = 0o111;

/// The folders of `path`, a list separated as `PATH` is. An empty entry
/// is the current folder.
fn folders(path: &OsStr) -> impl Iterator<Item = &Path> {
    path.as_bytes()
        .split(|&b| b == b':')
        .map(|entry| if entry.is_empty() { b"." } else { entry })
        .map(|entry| Path::new(OsStr::from_bytes(entry)))
}

/// Whether `name` is searched for in the folders of a path: it has no `/`.
fn is_bare(name: &OsStr) -> bool {
    !name.as_bytes().contains(&b'/')
}

/// Whether the system lets this process run `file`: it is not a folder
/// and the effective user may execute it.
pub(crate) fn is_executable(file: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(file) else {
        return false;
    };
    if meta.is_dir() {
        return false;
    }
    match accessat(CWD, file, Access::EXEC_OK, AtFlags::EACCESS) {
        Ok(()) => true,
        Err(Errno::NOSYS | Errno::PERM) => {
            meta.permissions().mode() & EXEC_BITS != 0
        }
        Err(_) => false,
    }
}

/// An error, worded as the system words it, when the bare name `name`
/// is found first in a folder of `path` that is not absolute (an empty
/// entry is the current folder), taken from `cwd`: such a program must
/// not be run. A name that is not bare is not searched for.
pub fn refuse_relative(
    cwd: &Path,
    path: &OsStr,
    name: &OsStr,
) -> Result<(), String> {
    if !is_bare(name) {
        return Ok(());
    }
    let hit = folders(path)
        .map(|dir| dir.join(name))
        .find(|file| is_executable(&cwd.join(file)));
    if hit.is_some_and(|file| !file.is_absolute()) {
        let name = quote_bytes(name.as_bytes());
        return Err(format!(
            "exec: {name}: cannot run executable found relative to \
             current directory"
        ));
    }
    Ok(())
}

/// The first executable file called `name` in an absolute folder of
/// `path`. Entries that are not absolute (empty ones, `.`, `bin`) are
/// skipped, so nothing runs from the current folder. A name that is not
/// bare is not searched for.
pub fn find_in_path(path: &OsStr, name: &OsStr) -> Option<PathBuf> {
    if !is_bare(name) {
        return None;
    }
    folders(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|file| is_executable(file))
}

/// Why `program` could not be started: a bare name that is not on the
/// search path, or the system's reason for a path.
pub fn exec_failure(program: &OsStr, err: io::Error) -> String {
    let bytes = program.as_bytes();
    let not_found = matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    );
    if not_found && !bytes.contains(&b'/') {
        let name = quote_bytes(bytes);
        return format!("exec: {name}: executable file not found in $PATH");
    }
    errs::io("fork/exec", Path::new(program), err)
        .message()
        .to_owned()
}

/// How a program that ran ended unsuccessfully: `exit status 3`,
/// `signal: killed`, with ` (core dumped)` when it left a core.
pub fn exit_text(status: ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit status {code}"),
        (None, Some(signal)) => {
            let core = if status.core_dumped() {
                " (core dumped)"
            } else {
                ""
            };
            format!("signal: {}{core}", signal_name(signal))
        }
        (None, None) => "stopped".to_owned(),
    }
}

/// The system's name for a signal, `signal N` when there is none.
pub(crate) fn signal_name(signal: i32) -> String {
    match signal_word(signal) {
        Some(word) => word.to_owned(),
        None => format!("signal {signal}"),
    }
}

fn signal_word(signal: i32) -> Option<&'static str> {
    let word = match signal {
        1 => "hangup",
        2 => "interrupt",
        3 => "quit",
        4 => "illegal instruction",
        8 => "floating point exception",
        9 => "killed",
        11 => "segmentation fault",
        13 => "broken pipe",
        14 => "alarm clock",
        15 => "terminated",
        other => return platform_word(other),
    };
    Some(word)
}

#[cfg(target_os = "macos")]
fn platform_word(signal: i32) -> Option<&'static str> {
    let word = match signal {
        5 => "trace/BPT trap",
        6 => "abort trap",
        7 => "EMT trap",
        10 => "bus error",
        12 => "bad system call",
        16 => "urgent I/O condition",
        17 => "suspended (signal)",
        18 => "suspended",
        19 => "continued",
        20 => "child exited",
        21 => "stopped (tty input)",
        22 => "stopped (tty output)",
        23 => "I/O possible",
        24 => "cputime limit exceeded",
        25 => "filesize limit exceeded",
        26 => "virtual timer expired",
        27 => "profiling timer expired",
        28 => "window size changes",
        29 => "information request",
        30 => "user defined signal 1",
        31 => "user defined signal 2",
        _ => return None,
    };
    Some(word)
}

#[cfg(not(target_os = "macos"))]
fn platform_word(signal: i32) -> Option<&'static str> {
    let word = match signal {
        5 => "trace/breakpoint trap",
        6 => "aborted",
        7 => "bus error",
        10 => "user defined signal 1",
        12 => "user defined signal 2",
        16 => "stack fault",
        17 => "child exited",
        18 => "continued",
        19 => "stopped (signal)",
        20 => "stopped",
        21 => "stopped (tty input)",
        22 => "stopped (tty output)",
        23 => "urgent I/O condition",
        24 => "CPU time limit exceeded",
        25 => "file size limit exceeded",
        26 => "virtual timer expired",
        27 => "profiling timer expired",
        28 => "window changed",
        29 => "I/O possible",
        30 => "power failure",
        31 => "bad system call",
        _ => return None,
    };
    Some(word)
}

#[cfg(test)]
mod tests;
