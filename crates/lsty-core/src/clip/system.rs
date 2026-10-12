//! Running a reader or writer program for real.

use std::{
    io::{self, Read as _, Write as _},
    os::unix::process::CommandExt as _,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    thread,
    time::{Duration, Instant},
};

use super::{Program, Runner};
use crate::program::{exec_failure, exit_text};

/// How often a program that closed its output is asked whether it
/// ended.
const POLL: Duration = Duration::from_millis(10);

/// How long the output may take after the program ended.
pub(super) const GRACE: Duration = Duration::from_millis(100);

/// Runs programs on this system. The program's standard error is
/// discarded; its standard input is empty unless it is given text.
#[derive(Clone, Copy, Debug)]
pub struct OsRunner;

impl Runner for OsRunner {
    fn run(
        &self,
        program: &Program,
        input: Option<&[u8]>,
        limit: Duration,
    ) -> io::Result<Vec<u8>> {
        match input {
            Some(text) => write_within(program, text, limit).map(|()| vec![]),
            None => run_within(program, limit, GRACE),
        }
    }
}

/// Starts `program` with the given standard input and output.
fn spawn(program: &Program, stdin: Stdio, stdout: Stdio) -> io::Result<Child> {
    Command::new(&program.file)
        .arg0(&program.name)
        .args(&program.args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            io::Error::other(exec_failure(program.file.as_os_str(), &e))
        })
}

/// Gives `text` to `program` and waits for it for at most `limit`. The
/// text goes on a thread of its own, so that a program that does not
/// read it cannot hold this one.
fn write_within(
    program: &Program,
    text: &[u8],
    limit: Duration,
) -> io::Result<()> {
    let deadline = Instant::now().checked_add(limit);
    let mut child = spawn(program, Stdio::piped(), Stdio::null())?;
    let text = text.to_vec();
    let stdin: Option<ChildStdin> = child.stdin.take();
    let spawned =
        thread::Builder::new()
            .name("clipboard".to_owned())
            .spawn(move || {
                // The program may end without reading; the input closes.
                drop(stdin.map(|mut pipe| pipe.write_all(&text)));
            });
    if let Err(e) = spawned {
        stop(&mut child);
        return Err(e);
    }
    let status = finish(&mut child, deadline)?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(exit_text(status)))
    }
}

/// Runs `program` for at most `limit`, giving its output `grace` after
/// it ended.
pub(super) fn run_within(
    program: &Program,
    limit: Duration,
    grace: Duration,
) -> io::Result<Vec<u8>> {
    let deadline = Instant::now().checked_add(limit);
    let mut child = spawn(program, Stdio::null(), Stdio::piped())?;
    let rx = collect_output(&mut child)?;
    let (out, status) = await_output(&mut child, &rx, deadline, grace);
    let status = match (&out, status) {
        (Ok(_), Some(status)) => Ok(status),
        (Ok(_), None) => finish(&mut child, deadline),
        (Err(_), _) => {
            stop(&mut child);
            Err(timed_out())
        }
    };
    match (out, status) {
        (Ok(bytes), Ok(status)) if status.success() => Ok(bytes),
        (Ok(_), Ok(status)) => Err(io::Error::other(exit_text(status))),
        (Err(e), _) | (Ok(_), Err(e)) => Err(e),
    }
}

/// Waits for the output. Once the program has ended, the output gets a
/// short time more: a program left running by it may hold the pipe.
/// Gives the status when it was seen on the way.
fn await_output(
    child: &mut Child,
    rx: &Receiver<io::Result<Vec<u8>>>,
    deadline: Option<Instant>,
    grace: Duration,
) -> (io::Result<Vec<u8>>, Option<ExitStatus>) {
    let mut ended: Option<(Instant, ExitStatus)> = None;
    loop {
        match rx.recv_timeout(POLL) {
            Ok(read) => return (read, ended.map(|(_, status)| status)),
            Err(RecvTimeoutError::Disconnected) => {
                let lost = io::Error::other("the output was lost");
                return (Err(lost), None);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return (Err(timed_out()), None);
        }
        match &ended {
            Some((at, _)) if at.elapsed() >= grace => {
                return (Err(timed_out()), None);
            }
            Some(_) => {}
            None => {
                if let Ok(Some(status)) = child.try_wait() {
                    ended = Some((Instant::now(), status));
                }
            }
        }
    }
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "timed out")
}

/// Reads the output of `child` on a thread of its own.
fn collect_output(
    child: &mut Child,
) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
    let (tx, rx) = channel();
    let Some(mut stdout) = child.stdout.take() else {
        return Err(io::Error::other("no output"));
    };
    let spawned =
        thread::Builder::new()
            .name("clipboard".to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let read = stdout.read_to_end(&mut bytes).map(|_| bytes);
                // Nobody is left to take it when the reader gave up.
                drop(tx.send(read));
            });
    match spawned {
        Ok(_) => Ok(rx),
        Err(e) => {
            stop(child);
            Err(e)
        }
    }
}

/// Waits for the program to end after it printed everything, within the
/// time that is left; otherwise stops it.
fn finish(
    child: &mut Child,
    deadline: Option<Instant>,
) -> io::Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            stop(child);
            return Err(timed_out());
        }
        thread::sleep(POLL);
    }
}

/// Kills the program and collects it.
fn stop(child: &mut Child) {
    // It may have ended already; there is nothing more to do either way.
    drop(child.kill());
    drop(child.wait());
}
