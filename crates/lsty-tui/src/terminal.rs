//! The real terminal: raw mode, the alternate screen, and crossterm's
//! events turned into the loop's input.

use std::fs::File;
use std::io::{self, IsTerminal as _, Write};
use std::os::fd::{AsFd as _, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lsty_core::case::simple_upper;
use lsty_core::clip::osc52;
use lsty_core::keys::{Mods, Named, Stroke};
use lsty_core::lock::Cancel;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::Rect;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use signal_hook::SigId;
use signal_hook::consts::{SIGHUP, SIGINT};

use crate::model::Model;
use crate::msg::{Button, Gesture, Mouse};
use crate::run::{Feed, Input, Screen};
use crate::signals::Stop;

/// Alternate screen, cell-motion mouse reporting with SGR coordinates,
/// bracketed paste, hidden cursor.
pub(crate) const ENTER: &[u8] =
    b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?25l";
/// Undoes [`ENTER`], in reverse.
pub(crate) const LEAVE: &[u8] =
    b"\x1b[?25h\x1b[?2004l\x1b[?1006l\x1b[?1002l\x1b[?1049l";

/// Whether a screen holds the process's terminal, from raw mode on;
/// see [`restore_after_panic`].
static TAKEN: AtomicBool = AtomicBool::new(false);

/// The longest the reader waits for input before it looks whether it
/// is to end.
pub(crate) const SLICE: Duration = Duration::from_millis(100);
const SLICE_POLL: Timespec = Timespec {
    tv_sec: SLICE.as_secs().cast_signed(),
    tv_nsec: SLICE.subsec_nanos() as _,
};

/// The longest ending the reader waits for its thread. Once the terminal
/// is gone, crossterm's read can keep reading nothing without returning;
/// the thread is then left to end with the process.
const END_WAIT: Duration = Duration::from_millis(500);

/// The terminal of the process, in raw mode on the alternate screen,
/// with mouse reporting on. Dropping it puts the terminal back.
pub struct CrosstermScreen {
    terminal: Terminal<CrosstermBackend<Out>>,
    stop: Stop,
    reader: Option<Reader>,
    /// Where the reader hands input to, once it has started.
    feed: Option<Feed>,
    /// The handler that keeps `^C` from ending us while a program owns
    /// the terminal.
    interrupt: Option<SigId>,
    /// The terminal crossterm reads, picked while it is open: once it
    /// hangs up it no longer reads as a terminal.
    tty: OwnedFd,
}

/// The thread that reads the terminal's events, and the one that
/// watches for the terminal to go away.
struct Reader {
    over: Arc<AtomicBool>,
    thread: JoinHandle<()>,
    /// Disconnected once `thread` has ended.
    ended: Receiver<()>,
    watch: JoinHandle<()>,
}

impl Reader {
    fn start(tty: OwnedFd, feed: Feed) -> io::Result<Self> {
        let over = Arc::new(AtomicBool::new(false));
        let (alive, ended) = mpsc::channel::<()>();
        let flag = Arc::clone(&over);
        let thread = thread::Builder::new()
            .name("terminal".to_owned())
            .spawn(move || {
                let _alive = alive;
                read_events(&feed, &flag);
            })?;
        let flag = Arc::clone(&over);
        let watch = thread::Builder::new()
            .name("hangup".to_owned())
            .spawn(move || watch_hangup(&tty, &flag))
            .inspect_err(|_| over.store(true, Ordering::Relaxed))?;
        Ok(Self {
            over,
            thread,
            ended,
            watch,
        })
    }

    fn end(self) {
        self.over.store(true, Ordering::Relaxed);
        // The threads have nothing to report.
        drop(self.watch.join());
        match self.ended.recv_timeout(END_WAIT) {
            Err(RecvTimeoutError::Disconnected) => drop(self.thread.join()),
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Stops the loop as SIGHUP does once the terminal hangs up: a terminal
/// that is not the controlling one sends no signal when it goes, and
/// crossterm reads on without telling. `poll` reports a hangup even for
/// no events, so this never takes input from the reader.
fn watch_hangup(tty: &OwnedFd, over: &AtomicBool) {
    while !over.load(Ordering::Relaxed) {
        let mut fds = [PollFd::new(tty, PollFlags::empty())];
        match poll(&mut fds, Some(&SLICE_POLL)) {
            Ok(0) | Err(Errno::INTR) => {}
            Ok(_) => {
                let [fd] = &fds;
                if fd.revents().intersects(PollFlags::HUP | PollFlags::ERR) {
                    drop(signal_hook::low_level::raise(SIGHUP));
                }
                // Otherwise the system cannot poll the terminal (POLLNVAL),
                // and there is nothing to watch.
                return;
            }
            Err(_) => return,
        }
    }
}

/// The terminal crossterm reads: standard input if it is one, else
/// `/dev/tty`.
fn input_terminal() -> io::Result<OwnedFd> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        stdin.as_fd().try_clone_to_owned()
    } else {
        File::open("/dev/tty").map(OwnedFd::from)
    }
}

/// Hands the terminal's events to `feed` until `over` is set or the
/// loop has ended.
fn read_events(feed: &Feed, over: &AtomicBool) {
    while !over.load(Ordering::Relaxed) {
        match event::poll(SLICE) {
            Ok(false) => {}
            Ok(true) => match event::read() {
                Ok(event) => {
                    if let Some(input) = input_of(&event)
                        && !feed.send(input)
                    {
                        return;
                    }
                }
                Err(e) => return feed.fail(e),
            },
            Err(e) => return feed.fail(e),
        }
    }
}

impl CrosstermScreen {
    fn start_reader(&mut self) -> io::Result<()> {
        if let Some(feed) = self.feed.clone() {
            let tty = self.tty.try_clone()?;
            self.reader = Some(Reader::start(tty, feed)?);
        }
        Ok(())
    }

    /// Takes over the terminal on stdout. SIGHUP and SIGTERM
    /// end the loop like a quit.
    ///
    /// # Errors
    ///
    /// The terminal cannot be found, switched to raw mode or drawn on.
    pub fn open() -> io::Result<Self> {
        let tty = input_terminal()?;
        let stop = Stop::new()?;
        enable_raw_mode()?;
        TAKEN.store(true, Ordering::SeqCst);
        // crossterm picks the terminal it reads on its first poll; make
        // that now too, not on the reader thread once it may be gone. In
        // raw mode only: a cooked terminal with a pending end of file
        // would block the poll.
        let mut terminal = event::poll(Duration::ZERO)
            .and_then(|_| {
                Terminal::new(CrosstermBackend::new(Out(io::stdout())))
            })
            .inspect_err(|_| {
                // Nothing more can be done when this fails too.
                drop(disable_raw_mode());
                TAKEN.store(false, Ordering::SeqCst);
            })?;
        enter(terminal.backend_mut())
            .inspect_err(|_| give_back(&TAKEN, terminal.backend_mut()))?;
        Ok(Self {
            terminal,
            stop,
            reader: None,
            feed: None,
            interrupt: None,
            tty,
        })
    }
}

fn enter(out: &mut impl Write) -> io::Result<()> {
    out.write_all(ENTER)?;
    out.flush()
}

/// Gives the terminal back; errors cannot be dealt with here.
fn restore(out: &mut impl Write) {
    drop(disable_raw_mode());
    drop(out.write_all(LEAVE));
    drop(out.flush());
}

/// Gives the terminal back if a screen holds it. A release build aborts
/// on a panic without dropping the screen, so the binary's panic hook
/// calls this; the screen is assumed to draw on standard output.
pub fn restore_after_panic() {
    give_back(&TAKEN, &mut io::stdout());
}

/// Restores the terminal through `out` once, if `taken` is set.
pub(crate) fn give_back(taken: &AtomicBool, out: &mut impl Write) {
    if taken.swap(false, Ordering::SeqCst) {
        restore(out);
    }
}

impl Drop for CrosstermScreen {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.end();
        }
        give_back(&TAKEN, self.terminal.backend_mut());
    }
}

/// The screen's output. While the screen does not hold the terminal, a
/// failed write is as good as done: nothing is left to tell about it.
/// Dropping the terminal shows the cursor again; on a terminal that has
/// gone away the write fails, and ratatui then prints to standard error,
/// which panics when that is the same terminal.
struct Out(io::Stdout);

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.write(buf) {
            Err(_) if !TAKEN.load(Ordering::SeqCst) => Ok(buf.len()),
            other => other,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.flush() {
            Err(_) if !TAKEN.load(Ordering::SeqCst) => Ok(()),
            other => other,
        }
    }
}

impl Screen for CrosstermScreen {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        let size = self.terminal.size()?;
        Ok((size.width, size.height))
    }

    fn cancel(&self) -> Cancel {
        self.stop.cancel()
    }

    fn draw(&mut self, model: &Model) -> io::Result<()> {
        self.terminal
            .draw(|frame| {
                model.draw(frame.buffer_mut());
                if let Some(at) = model.cursor_at() {
                    frame.set_cursor_position(at);
                }
            })
            .map(drop)
    }

    fn copy(&mut self, text: &[u8]) -> io::Result<()> {
        let out = self.terminal.backend_mut();
        out.write_all(&osc52(text))?;
        out.flush()
    }

    fn start(&mut self, feed: Feed) -> io::Result<()> {
        self.stop.start(feed.clone())?;
        self.feed = Some(feed);
        self.start_reader()
    }

    fn suspend(&mut self) -> io::Result<()> {
        if let Some(reader) = self.reader.take() {
            reader.end();
        }
        // The program that runs now owns the terminal: a panic must not
        // restore it under that program.
        give_back(&TAKEN, self.terminal.backend_mut());
        // The terminal is cooked now, so `^C` signals the whole group.
        // A handler, not SIG_IGN: the program starts with the default.
        let flag = Arc::new(AtomicBool::new(false));
        self.interrupt = Some(signal_hook::flag::register(SIGINT, flag)?);
        Ok(())
    }

    fn resume(&mut self) -> io::Result<()> {
        if let Some(id) = self.interrupt.take() {
            signal_hook::low_level::unregister(id);
        }
        enable_raw_mode()?;
        TAKEN.store(true, Ordering::SeqCst);
        enter(self.terminal.backend_mut())?;
        // `Terminal::clear` asks the terminal where the cursor is, and
        // nothing reads the answer yet; resizing redraws everything too.
        let size = self.terminal.size()?;
        self.terminal
            .resize(Rect::new(0, 0, size.width, size.height))?;
        self.start_reader()
    }
}

/// What the loop makes of an event; focus events are not used, nor is
/// a mouse that moves with no button held.
pub(crate) fn input_of(event: &Event) -> Option<Input> {
    match event {
        Event::Key(key) => stroke_of(*key).map(Input::Key),
        Event::Resize(w, h) => Some(Input::Resize(*w, *h)),
        Event::Paste(text) => Some(Input::Paste(text.clone())),
        Event::Mouse(mouse) => mouse_of(*mouse).map(Input::Mouse),
        Event::FocusGained | Event::FocusLost => None,
    }
}

fn button_of(button: MouseButton) -> Button {
    match button {
        MouseButton::Left => Button::Left,
        MouseButton::Right | MouseButton::Middle => Button::Other,
    }
}

/// The mouse event of crossterm's, when it is one of the kinds that do
/// anything: a button going down or up, a drag, a turn of the wheel.
pub(crate) fn mouse_of(event: MouseEvent) -> Option<Mouse> {
    let gesture = match event.kind {
        MouseEventKind::Down(b) => Gesture::Down(button_of(b)),
        MouseEventKind::Up(b) => Gesture::Up(button_of(b)),
        MouseEventKind::Drag(b) => Gesture::Drag(button_of(b)),
        MouseEventKind::ScrollUp => Gesture::Wheel { up: true },
        MouseEventKind::ScrollDown => Gesture::Wheel { up: false },
        MouseEventKind::Moved
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => return None,
    };
    Some(Mouse {
        gesture,
        x: event.column,
        y: event.row,
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
    })
}

/// The key of a key press, in the notation of the key bindings. A
/// release is no key.
pub(crate) fn stroke_of(key: KeyEvent) -> Option<Stroke> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let held = |m| key.modifiers.contains(m);
    let mut mods = Mods::NONE;
    if held(KeyModifiers::CONTROL) {
        mods = mods.with(Mods::CTRL);
    }
    if held(KeyModifiers::ALT) {
        mods = mods.with(Mods::ALT);
    }
    if let KeyCode::Char(c) = key.code {
        let c = if held(KeyModifiers::SHIFT) {
            simple_upper(c)
        } else {
            c
        };
        return Some(Stroke::char(c).with_mods(mods));
    }
    let shift = held(KeyModifiers::SHIFT) || key.code == KeyCode::BackTab;
    if shift {
        mods = mods.with(Mods::SHIFT);
    }
    named(key.code).map(|n| Stroke::named(n).with_mods(mods))
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "the other keys have no name in the notation"
)]
fn named(code: KeyCode) -> Option<Named> {
    match code {
        KeyCode::Enter => Some(Named::Cr),
        KeyCode::Esc => Some(Named::Esc),
        KeyCode::Tab | KeyCode::BackTab => Some(Named::Tab),
        KeyCode::Backspace => Some(Named::Bs),
        KeyCode::Delete => Some(Named::Del),
        KeyCode::Up => Some(Named::Up),
        KeyCode::Down => Some(Named::Down),
        KeyCode::Left => Some(Named::Left),
        KeyCode::Right => Some(Named::Right),
        KeyCode::Home => Some(Named::Home),
        KeyCode::End => Some(Named::End),
        KeyCode::PageUp => Some(Named::PageUp),
        KeyCode::PageDown => Some(Named::PageDown),
        KeyCode::F(n) => Named::function(n),
        _ => None,
    }
}
