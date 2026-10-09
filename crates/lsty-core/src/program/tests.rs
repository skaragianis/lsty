use std::ffi::OsStr;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{
    exec_failure, exit_text, find_in_path, is_executable, refuse_relative,
    signal_name,
};

fn make(dir: &Path, name: &str, mode: u32) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, "#!/bin/sh\n").unwrap_or_else(|e| panic!("{e}"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("{e}"));
    file
}

/// A folder as a relative path from the current folder.
fn relative_to(dir: &Path) -> PathBuf {
    // Extra `..` at the root stay at the root.
    let up = "../".repeat(64);
    let below = dir.strip_prefix("/").unwrap_or(dir);
    PathBuf::from(up).join(below)
}

fn joined(entries: &[PathBuf]) -> std::ffi::OsString {
    std::env::join_paths(entries).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_program_is_found_in_the_search_path() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let tool = make(dir.path(), "tool", 0o755);
    make(dir.path(), "plain", 0o644);
    let path = joined(&[
        PathBuf::new(),
        PathBuf::from("/no/such/folder"),
        dir.path().to_path_buf(),
    ]);
    assert_eq!(find_in_path(&path, OsStr::new("tool")), Some(tool));
    assert_eq!(find_in_path(&path, OsStr::new("plain")), None);
    assert_eq!(find_in_path(&path, OsStr::new("absent")), None);
}

#[test]
fn a_relative_search_path_entry_is_skipped() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = make(dir.path(), "tool", 0o755);
    let relative = relative_to(dir.path());
    assert!(relative.join("tool").is_file());
    let path = joined(std::slice::from_ref(&relative));
    assert_eq!(find_in_path(&path, OsStr::new("tool")), None);
    let both = joined(&[relative, dir.path().to_path_buf()]);
    assert_eq!(find_in_path(&both, OsStr::new("tool")), Some(file));
}

#[test]
fn a_program_found_first_in_a_relative_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    make(dir.path(), "tool", 0o755);
    let cwd = dir.path().join("sub");
    std::fs::create_dir(&cwd).unwrap_or_else(|e| panic!("{e}"));
    let absolute = joined(&[dir.path().to_path_buf()]);
    let name = OsStr::new("tool");
    let refused = |cwd: &Path, entries: &[&OsStr], name: &OsStr| {
        let path = entries.join(OsStr::new(":"));
        refuse_relative(cwd, &path, name)
    };
    assert_eq!(refuse_relative(&cwd, &absolute, name), Ok(()));
    let abs = absolute.as_os_str();
    let want = Err("exec: \"tool\": cannot run executable found relative \
                    to current directory"
        .to_owned());
    assert_eq!(refused(dir.path(), &[OsStr::new("."), abs], name), want);
    assert_eq!(refused(dir.path(), &[OsStr::new(""), abs], name), want);
    assert_eq!(refused(&cwd, &[OsStr::new(".."), abs], name), want);
    assert_eq!(refused(&cwd, &[abs, OsStr::new("..")], name), Ok(()));
    let absent = OsStr::new("absent");
    assert_eq!(refused(&cwd, &[OsStr::new("."), abs], absent), Ok(()));
    assert_eq!(refused(&cwd, &[], absent), Ok(()));
}

#[test]
fn a_name_with_a_slash_or_no_letters_is_not_searched_for() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    make(dir.path(), "tool", 0o755);
    let path = joined(&[dir.path().to_path_buf(), PathBuf::from(".")]);
    for name in ["./tool", "sub/tool", ""] {
        let name = OsStr::new(name);
        assert_eq!(find_in_path(&path, name), None, "{name:?}");
        assert_eq!(refuse_relative(dir.path(), &path, name), Ok(()));
    }
    let dot = OsStr::new(".");
    assert_eq!(
        refuse_relative(dir.path(), dot, OsStr::new("./tool")),
        Ok(())
    );
}

#[test]
fn a_folder_or_a_file_nobody_may_run_is_not_a_program() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir(dir.path().join("tool"))
        .unwrap_or_else(|e| panic!("{e}"));
    make(dir.path(), "plain", 0o644);
    assert!(!is_executable(&dir.path().join("tool")));
    assert!(!is_executable(&dir.path().join("plain")));
    assert!(!is_executable(&dir.path().join("absent")));
    let run = make(dir.path(), "run", 0o700);
    assert!(is_executable(&run));
}

#[test]
fn a_bare_name_that_is_not_found_names_the_search_path() {
    let err = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(
        exec_failure(OsStr::new("nosuched"), err),
        "exec: \"nosuched\": executable file not found in $PATH"
    );
}

#[test]
fn invalid_bytes_of_a_name_are_hex_escapes() {
    use std::os::unix::ffi::OsStrExt as _;
    let err = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(
        exec_failure(OsStr::from_bytes(b"ed\xff"), err),
        "exec: \"ed\\xff\": executable file not found in $PATH"
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
fn a_signal_without_a_name_reads_as_its_number() {
    assert_eq!(signal_name(0), "signal 0");
    assert_eq!(signal_name(99), "signal 99");
}

#[cfg(not(target_os = "macos"))]
const LINUX_SIGNALS: &[(i32, &str)] = &[
    (1, "hangup"),
    (2, "interrupt"),
    (3, "quit"),
    (4, "illegal instruction"),
    (5, "trace/breakpoint trap"),
    (6, "aborted"),
    (7, "bus error"),
    (8, "floating point exception"),
    (9, "killed"),
    (10, "user defined signal 1"),
    (11, "segmentation fault"),
    (12, "user defined signal 2"),
    (13, "broken pipe"),
    (14, "alarm clock"),
    (15, "terminated"),
    (16, "stack fault"),
    (17, "child exited"),
    (18, "continued"),
    (19, "stopped (signal)"),
    (20, "stopped"),
    (21, "stopped (tty input)"),
    (22, "stopped (tty output)"),
    (23, "urgent I/O condition"),
    (24, "CPU time limit exceeded"),
    (25, "file size limit exceeded"),
    (26, "virtual timer expired"),
    (27, "profiling timer expired"),
    (28, "window changed"),
    (29, "I/O possible"),
    (30, "power failure"),
    (31, "bad system call"),
];

#[cfg(not(target_os = "macos"))]
#[test]
fn every_signal_has_its_system_name() {
    for &(signal, name) in LINUX_SIGNALS {
        assert_eq!(signal_name(signal), name, "signal {signal}");
    }
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
