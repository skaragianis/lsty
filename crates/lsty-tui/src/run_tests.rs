use std::cell::Cell;
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::keys::Stroke;
use lsty_core::lock::{Cancel, Mode, Want, acquire};

use crate::test_util::{SAMPLE, deps, workdir};
use crate::{Input, Model, Screen, run};

/// A terminal that plays the inputs it is given.
struct Script {
    size: (u16, u16),
    inputs: VecDeque<Input>,
    drawn: usize,
    waits: Vec<Option<Duration>>,
}

impl Script {
    fn new(inputs: impl IntoIterator<Item = Input>) -> Self {
        Self {
            size: (80, 24),
            inputs: inputs.into_iter().collect(),
            drawn: 0,
            waits: Vec::new(),
        }
    }
}

impl Screen for Script {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        Ok(self.size)
    }

    fn draw(&mut self, _: &Model) -> io::Result<()> {
        self.drawn += 1;
        Ok(())
    }

    fn next(&mut self, wait: Option<Duration>) -> io::Result<Input> {
        self.waits.push(wait);
        Ok(self.inputs.pop_front().unwrap_or(Input::Stop))
    }
}

fn key(c: char) -> Input {
    Input::Key(Stroke::char(c))
}

#[test]
fn q_ends_the_loop_with_code_0() {
    let dir = workdir(SAMPLE);
    let mut screen = Script::new([key('j'), key('q'), key('j')]);
    let mut stderr = Vec::new();
    let code = run(
        deps(dir.path(), Config::default()),
        || Ok(&mut screen),
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stderr), "");
    assert_eq!(screen.inputs.len(), 1, "stopped at q");
    assert_eq!(screen.drawn, 2, "drawn before each wait");
}

#[test]
fn the_loop_waits_for_the_minute_timer() {
    let dir = workdir(SAMPLE);
    let mut screen = Script::new([Input::Resize(60, 10), key('q')]);
    let mut stderr = Vec::new();
    run(
        deps(dir.path(), Config::default()),
        || Ok(&mut screen),
        &mut stderr,
    );
    let first = screen.waits.first().copied().flatten();
    assert!(first.is_some_and(|w| w <= Duration::from_secs(60)));
    assert!(first.is_some_and(|w| w > Duration::from_secs(50)));
}

#[test]
fn an_idle_wait_delivers_the_timer_that_is_due() {
    let dir = workdir(SAMPLE);
    let mut screen = Script::new([Input::Idle, key('q')]);
    let mut stderr = Vec::new();
    let code = run(
        deps(dir.path(), Config::default()),
        || Ok(&mut screen),
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(screen.waits.len(), 2);
}

#[test]
fn a_busy_lock_ends_the_run_with_its_error() {
    let dir = workdir(SAMPLE);
    let mut d = deps(dir.path(), Config::default());
    d.config.lock_timeout = Duration::from_millis(50);
    let want = Want {
        file: d.found.todo.clone(),
        mode: Mode::Exclusive,
    };
    let held = acquire(&[want], Duration::from_secs(1), &Cancel::new());
    let mut held = held.unwrap_or_else(|e| panic!("{}", e.message()));
    let opened = Cell::new(false);
    let mut stderr = Vec::new();
    let code = run(
        d,
        || {
            opened.set(true);
            Ok(Script::new([]))
        },
        &mut stderr,
    );
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    assert_eq!(code, 4);
    let text = String::from_utf8_lossy(&stderr);
    assert_eq!(text, "lsty: todo.txt is busy (locked by another writer)\n");
    assert!(!opened.get(), "the terminal is not taken over");
}

#[test]
fn a_screen_that_cannot_be_opened_ends_the_run() {
    let dir = workdir(SAMPLE);
    let mut stderr = Vec::new();
    let code = run::<Script>(
        deps(dir.path(), Config::default()),
        || Err(io::Error::other("no terminal")),
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert_eq!(
        String::from_utf8_lossy(&stderr),
        "lsty: terminal: no terminal\n"
    );
}
