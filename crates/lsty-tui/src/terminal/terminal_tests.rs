use std::{
    fs::File,
    io::{self, Write},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, atomic::Ordering, mpsc::channel,
    },
};

use lsty_core::keys::{Mods, Named, Stroke};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};

use super::{
    ENTER, LEAVE, MOUSE_OFF, MOUSE_ON, Out, Reader, TAKEN, give_back,
    input_of, stroke_of,
};
use crate::run::Feed;

static SERIAL: Mutex<()> = Mutex::new(());

/// Clears whether a screen holds the terminal, for the test that keeps
/// the guard: the flag is shared by the whole process.
fn holding() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    TAKEN.store(false, Ordering::SeqCst);
    guard
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

#[test]
fn strokes_of_key_presses() {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    let rows = [
        (key(KeyCode::Char('j'), NONE), Stroke::char('j')),
        (
            key(KeyCode::Char('g'), KeyModifiers::SHIFT),
            Stroke::char('G'),
        ),
        (
            key(KeyCode::Char('G'), KeyModifiers::SHIFT),
            Stroke::char('G'),
        ),
        // The simple upper case, as `<S-…>` in a key file: no letter
        // turns into two characters.
        (
            key(KeyCode::Char('\u{DF}'), KeyModifiers::SHIFT),
            Stroke::char('\u{DF}'),
        ),
        (
            key(KeyCode::Char('\u{1F80}'), KeyModifiers::SHIFT),
            Stroke::char('\u{1F88}'),
        ),
        (key(KeyCode::Char(' '), NONE), Stroke::char(' ')),
        (
            key(KeyCode::Char('d'), KeyModifiers::CONTROL),
            Stroke::char('d').with_mods(Mods::CTRL),
        ),
        (
            key(KeyCode::Char('x'), KeyModifiers::ALT),
            Stroke::char('x').with_mods(Mods::ALT),
        ),
        (key(KeyCode::Enter, NONE), Stroke::named(Named::Cr)),
        (key(KeyCode::Esc, NONE), Stroke::named(Named::Esc)),
        (
            key(KeyCode::Tab, KeyModifiers::SHIFT),
            Stroke::named(Named::Tab).with_mods(Mods::SHIFT),
        ),
        (
            key(KeyCode::BackTab, KeyModifiers::SHIFT),
            Stroke::named(Named::Tab).with_mods(Mods::SHIFT),
        ),
        (key(KeyCode::Backspace, NONE), Stroke::named(Named::Bs)),
        (key(KeyCode::Delete, NONE), Stroke::named(Named::Del)),
        (key(KeyCode::Up, NONE), Stroke::named(Named::Up)),
        (key(KeyCode::Down, NONE), Stroke::named(Named::Down)),
        (key(KeyCode::Left, NONE), Stroke::named(Named::Left)),
        (key(KeyCode::Right, NONE), Stroke::named(Named::Right)),
        (key(KeyCode::Home, NONE), Stroke::named(Named::Home)),
        (key(KeyCode::End, NONE), Stroke::named(Named::End)),
        (key(KeyCode::PageUp, NONE), Stroke::named(Named::PageUp)),
        (key(KeyCode::PageDown, NONE), Stroke::named(Named::PageDown)),
        (
            key(KeyCode::F(5), NONE),
            Stroke::named(Named::function(5).unwrap_or(Named::Esc)),
        ),
        (key(KeyCode::Char('é'), NONE), Stroke::char('é')),
    ];
    for (event, want) in rows {
        assert_eq!(stroke_of(event), Some(want), "{event:?}");
    }
}

#[test]
fn a_release_and_an_unnamed_key_are_no_stroke() {
    let release = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    };
    assert_eq!(stroke_of(release), None);
    assert_eq!(stroke_of(key(KeyCode::Insert, KeyModifiers::NONE)), None);
}

#[test]
fn entering_and_leaving_write_mirrored_codes() {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    for code in ["1049", "2004", "25"] {
        assert!(text(ENTER).contains(&format!("?{code}")), "{code}");
        assert!(text(LEAVE).contains(&format!("?{code}")), "{code}");
    }
    assert!(!text(ENTER).contains("100"), "no mouse unless asked");
    assert!(text(MOUSE_ON).contains("?1002h"));
    assert!(!text(MOUSE_ON).contains("1003"), "no all-motion reporting");
    assert!(text(LEAVE).contains("?1002l") && text(LEAVE).contains("?1006l"));
    assert!(text(MOUSE_OFF).contains("?1002l"));
    assert!(text(ENTER).contains("?25l") && text(LEAVE).contains("?25h"));
}

#[test]
fn the_terminal_is_given_back_once_and_only_when_taken() {
    let _held = holding();
    let mut out = Vec::new();
    give_back(&mut out);
    assert!(out.is_empty(), "nothing to give back");
    TAKEN.store(true, Ordering::SeqCst);
    give_back(&mut out);
    assert_eq!(out, LEAVE);
    give_back(&mut out);
    assert_eq!(out, LEAVE, "a second call writes nothing");
}

#[test]
fn a_paste_arrives_as_one_input() {
    use crate::run::Input;

    let event = Event::Paste("one\ntwo".to_owned());
    assert_eq!(input_of(&event), Some(Input::Paste("one\ntwo".to_owned())));
}

/// A terminal that has gone away: every write fails.
struct Gone;

impl Write for Gone {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("gone"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("gone"))
    }
}

#[test]
fn a_write_that_fails_is_told_only_while_a_screen_holds_the_terminal() {
    let _held = holding();
    let mut out = Out(Gone);
    assert_eq!(out.write(b"abc").ok(), Some(3));
    assert!(out.flush().is_ok());
    TAKEN.store(true, Ordering::SeqCst);
    assert!(out.write(b"abc").is_err());
    assert!(out.flush().is_err());
}

#[test]
fn ending_the_reader_tells_its_threads_to_end() {
    let (tx, _rx) = channel();
    let tty = File::open("/dev/null").unwrap_or_else(|e| panic!("{e}"));
    let reader = Reader::start(tty.into(), Feed::new(tx))
        .unwrap_or_else(|e| panic!("{e}"));
    let over = Arc::clone(&reader.over);
    assert!(!over.load(Ordering::Relaxed));
    reader.end();
    assert!(over.load(Ordering::Relaxed));
}
