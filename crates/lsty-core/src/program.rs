//! Finding a program in a search path, as the system would run it, and
//! wording how starting it and its ending fail.

use std::{
    ffi::OsStr,
    io,
    os::unix::{
        ffi::OsStrExt as _, fs::PermissionsExt as _,
        process::ExitStatusExt as _,
    },
    path::{Path, PathBuf},
    process::ExitStatus,
};

use rustix::{
    fs::{Access, AtFlags, CWD, accessat},
    io::Errno,
};

use crate::errs::{self, quote};

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

/// The file to run for `name`: a name with a `/` as it is, else the first
/// executable file called `name` in a folder of `path`, taken from `cwd`.
///
/// # Errors
///
/// There is no such file, or it is found through a folder that is not
/// absolute (an empty entry is the current folder): such a program must
/// not be run. The text is worded as the system words it.
pub fn lookup(
    cwd: &Path,
    path: &OsStr,
    name: &OsStr,
) -> Result<PathBuf, String> {
    if !is_bare(name) {
        return Ok(PathBuf::from(name));
    }
    let hit = folders(path)
        .map(|dir| dir.join(name))
        .find(|file| is_executable(&cwd.join(file)));
    match hit {
        Some(file) if !file.is_absolute() => {
            let name = quote(name.as_bytes());
            Err(format!(
                "exec: {name}: cannot run executable found relative to \
                 current directory"
            ))
        }
        Some(file) => Ok(file),
        None => Err(exec_failure(name, &io::ErrorKind::NotFound.into())),
    }
}

/// Why `program` could not be started: a bare name that is not on the
/// search path, or the system's reason for a path.
pub fn exec_failure(program: &OsStr, err: &io::Error) -> String {
    let not_found = matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    );
    if not_found && is_bare(program) {
        let name = quote(program.as_bytes());
        return format!("exec: {name}: executable file not found in $PATH");
    }
    format!(
        "fork/exec {}: {}",
        errs::lossy_path(program),
        errs::io_text(err)
    )
}

/// How a program that ran ended unsuccessfully: `exit status 3`,
/// `signal: killed`, with ` (core dumped)` when it left a core.
pub fn exit_text(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit status {code}");
    }
    let core = if status.core_dumped() {
        " (core dumped)"
    } else {
        ""
    };
    // Without `WUNTRACED` a status with no code is a signal.
    let signal = status.signal().unwrap_or_default();
    format!("signal: {}{core}", signal_name(signal))
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
