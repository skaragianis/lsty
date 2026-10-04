//! The real terminal: raw mode, the alternate screen, and crossterm's
//! events turned into the loop's input.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use lsty_core::keys::{Mods, Named, Stroke};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::model::Model;
use crate::run::{Input, Screen};
use crate::signals::Stop;

/// Alternate screen, cell-motion mouse reporting with SGR coordinates,
/// bracketed paste, hidden cursor.
pub(crate) const ENTER: &[u8] =
    b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?25l";
/// Undoes [`ENTER`], in reverse.
pub(crate) const LEAVE: &[u8] =
    b"\x1b[?25h\x1b[?2004l\x1b[?1006l\x1b[?1002l\x1b[?1049l";

/// The longest a wait lasts before the signals are looked at again.
pub(crate) const SLICE: Duration = Duration::from_millis(100);

/// The terminal of the process, in raw mode on the alternate screen,
/// with mouse reporting on. Dropping it puts the terminal back.
pub struct CrosstermScreen<W: Write> {
    terminal: Terminal<CrosstermBackend<W>>,
    stop: Stop,
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
        let mut terminal = match Terminal::new(CrosstermBackend::new(out)) {
            Ok(terminal) => terminal,
            Err(e) => {
                // Nothing more can be done when this fails too.
                drop(disable_raw_mode());
                return Err(e);
            }
        };
        if let Err(e) = enter(terminal.backend_mut()) {
            restore(terminal.backend_mut());
            return Err(e);
        }
        Ok(Self { terminal, stop })
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

impl<W: Write> Drop for CrosstermScreen<W> {
    fn drop(&mut self) {
        restore(self.terminal.backend_mut());
    }
}

impl<W: Write> Screen for CrosstermScreen<W> {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        let size = self.terminal.size()?;
        Ok((size.width, size.height))
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

    fn next(&mut self, wait: Option<Duration>) -> io::Result<Input> {
        let deadline = wait.map(|w| Instant::now() + w);
        loop {
            if self.stop.asked() {
                return Ok(Input::Stop);
            }
            if event::poll(slice(deadline, Instant::now()))?
                && let Some(input) = input_of(&event::read()?)
            {
                return Ok(input);
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(Input::Idle);
            }
        }
    }
}

/// How long to poll next: what is left until `deadline`, but not more
/// than [`SLICE`].
pub(crate) fn slice(deadline: Option<Instant>, now: Instant) -> Duration {
    deadline.map_or(SLICE, |d| d.saturating_duration_since(now).min(SLICE))
}

/// What the loop makes of an event; mouse, focus and paste events are not
/// used yet.
fn input_of(event: &Event) -> Option<Input> {
    match event {
        Event::Key(key) => stroke_of(*key).map(Input::Key),
        Event::Resize(w, h) => Some(Input::Resize(*w, *h)),
        Event::FocusGained
        | Event::FocusLost
        | Event::Mouse(_)
        | Event::Paste(_) => None,
    }
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
