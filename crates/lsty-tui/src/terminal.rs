//! The real terminal: raw mode, the alternate screen, and crossterm's
//! events turned into the loop's input.

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

/// The terminal of the process, in raw mode on the alternate screen,
/// with mouse reporting on. Dropping it puts the terminal back.
pub struct CrosstermScreen<W: Write> {
    terminal: Terminal<CrosstermBackend<W>>,
    stop: Stop,
    reader: Option<Reader>,
}

/// The thread that reads the terminal's events.
struct Reader {
    over: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Reader {
    fn start(feed: Feed) -> io::Result<Self> {
        let over = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&over);
        let thread = thread::Builder::new()
            .name("terminal".to_owned())
            .spawn(move || read_events(&feed, &flag))?;
        Ok(Self { over, thread })
    }

    fn end(self) {
        self.over.store(true, Ordering::Relaxed);
        // The thread has nothing to report.
        drop(self.thread.join());
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

impl<W: Write> CrosstermScreen<W> {
    /// Takes over the terminal that `out` writes to. SIGHUP and SIGTERM
    /// end the loop like a quit.
    ///
    /// # Errors
    ///
    /// The terminal cannot be switched to raw mode or drawn on.
    pub fn open(out: W) -> io::Result<Self> {
        let stop = Stop::new()?;
        enable_raw_mode()?;
        TAKEN.store(true, Ordering::SeqCst);
        let mut terminal = match Terminal::new(CrosstermBackend::new(out)) {
            Ok(terminal) => terminal,
            Err(e) => {
                // Nothing more can be done when this fails too.
                drop(disable_raw_mode());
                TAKEN.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        if let Err(e) = enter(terminal.backend_mut()) {
            give_back(&TAKEN, terminal.backend_mut());
            return Err(e);
        }
        Ok(Self {
            terminal,
            stop,
            reader: None,
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

impl<W: Write> Drop for CrosstermScreen<W> {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.end();
        }
        give_back(&TAKEN, self.terminal.backend_mut());
    }
}

impl<W: Write> Screen for CrosstermScreen<W> {
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
                model.view(frame.buffer_mut());
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
        self.reader = Some(Reader::start(feed)?);
        Ok(())
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
            c.to_uppercase().next().unwrap_or(c)
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
