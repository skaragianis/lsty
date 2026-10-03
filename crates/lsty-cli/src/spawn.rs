//! How starting a program and its ending are worded when they fail.

use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::ExitStatus;

use lsty_core::errs;

pub use lsty_core::errs::lossy;
pub use lsty_core::links::editor_command;

/// Why `program` could not be started: a bare name that is not on the
/// search path, or the system's reason for a path.
pub fn exec_failure(program: &OsStr, err: io::Error) -> String {
    let bytes = program.as_bytes();
    let not_found = matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    );
    if not_found && !bytes.contains(&b'/') {
        let name = errs::quote(&lossy(bytes));
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
pub fn signal_name(signal: i32) -> String {
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
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn a_bare_name_that_is_not_found_names_the_search_path() {
        let err = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(
            exec_failure(OsStr::new("nosuched"), err),
            "exec: \"nosuched\": executable file not found in $PATH"
        );
    }

    #[test]
    fn a_bare_name_that_is_not_executable_counts_as_not_found() {
        let err = io::Error::from_raw_os_error(13);
        assert_eq!(
            exec_failure(OsStr::new("ed2"), err),
            "exec: \"ed2\": executable file not found in $PATH"
        );
        let err = io::Error::from_raw_os_error(13);
        assert_eq!(
            exec_failure(OsStr::new("./ed2"), err),
            "fork/exec ./ed2: permission denied"
        );
    }

    #[test]
    fn a_path_that_cannot_start_names_the_system_call() {
        let err = |code| io::Error::from_raw_os_error(code);
        assert_eq!(
            exec_failure(OsStr::new("/no/such/ed"), err(2)),
            "fork/exec /no/such/ed: no such file or directory"
        );
        assert_eq!(
            exec_failure(OsStr::new("/tmp"), err(13)),
            "fork/exec /tmp: permission denied"
        );
    }

    #[test]
    fn signals_have_the_system_names() {
        assert_eq!(signal_name(9), "killed");
        assert_eq!(signal_name(15), "terminated");
        assert_eq!(signal_name(2), "interrupt");
        assert_eq!(signal_name(0), "signal 0");
        assert_eq!(signal_name(99), "signal 99");
    }

    #[test]
    fn endings_read_as_exit_status_or_signal() {
        let ended = |script: &str| {
            let status = Command::new("sh").args(["-c", script]).status();
            status.map(exit_text).unwrap_or_default()
        };
        assert_eq!(ended("exit 3"), "exit status 3");
        assert_eq!(ended("kill -9 $$"), "signal: killed");
        assert_eq!(ended("kill -15 $$"), "signal: terminated");
    }
}
