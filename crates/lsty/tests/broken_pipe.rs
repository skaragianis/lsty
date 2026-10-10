//! Output written to a pipe nobody reads ends the run with the exit code
//! of a process killed by `SIGPIPE`, and says nothing.

use std::io;
use std::process::{Command, Stdio};

#[test]
fn a_closed_stdout_exits_141_silently() -> io::Result<()> {
    let (reader, writer) = io::pipe()?;
    drop(reader);
    let mut command = Command::new(env!("CARGO_BIN_EXE_lsty"));
    command
        .arg("-h")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(Stdio::piped());
    let output = lsty_scripttest::with_coverage(&mut command).output()?;
    assert_eq!(output.status.code(), Some(141));
    assert_eq!(output.stderr, b"");
    Ok(())
}
