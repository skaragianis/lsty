//! The event loop: draws the model, waits for input or a timer, and
//! carries out what the model asks for.

use std::{
    ffi::OsString,
    io::{self, Write},
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    thread,
    time::{Duration, Instant},
};

use lsty_core::{
    errs::{self, Code, EXIT_IO, Error},
    keys::Stroke,
    lock::Cancel,
    visible::visible_bytes,
};

use crate::{
    deps::{Deps, Start},
    live::{self, Load},
    model::Model,
    msg::{Cmd, Mouse, Msg},
    terminal::CrosstermScreen,
    watch::Sink,
};

/// What a terminal gives the loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Input {
    /// A key was pressed.
    Key(Stroke),
    /// Text was pasted.
    Paste(String),
    /// The terminal has a new size, in columns and rows.
    Resize(u16, u16),
    /// The mouse did something.
    Mouse(Mouse),
    /// The terminal went away or the program was asked to stop.
    Stop,
}

/// Everything the loop waits for arrives on one channel.
#[derive(Debug)]
pub(crate) enum Event {
    Input(Input),
    Msg(Msg),
    Failed(io::Error),
}

/// Where a terminal hands its input to the loop.
#[derive(Clone, Debug)]
pub(crate) struct Feed(Sender<Event>);

impl Feed {
    pub(crate) const fn new(tx: Sender<Event>) -> Self {
        Self(tx)
    }

    /// Hands over an input. False when the loop has ended.
    pub(crate) fn send(&self, input: Input) -> bool {
        self.0.send(Event::Input(input)).is_ok()
    }

    /// Tells the loop that the terminal can no longer be read.
    pub(crate) fn fail(&self, err: io::Error) {
        // Nothing is left to tell when the loop has ended.
        drop(self.0.send(Event::Failed(err)));
    }
}

/// A terminal as the loop uses it.
pub(crate) trait Screen {
    /// The size in columns and rows.
    fn size(&mut self) -> io::Result<(u16, u16)>;

    /// Draws the model.
    fn draw(&mut self, model: &Model) -> io::Result<()>;

    /// Starts handing input to `feed`, without blocking.
    fn start(&mut self, feed: Feed) -> io::Result<()>;

    /// Puts `text` on the system clipboard. The default does nothing.
    fn copy(&mut self, _text: &[u8]) {}

    /// Gives the terminal back to the user's shell for a program that
    /// runs on it, and stops reading input. The default does nothing.
    ///
    /// # Errors
    ///
    /// The terminal cannot be reset.
    fn suspend(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Takes the terminal back after [`Screen::suspend`], draws the
    /// screen again from scratch and reads input again. The default
    /// does nothing.
    ///
    /// # Errors
    ///
    /// The terminal cannot be taken back.
    fn resume(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// A token that is cancelled when the program is asked to stop, so
    /// that a wait for a lock ends at once. The default is never
    /// cancelled.
    fn cancel(&self) -> Cancel {
        Cancel::new()
    }
}

/// Shows the list until the user quits and returns the exit code. The
/// list is read first; `open` takes over the terminal only once that
/// has worked. An error ends the program with its message on `stderr`,
/// written after the terminal is given back.
pub fn run(
    start: Start,
    deps: Deps,
    open: impl FnOnce() -> io::Result<CrosstermScreen>,
    stderr: &mut dyn Write,
) -> u8 {
    run_with(start, deps, open, stderr)
}

/// [`run`] on any terminal.
pub(crate) fn run_with<S: Screen>(
    start: Start,
    deps: Deps,
    open: impl FnOnce() -> io::Result<S>,
    stderr: &mut dyn Write,
) -> u8 {
    let (tx, rx) = channel();
    let sink: Sink = {
        let tx = tx.clone();
        Arc::new(move |notice| {
            // Nothing is left to tell when the loop has ended.
            drop(tx.send(Event::Msg(Msg::Notice(notice))));
        })
    };
    let mut model = match Model::with_sink(start, deps, sink) {
        Ok(model) => model,
        Err(e) => return report(&e, stderr),
    };
    let mut screen = match open() {
        Ok(screen) => screen,
        Err(e) => return terminal_error(&e, stderr),
    };
    model.cancel = screen.cancel();
    let result = event_loop(&mut model, &mut screen, &tx, &rx);
    // A reading that still waits for a lock is not waited for.
    model.cancel.cancel();
    drop(screen);
    match result {
        Ok(()) => 0,
        Err(e) => terminal_error(&e, stderr),
    }
}

fn terminal_error(e: &io::Error, stderr: &mut dyn Write) -> u8 {
    report(
        &Error::new(Code::Io, format!("terminal: {}", errs::io_text(e))),
        stderr,
    )
}

/// Writes `lsty: message` and the hint to `stderr`, and returns the exit
/// code of the error.
pub fn report(err: &Error, stderr: &mut dyn Write) -> u8 {
    let mut text = b"lsty: ".to_vec();
    text.extend_from_slice(&visible_bytes(err.message().as_bytes()));
    text.push(b'\n');
    if let Some(hint) = err.hint() {
        text.extend_from_slice(b"hint: ");
        text.extend_from_slice(&visible_bytes(hint.as_bytes()));
        text.push(b'\n');
    }
    stderr.write_all(&text).map_or(EXIT_IO, |()| err.exit())
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

/// What a step of the loop made of what came.
enum Step {
    Cmds(Vec<Cmd>),
    Stop,
}

fn event_loop(
    model: &mut Model,
    screen: &mut impl Screen,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
) -> io::Result<()> {
    let mut timers = Timers::default();
    screen.start(Feed::new(tx.clone()))?;
    let (w, h) = screen.size()?;
    let mut cmds = model.update(Msg::Resize(w, h));
    cmds.extend(model.init());
    loop {
        if apply(cmds, &mut timers, tx, &*model, &mut *screen) {
            return Ok(());
        }
        screen.draw(model)?;
        match step(model, &mut timers, rx)? {
            Step::Cmds(next) => cmds = next,
            Step::Stop => return Ok(()),
        }
    }
}

/// Waits for the next event or the next timer and hands it to the model.
fn step(
    model: &mut Model,
    timers: &mut Timers,
    rx: &Receiver<Event>,
) -> io::Result<Step> {
    let due = timers.due();
    if !due.is_empty() {
        return Ok(Step::Cmds(update_all(model, due)));
    }
    let event = match timers.wait() {
        Some(wait) => match rx.recv_timeout(wait) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => {
                return Ok(Step::Cmds(update_all(model, timers.due())));
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(Step::Stop),
        },
        None => match rx.recv() {
            Ok(event) => event,
            Err(_) => return Ok(Step::Stop),
        },
    };
    let msg = match event {
        Event::Input(Input::Stop) => return Ok(Step::Stop),
        Event::Input(Input::Key(stroke)) => Msg::Key(stroke),
        Event::Input(Input::Paste(text)) => Msg::Paste(text),
        Event::Input(Input::Resize(w, h)) => Msg::Resize(w, h),
        Event::Input(Input::Mouse(event)) => Msg::Mouse(event),
        Event::Msg(msg) => msg,
        Event::Failed(e) => return Err(e),
    };
    Ok(Step::Cmds(model.update(msg)))
}

fn update_all(model: &mut Model, msgs: Vec<Msg>) -> Vec<Cmd> {
    msgs.into_iter().flat_map(|m| model.update(m)).collect()
}

/// Carries out the commands; true when the program should end.
fn apply(
    cmds: Vec<Cmd>,
    timers: &mut Timers,
    tx: &Sender<Event>,
    model: &Model,
    screen: &mut impl Screen,
) -> bool {
    let mut quit = false;
    for cmd in cmds {
        match cmd {
            Cmd::Quit => quit = true,
            Cmd::After { delay, msg } => timers.add(delay, msg),
            Cmd::Copy(text) => screen.copy(&text),
            Cmd::Load(load) => read_in_background(load, tx),
            Cmd::Edit(words) => edit(&words, model, screen, tx),
        }
    }
    quit
}

/// Reads on a thread of its own and sends the result to the loop. When
/// no thread can be had the reading fails like any other and is owed
/// again.
fn read_in_background(load: Box<Load>, tx: &Sender<Event>) {
    let (serial, clock) = (load.serial, load.clock.clone());
    let sender = tx.clone();
    let spawned =
        thread::Builder::new()
            .name("reload".to_owned())
            .spawn(move || {
                // Nothing is left to tell when the loop has ended.
                drop(sender.send(Event::Msg(load.read())));
            });
    if let Err(e) = spawned {
        let why =
            Error::new(Code::Io, format!("reload: {}", errs::io_text(&e)));
        // Nothing is left to tell when the loop has ended.
        drop(tx.send(Event::Msg(live::answer(serial, clock, Err(why)))));
    }
}

/// Hands the terminal to the editor and waits for it. The model hears
/// how it ended and the size the terminal has now.
fn edit(
    words: &[OsString],
    model: &Model,
    screen: &mut impl Screen,
    tx: &Sender<Event>,
) {
    let result = match screen.suspend() {
        Ok(()) => (model.deps.run_editor)(words),
        Err(e) => Err(format!("terminal: {}", errs::io_text(&e))),
    };
    let back = screen.resume();
    // Nothing is left to tell when the loop has ended.
    drop(tx.send(Event::Msg(Msg::Edited(result))));
    match back.and_then(|()| screen.size()) {
        Ok((w, h)) => drop(tx.send(Event::Msg(Msg::Resize(w, h)))),
        Err(e) => drop(tx.send(Event::Failed(e))),
    }
}
