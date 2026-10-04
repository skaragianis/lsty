//! Process tests of the terminal interface on a pseudo-terminal: the
//! list is read before the screen is taken over, and the terminal is
//! given back when the program is asked to stop.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

type Check = Result<(), Failed>;

const ENTER: &str = "\x1b[?1049h";
const LEAVE: &str = "\x1b[?1049l";
const LIMIT: Duration = Duration::from_secs(20);
const LIST: &str = "(A) Call client +work\nBuy milk +home\n";

/// What the program wrote to its terminal.
struct Screen {
    chunks: Receiver<Vec<u8>>,
    seen: Vec<u8>,
    master: File,
}

impl Screen {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.seen).into_owned()
    }

    /// Presses a key.
    fn type_key(&mut self, key: u8) -> Check {
        self.master.write_all(&[key])?;
        Ok(())
    }

    /// Reads until `needle` has been written.
    fn wait_for(&mut self, needle: &str) -> Check {
        let end = Instant::now() + LIMIT;
        while !self.text().contains(needle) {
            let left = end.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => self.seen.extend_from_slice(&chunk),
                Err(_) => {
                    return Err(
                        format!("no {needle:?} in {:?}", self.text()).into()
                    );
                }
            }
        }
        Ok(())
    }

    /// Reads until the terminal is closed.
    fn finish(&mut self) {
        loop {
            match self.chunks.recv_timeout(LIMIT) {
                Ok(chunk) => self.seen.extend_from_slice(&chunk),
                Err(
                    RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected,
                ) => {
                    return;
                }
            }
        }
    }
}

struct Work {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Work {
    fn new() -> Result<Self, Failed> {
        let tmp = tempfile::tempdir()?;
        let root = fs::canonicalize(tmp.path())?;
        fs::write(root.join("todo.txt"), LIST)?;
        Ok(Self { _tmp: tmp, root })
    }
}

fn open_pty() -> Result<(File, OwnedFd), Failed> {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let name = ptsname(&master, Vec::new())?;
    let size = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    tcsetwinsize(&master, size)?;
    let tty = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY,
        Mode::empty(),
    )?;
    Ok((File::from(master), tty))
}

/// Starts `lsty` with a terminal for its three streams.
fn start(root: &Path) -> Result<(Child, Screen), Failed> {
    let (master, tty) = open_pty()?;
    let mut reader = master.try_clone()?;
    let (send, chunks) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        // Reading ends with EIO once every copy of the terminal is closed.
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if send.send(buf[..n].to_vec()).is_err() {
                return;
            }
        }
    });
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lsty"));
    cmd.env_clear()
        .env("HOME", root)
        .env("TZ", "UTC")
        .env("TERM", "xterm-256color")
        .env("LSTY_NOW", "2026-09-29T0900Z")
        .env("TODO_FILE", root.join("todo.txt"))
        .current_dir(root)
        .stdin(tty.try_clone()?)
        .stdout(tty.try_clone()?)
        .stderr(tty);
    if let Some(file) = lsty_scripttest::coverage_profile() {
        cmd.env("LLVM_PROFILE_FILE", file);
    }
    let child = cmd.spawn()?;
    // The terminal closes once the child's copies are the only ones.
    drop(cmd);
    let screen = Screen {
        chunks,
        seen: Vec::new(),
        master,
    };
    Ok((child, screen))
}

fn exit_code(child: &mut Child) -> Result<i32, Failed> {
    let end = Instant::now() + LIMIT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(-1));
        }
        if Instant::now() > end {
            child.kill()?;
            return Err("the program did not end".to_owned().into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn busy_list_never_opens_the_screen() -> Check {
    let work = Work::new()?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    let (mut child, mut screen) = start(&work.root)?;
    let code = exit_code(&mut child)?;
    folder.unlock()?;
    screen.finish();
    let text = screen.text();
    if code != 4 {
        return Err(format!("exit {code}, {text:?}").into());
    }
    if !text.contains("busy") {
        return Err(format!("no error in {text:?}").into());
    }
    if text.contains(ENTER) || text.contains(LEAVE) {
        return Err(format!("the screen was taken over: {text:?}").into());
    }
    Ok(())
}

fn stop_by(signal: &str) -> Check {
    let work = Work::new()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for(ENTER)?;
    screen.wait_for("NORMAL")?;
    let sent = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(child.id().to_string())
        .status()?;
    if !sent.success() {
        return Err("kill failed".to_owned().into());
    }
    let code = exit_code(&mut child)?;
    screen.finish();
    let text = screen.text();
    if code != 0 {
        return Err(format!("{signal}: exit {code}").into());
    }
    if !text.contains(&format!("\x1b[?2004l\x1b[?1006l\x1b[?1002l{LEAVE}")) {
        return Err(format!("{signal}: not restored: {text:?}").into());
    }
    if fs::read_to_string(work.root.join("todo.txt"))? != LIST {
        return Err("the file changed".to_owned().into());
    }
    Ok(())
}

/// A signal while the first reading waits for the lock ends the process
/// at once, before any screen is open.
fn signal_during_the_first_read() -> Check {
    let work = Work::new()?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    let (mut child, mut screen) = start(&work.root)?;
    thread::sleep(Duration::from_millis(400));
    let sent = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()?;
    if !sent.success() {
        return Err("kill failed".to_owned().into());
    }
    let at = Instant::now();
    let code = exit_code(&mut child)?;
    let waited = at.elapsed();
    folder.unlock()?;
    screen.finish();
    if code == 0 || code == 4 {
        return Err(format!("exit {code}: the signal was lost").into());
    }
    if waited > Duration::from_millis(1000) {
        return Err(format!("ended after {waited:?}").into());
    }
    Ok(())
}

fn q_quits_and_restores() -> Check {
    let work = Work::new()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for("NORMAL")?;
    screen.type_key(b'q')?;
    let code = exit_code(&mut child)?;
    screen.finish();
    if code != 0 || !screen.text().contains(LEAVE) {
        return Err(format!("exit {code}, {:?}", screen.text()).into());
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let trials = vec![
        Trial::test(
            "30.27 a busy list never opens the screen",
            busy_list_never_opens_the_screen,
        ),
        Trial::test("30.29 SIGTERM restores the terminal", || stop_by("TERM")),
        Trial::test("30.30 SIGHUP restores the terminal", || stop_by("HUP")),
        Trial::test("30.31 q restores the terminal", q_quits_and_restores),
        Trial::test(
            "a signal during the first reading ends the process",
            signal_during_the_first_read,
        ),
    ];
    libtest_mimic::run(&Arguments::from_args(), trials).exit_code()
}
