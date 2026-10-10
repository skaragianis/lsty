//! Process tests of the terminal interface on a pseudo-terminal: the
//! list is read before the screen is taken over, and the terminal is
//! given back when the program is asked to stop. A change of the files
//! by another process is shown without a key press.
#![expect(
    clippy::disallowed_methods,
    reason = "tests build paths of their own, which are UTF-8"
)]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use rustix::termios::{Winsize, tcsetwinsize};

type Check = Result<(), Failed>;

const ENTER: &str = "\x1b[?1049h";
const LEAVE: &str = "\x1b[?1049l";
const LIMIT: Duration = Duration::from_secs(20);
/// The text of the item another writer adds to `LIST`.
const ADDED: &str = "Walk the dog";
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
        self.wait_until(&format!("{needle:?}"), |text| text.contains(needle))
    }

    /// Reads until `done` holds of everything written so far.
    fn wait_until(
        &mut self,
        what: &str,
        done: impl Fn(&str) -> bool,
    ) -> Check {
        let end = Instant::now() + LIMIT;
        while !done(&self.text()) {
            let left = end.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => self.seen.extend_from_slice(&chunk),
                Err(_) => {
                    return Err(
                        format!("no {what} in {:?}", self.text()).into()
                    );
                }
            }
        }
        Ok(())
    }

    /// Reads until the terminal is closed.
    fn finish(&mut self) {
        while let Ok(chunk) = self.chunks.recv_timeout(LIMIT) {
            self.seen.extend_from_slice(&chunk);
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

    /// Makes a lock wait outlast `LIMIT`, so a program that ends in time
    /// did not merely give up waiting.
    fn patient(self) -> Result<Self, Failed> {
        let config = self.root.join(".config").join("lsty");
        fs::create_dir_all(&config)?;
        fs::write(config.join("config.toml"), "lock_timeout = \"10m\"\n")?;
        Ok(self)
    }
}

fn open_pty() -> Result<(File, OwnedFd), Failed> {
    let (master, tty) = lsty_scripttest::open_pty()?;
    let size = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // Set on the terminal side: macOS refuses the size on the master.
    tcsetwinsize(&tty, size)?;
    Ok((File::from(master), tty))
}

/// Starts `lsty` with a terminal for its three streams.
fn start(root: &Path) -> Result<(Child, Screen), Failed> {
    start_with(root, &[])
}

/// Like [`start`], with more variables in the environment.
fn start_with(
    root: &Path,
    vars: &[(&str, &Path)],
) -> Result<(Child, Screen), Failed> {
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
    let child = spawn(root, vars, tty)?;
    let screen = Screen {
        chunks,
        seen: Vec::new(),
        master,
    };
    Ok((child, screen))
}

/// Starts `lsty` in `root` with `tty` for its three streams.
fn spawn(
    root: &Path,
    vars: &[(&str, &Path)],
    tty: OwnedFd,
) -> Result<Child, Failed> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lsty"));
    cmd.env_clear()
        .env("HOME", root)
        .env("TZ", "UTC")
        .env("TERM", "xterm-256color")
        .env("LSTY_NOW", "2026-09-29T0900Z")
        .env("TODO_FILE", root.join("todo.txt"))
        .envs(vars.iter().copied())
        .current_dir(root)
        .stdin(tty.try_clone()?)
        .stdout(tty.try_clone()?)
        .stderr(tty);
    let child = lsty_scripttest::with_coverage(&mut cmd).spawn()?;
    // The terminal closes once the child's copies are the only ones.
    drop(cmd);
    Ok(child)
}

fn exit_code(child: &mut Child) -> Result<i32, Failed> {
    let end = Instant::now() + LIMIT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(-1));
        }
        if Instant::now() > end {
            child.kill()?;
            return Err("the program did not end".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Sends the signal `name` (`TERM`, `HUP`, `INT`) to `child`.
fn signal(child: &Child, name: &str) -> Check {
    let sent = Command::new("kill")
        .arg(format!("-{name}"))
        .arg(child.id().to_string())
        .status()?;
    if !sent.success() {
        return Err("kill failed".into());
    }
    Ok(())
}

/// Waits for the program to end and checks its exit code.
fn ends_with(child: &mut Child, want: i32) -> Check {
    let code = exit_code(child)?;
    if code != want {
        return Err(format!("exit {code}").into());
    }
    Ok(())
}

fn busy_list_never_opens_the_screen() -> Check {
    let work = Work::new()?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    let (mut child, mut screen) = start(&work.root)?;
    let code = exit_code(&mut child)?;
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

fn stop_by(name: &str) -> Check {
    let work = Work::new()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for(ENTER)?;
    screen.wait_for("NORMAL")?;
    signal(&child, name)?;
    ends_with(&mut child, 0)?;
    screen.finish();
    let text = screen.text();
    if !text.contains(&format!("\x1b[?2004l\x1b[?1006l\x1b[?1002l{LEAVE}")) {
        return Err(format!("not restored: {text:?}").into());
    }
    if fs::read_to_string(work.root.join("todo.txt"))? != LIST {
        return Err("the file changed".into());
    }
    Ok(())
}

/// Replaces the todo file as another writer would: a temporary file,
/// renamed.
fn replace_list(root: &Path) -> Check {
    let staged = root.join("todo.txt.tmp");
    fs::write(&staged, format!("{LIST}{ADDED} +pets\n"))?;
    fs::rename(&staged, root.join("todo.txt"))?;
    Ok(())
}

fn another_writer_is_shown_without_a_key() -> Check {
    let work = Work::new()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for("Buy milk")?;
    replace_list(&work.root)?;
    screen.wait_for(ADDED)?;
    screen.type_key(b'q')?;
    ends_with(&mut child, 0)
}

/// A signal while a reload waits for a lock that stays held ends the
/// program at once.
fn stop_during_a_busy_reload() -> Check {
    let work = Work::new()?.patient()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for("Buy milk")?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    replace_list(&work.root)?;
    // The debounce is 50 ms. Waiting for the lock cannot be seen from
    // outside (it polls), so a slow machine only makes this pass without
    // testing the busy wait.
    thread::sleep(Duration::from_millis(400));
    signal(&child, "TERM")?;
    ends_with(&mut child, 0)?;
    screen.finish();
    if !screen.text().contains(LEAVE) {
        return Err("the terminal was not given back".into());
    }
    Ok(())
}

/// A signal while the first reading waits for the lock ends the process
/// at once, before any screen is open.
fn signal_during_the_first_read() -> Check {
    let work = Work::new()?.patient()?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    let (mut child, _screen) = start(&work.root)?;
    // Give the child time to start reading. Waiting for the lock cannot
    // be seen from outside (it polls), so on a slow machine the signal may
    // arrive earlier and end the child by default: the test then passes
    // without testing the wait.
    thread::sleep(Duration::from_millis(400));
    signal(&child, "TERM")?;
    // -1: ended by the signal itself, as by default.
    ends_with(&mut child, -1)
}

/// An end of file typed while the first reading waits does not hold the
/// screen back once the list is read.
fn eof_before_the_screen_opens() -> Check {
    let work = Work::new()?;
    let folder = File::open(&work.root)?;
    folder.lock()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.type_key(4)?;
    screen.type_key(b'q')?;
    // Let the line discipline take the bytes in cooked mode; without the
    // wait the test can pass without testing anything.
    thread::sleep(Duration::from_millis(100));
    folder.unlock()?;
    ends_with(&mut child, 0)
}

/// Reads `master` until `needle` has been written, without another copy
/// of it, so that dropping it closes the terminal.
fn read_until(master: &mut File, needle: &str) -> Check {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    const SLICE: Timespec = Timespec {
        tv_sec: 0,
        tv_nsec: 100_000_000,
    };
    let end = Instant::now() + LIMIT;
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains(needle) {
        if Instant::now() > end {
            let text = String::from_utf8_lossy(&seen);
            return Err(format!("no {needle:?} in {text:?}").into());
        }
        let mut fds = [PollFd::new(&*master, PollFlags::IN)];
        if poll(&mut fds, Some(&SLICE))? > 0 {
            let mut buf = [0_u8; 4096];
            let n = master.read(&mut buf)?;
            seen.extend_from_slice(buf.get(..n).unwrap_or_default());
        }
    }
    Ok(())
}

/// A terminal that goes away while the screen is open ends the program,
/// though the system sends no SIGHUP: the test's terminal is not the
/// program's controlling one. It exits 1, not 0: the screen cannot be
/// given back, so standard output is never flushed, or reading the
/// terminal fails first.
fn a_closed_terminal_ends_the_program() -> Check {
    let work = Work::new()?;
    let (mut master, tty) = open_pty()?;
    let mut child = spawn(&work.root, &[], tty)?;
    read_until(&mut master, "NORMAL")?;
    drop(master);
    ends_with(&mut child, 1)?;
    if fs::read_to_string(work.root.join("todo.txt"))? != LIST {
        return Err("the file changed".into());
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
    let text = screen.text();
    if code != 0 || !text.contains(LEAVE) {
        return Err(format!("exit {code}, {text:?}").into());
    }
    Ok(())
}

/// Mouse reports are asked for unless `mouse` is off, and `:set` changes
/// that at once.
fn mouse_reports_follow_the_option() -> Check {
    const ON: &str = "\x1b[?1002h";
    let work = Work::new()?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for(ON)?;
    for key in b":set nomouse\r" {
        screen.type_key(*key)?;
    }
    screen.wait_for("\x1b[?1006l\x1b[?1002l")?;
    screen.type_key(b'q')?;
    ends_with(&mut child, 0)?;
    let work = Work::new()?;
    let config = work.root.join(".config").join("lsty");
    fs::create_dir_all(&config)?;
    fs::write(config.join("config.toml"), "mouse = false\n")?;
    let (mut child, mut screen) = start(&work.root)?;
    screen.wait_for("NORMAL")?;
    for key in b":set mouse\r" {
        screen.type_key(*key)?;
    }
    screen.wait_for(ON)?;
    screen.type_key(b'q')?;
    ends_with(&mut child, 0)?;
    screen.finish();
    let text = screen.text();
    if text.find("NORMAL") > text.find(ON) {
        return Err(format!("reports asked too early: {text:?}").into());
    }
    Ok(())
}

/// Waits until `path` exists.
fn wait_for_file(path: &Path) -> Check {
    let end = Instant::now() + LIMIT;
    while !path.exists() {
        if Instant::now() > end {
            return Err(format!("no {} in time", path.display()).into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

/// A work folder whose only item links `notes.md`, and an editor that
/// writes `ran` and then waits for `release`.
fn with_held_editor() -> Result<(Work, PathBuf), Failed> {
    use std::os::unix::fs::PermissionsExt as _;
    let work = Work::new()?;
    fs::write(work.root.join("todo.txt"), "Read file:notes.md\n")?;
    fs::write(work.root.join("notes.md"), "hello\n")?;
    let editor = work.root.join("editor.sh");
    fs::write(
        &editor,
        "#!/bin/sh\ntouch \"$(dirname \"$0\")/ran\"\n\
         while [ ! -f \"$(dirname \"$0\")/release\" ]; do sleep 0.02; done\n",
    )?;
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))?;
    Ok((work, editor))
}

/// The text after the second entry of the screen.
fn taken_back(text: &str) -> &str {
    text.split(ENTER).nth(2).unwrap_or_default()
}

/// `gx` on a text file gives the terminal to the editor, and the screen
/// is taken back, drawn in full, when the editor ends. With `interrupt`,
/// SIGINT, which `^C` sends to every process of the terminal's group
/// while the editor has it, reaches lsty too and must not end it (the
/// test has no controlling terminal, so it sends the signal itself).
fn editor_round_trip(interrupt: bool) -> Check {
    let (work, editor) = with_held_editor()?;
    let (mut child, mut screen) =
        start_with(&work.root, &[("EDITOR", &editor)])?;
    screen.wait_for("NORMAL")?;
    screen.type_key(b'g')?;
    screen.type_key(b'x')?;
    wait_for_file(&work.root.join("ran"))?;
    screen.wait_for(LEAVE)?;
    if interrupt {
        signal(&child, "INT")?;
        thread::sleep(Duration::from_millis(300));
        if child.try_wait()?.is_some() {
            return Err(format!("ended: {:?}", screen.text()).into());
        }
    }
    fs::write(work.root.join("release"), "")?;
    // The screen is entered twice: the text after the second entry is
    // the one taken back.
    screen.wait_until("screen taken back", |text| {
        taken_back(text).contains("NORMAL")
    })?;
    if !taken_back(&screen.text()).contains("\x1b[?1002h") {
        return Err("the mouse was not asked for again".into());
    }
    screen.type_key(b'q')?;
    ends_with(&mut child, 0)
}

fn main() -> std::process::ExitCode {
    let trials = vec![
        Trial::test(
            "30.27 a busy list never opens the screen",
            busy_list_never_opens_the_screen,
        ),
        Trial::test(
            "30.45 an end of file before the screen opens",
            eof_before_the_screen_opens,
        ),
        Trial::test("30.29 SIGTERM restores the terminal", || stop_by("TERM")),
        Trial::test("30.30 SIGHUP restores the terminal", || stop_by("HUP")),
        Trial::test("30.31 q restores the terminal", q_quits_and_restores),
        Trial::test(
            "mouse reports follow the option",
            mouse_reports_follow_the_option,
        ),
        // Only Linux opens the master so that no child inherits it.
        Trial::test(
            "a closed terminal ends the program",
            a_closed_terminal_ends_the_program,
        )
        .with_ignored_flag(cfg!(not(target_os = "linux"))),
        Trial::test(
            "a signal during the first reading ends the process",
            signal_during_the_first_read,
        ),
        Trial::test(
            "another writer is shown without a key",
            another_writer_is_shown_without_a_key,
        ),
        Trial::test(
            "a stop signal ends a busy reload",
            stop_during_a_busy_reload,
        ),
        Trial::test("44.38 the editor gets the terminal", || {
            editor_round_trip(false)
        }),
        Trial::test(
            "44.39 SIGINT in the editor does not end the program",
            || editor_round_trip(true),
        ),
    ];
    libtest_mimic::run(&Arguments::from_args(), trials).exit_code()
}
