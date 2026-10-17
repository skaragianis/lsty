use std::{
    ffi::OsStr,
    io,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::Command,
};

use rustix::io::Errno;

use super::{
    exec_failure, exit_text, has_exec_bit, is_executable, linux_word, lookup,
    macos_word, signal_name,
};

fn make(dir: &Path, name: &str, mode: u32) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, "#!/bin/sh\n").unwrap_or_else(|e| panic!("{e}"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("{e}"));
    file
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
    let found = |name: &str| lookup(Path::new("/"), &path, OsStr::new(name));
    assert_eq!(found("tool"), Ok(tool));
    let missing = |name: &str| {
        Err(format!(
            "exec: \"{name}\": executable file not found in $PATH"
        ))
    };
    assert_eq!(found("plain"), missing("plain"));
    assert_eq!(found("absent"), missing("absent"));
}

#[test]
fn a_program_found_first_in_a_relative_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    make(dir.path(), "tool", 0o755);
    let cwd = dir.path().join("sub");
    std::fs::create_dir(&cwd).unwrap_or_else(|e| panic!("{e}"));
    let abs = dir.path().as_os_str();
    let name = OsStr::new("tool");
    let refused = |cwd: &Path, entries: &[&OsStr], name: &OsStr| {
        let path = entries.join(OsStr::new(":"));
        lookup(cwd, &path, name)
    };
    let want = Err("exec: \"tool\": cannot run executable found relative \
                    to current directory"
        .to_owned());
    assert_eq!(refused(dir.path(), &[OsStr::new("."), abs], name), want);
    assert_eq!(refused(dir.path(), &[OsStr::new(""), abs], name), want);
    assert_eq!(refused(&cwd, &[OsStr::new(".."), abs], name), want);
    assert_eq!(
        refused(&cwd, &[abs, OsStr::new("..")], name),
        Ok(dir.path().join("tool"))
    );
    let absent = OsStr::new("absent");
    assert_eq!(
        refused(&cwd, &[OsStr::new("."), abs], absent),
        Err("exec: \"absent\": executable file not found in $PATH".to_owned())
    );
}

#[test]
fn a_name_with_a_slash_is_not_searched_for() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    make(dir.path(), "tool", 0o755);
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap_or_else(|e| panic!("{e}"));
    make(&sub, "tool", 0o755);
    let path = joined(&[PathBuf::from("."), dir.path().to_path_buf()]);
    for name in ["./tool", "sub/tool"] {
        let name = OsStr::new(name);
        assert_eq!(
            lookup(dir.path(), &path, name),
            Ok(name.into()),
            "{name:?}"
        );
    }
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
        exec_failure(OsStr::new("nosuched"), &err),
        "exec: \"nosuched\": executable file not found in $PATH"
    );
}

#[test]
fn invalid_bytes_of_a_name_are_hex_escapes() {
    use std::os::unix::ffi::OsStrExt as _;
    let err = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(
        exec_failure(OsStr::from_bytes(b"ed\xff"), &err),
        "exec: \"ed\\xff\": executable file not found in $PATH"
    );
}

#[test]
fn a_bare_name_that_is_not_executable_counts_as_not_found() {
    let err = io::Error::from(Errno::ACCESS);
    assert_eq!(
        exec_failure(OsStr::new("ed2"), &err),
        "exec: \"ed2\": executable file not found in $PATH"
    );
    assert_eq!(
        exec_failure(OsStr::new("./ed2"), &err),
        "fork/exec ./ed2: permission denied"
    );
}

#[test]
fn a_path_that_cannot_start_names_the_system_call() {
    let err = io::Error::from;
    assert_eq!(
        exec_failure(OsStr::new("/no/such/ed"), &err(Errno::NOENT)),
        "fork/exec /no/such/ed: no such file or directory"
    );
    assert_eq!(
        exec_failure(OsStr::new("/tmp"), &err(Errno::ACCESS)),
        "fork/exec /tmp: permission denied"
    );
}

#[test]
fn a_signal_without_a_name_reads_as_its_number() {
    assert_eq!(signal_name(0), "signal 0");
    assert_eq!(signal_name(99), "signal 99");
}

/// The signals every system names alike.
const COMMON_SIGNALS: &[(i32, &str)] = &[
    (1, "hangup"),
    (2, "interrupt"),
    (3, "quit"),
    (4, "illegal instruction"),
    (8, "floating point exception"),
    (9, "killed"),
    (11, "segmentation fault"),
    (13, "broken pipe"),
    (14, "alarm clock"),
    (15, "terminated"),
];

const LINUX_SIGNALS: &[(i32, &str)] = &[
    (5, "trace/breakpoint trap"),
    (6, "aborted"),
    (7, "bus error"),
    (10, "user defined signal 1"),
    (12, "user defined signal 2"),
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

const MACOS_SIGNALS: &[(i32, &str)] = &[
    (5, "trace/BPT trap"),
    (6, "abort trap"),
    (7, "EMT trap"),
    (10, "bus error"),
    (12, "bad system call"),
    (16, "urgent I/O condition"),
    (17, "suspended (signal)"),
    (18, "suspended"),
    (19, "continued"),
    (20, "child exited"),
    (21, "stopped (tty input)"),
    (22, "stopped (tty output)"),
    (23, "I/O possible"),
    (24, "cputime limit exceeded"),
    (25, "filesize limit exceeded"),
    (26, "virtual timer expired"),
    (27, "profiling timer expired"),
    (28, "window size changes"),
    (29, "information request"),
    (30, "user defined signal 1"),
    (31, "user defined signal 2"),
];

#[test]
fn every_signal_has_its_system_name() {
    let own = if cfg!(target_os = "macos") {
        MACOS_SIGNALS
    } else {
        LINUX_SIGNALS
    };
    for &(signal, name) in COMMON_SIGNALS.iter().chain(own) {
        assert_eq!(signal_name(signal), name, "signal {signal}");
    }
}

#[test]
fn each_system_has_names_of_its_own() {
    for &(signal, name) in LINUX_SIGNALS {
        assert_eq!(linux_word(signal), Some(name), "signal {signal}");
    }
    for &(signal, name) in MACOS_SIGNALS {
        assert_eq!(macos_word(signal), Some(name), "signal {signal}");
    }
}

#[test]
fn a_file_runs_when_any_of_its_execute_bits_is_set() {
    let rows = [
        (0o000, false),
        (0o644, false),
        (0o100, true),
        (0o010, true),
        (0o001, true),
        (0o755, true),
    ];
    for (mode, want) in rows {
        assert_eq!(has_exec_bit(mode), want, "{mode:o}");
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
