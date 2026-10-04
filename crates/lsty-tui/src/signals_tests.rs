use std::sync::mpsc::channel;
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::low_level::raise;

use crate::run::{Event, Feed, Input};
use crate::signals::Stop;

const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn a_stop_signal_is_told_and_nothing_else_is() {
    for signal in [SIGHUP, SIGTERM] {
        let mut stop = Stop::new().unwrap_or_else(|e| panic!("{e}"));
        let (tx, rx) = channel();
        stop.start(Feed::new(tx)).unwrap_or_else(|e| panic!("{e}"));
        raise(signal).unwrap_or_else(|e| panic!("{e}"));
        let got = rx.recv_timeout(PATIENCE);
        assert!(
            matches!(got, Ok(Event::Input(Input::Stop))),
            "signal {signal}"
        );
    }
}

#[test]
fn a_stop_signal_cancels_the_wait_for_a_lock() {
    let mut stop = Stop::new().unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = channel();
    stop.start(Feed::new(tx)).unwrap_or_else(|e| panic!("{e}"));
    let cancel = stop.cancel();
    assert!(!cancel.is_cancelled());
    raise(SIGHUP).unwrap_or_else(|e| panic!("{e}"));
    assert!(rx.recv_timeout(PATIENCE).is_ok());
    assert!(cancel.is_cancelled());
}
