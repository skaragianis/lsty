//! The event loop: draws the model, waits for input or a timer, and
//! carries out what the model asks for.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use lsty_core::errs::{Code, EXIT_IO, Error};
use lsty_core::keys::Stroke;

use crate::deps::Deps;
use crate::model::Model;
use crate::msg::{Cmd, Msg};

/// What a terminal gives the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A key was pressed.
    Key(Stroke),
    /// The terminal has a new size, in columns and rows.
    Resize(u16, u16),
    /// Nothing happened within the time asked for.
    Idle,
    /// The terminal went away or the program was asked to stop.
    Stop,
}

/// A terminal as the loop uses it.
pub trait Screen {
    /// The size in columns and rows.
    fn size(&mut self) -> io::Result<(u16, u16)>;

    /// Draws the model.
    fn draw(&mut self, model: &Model) -> io::Result<()>;

    /// Waits for input, at most `wait` when given.
    fn next(&mut self, wait: Option<Duration>) -> io::Result<Input>;
}

impl<S: Screen + ?Sized> Screen for &mut S {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        (**self).size()
    }

    fn draw(&mut self, model: &Model) -> io::Result<()> {
        (**self).draw(model)
    }

    fn next(&mut self, wait: Option<Duration>) -> io::Result<Input> {
        (**self).next(wait)
    }
}

/// Shows the list until the user quits and returns the exit code. The
/// list is read first; `open` takes over the terminal only once that
/// has worked. An error ends the program with its message on `stderr`,
/// written after the terminal is given back.
pub fn run<S: Screen>(
    deps: Deps,
    open: impl FnOnce() -> io::Result<S>,
    stderr: &mut dyn Write,
) -> u8 {
    let mut model = match Model::new(deps) {
        Ok(model) => model,
        Err(e) => return report(&e, stderr),
    };
    let mut screen = match open() {
        Ok(screen) => screen,
        Err(e) => return terminal_error(&e, stderr),
    };
    let result = event_loop(&mut model, &mut screen);
    drop(screen);
    match result {
        Ok(()) => 0,
        Err(e) => terminal_error(&e, stderr),
    }
}

fn terminal_error(e: &io::Error, stderr: &mut dyn Write) -> u8 {
    report(&Error::new(Code::Io, format!("terminal: {e}")), stderr)
}

/// Writes `lsty: message` and the hint to `stderr`, and returns the exit
/// code of the error.
pub fn report(err: &Error, stderr: &mut dyn Write) -> u8 {
    let hint = err.hint().map(|h| format!("hint: {h}\n"));
    let text =
        format!("lsty: {}\n{}", err.message(), hint.unwrap_or_default());
    match stderr.write_all(text.as_bytes()) {
        Ok(()) => err.exit(),
        Err(_) => EXIT_IO,
    }
}

/// Messages that wait for their time.
#[derive(Default)]
struct Timers(Vec<(Instant, Msg)>);

impl Timers {
    fn add(&mut self, delay: Duration, msg: Msg) {
        self.0.push((Instant::now() + delay, msg));
    }

    /// How long until the first timer is due.
    fn wait(&self) -> Option<Duration> {
        let first = self.0.iter().map(|(at, _)| *at).min()?;
        Some(first.saturating_duration_since(Instant::now()))
    }

    /// Takes the messages that are due.
    fn due(&mut self) -> Vec<Msg> {
        let now = Instant::now();
        let (due, later): (Vec<_>, Vec<_>) =
            self.0.drain(..).partition(|(at, _)| *at <= now);
        self.0 = later;
        due.into_iter().map(|(_, msg)| msg).collect()
    }
}

fn event_loop(model: &mut Model, screen: &mut impl Screen) -> io::Result<()> {
    let mut timers = Timers::default();
    let (w, h) = screen.size()?;
    let mut cmds = model.update(Msg::Resize(w, h));
    cmds.extend(model.init());
    loop {
        if apply(cmds, &mut timers) {
            return Ok(());
        }
        screen.draw(model)?;
        let msgs = match screen.next(timers.wait())? {
            Input::Stop => return Ok(()),
            Input::Key(stroke) => vec![Msg::Key(stroke)],
            Input::Resize(w, h) => vec![Msg::Resize(w, h)],
            Input::Idle => timers.due(),
        };
        cmds = msgs.into_iter().flat_map(|m| model.update(m)).collect();
    }
}

/// Carries out the commands; true when the program should end.
fn apply(cmds: Vec<Cmd>, timers: &mut Timers) -> bool {
    let mut quit = false;
    for cmd in cmds {
        match cmd {
            Cmd::Quit => quit = true,
            Cmd::After { delay, msg } => timers.add(delay, msg),
        }
    }
    quit
}
