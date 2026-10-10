use std::{
    sync::{atomic::Ordering, mpsc::channel},
    time::Duration,
};

use signal_hook::{
    consts::{SIGHUP, SIGTERM},
    low_level::raise,
};

use crate::{
    run::{Event, Feed, Input},
    signals::{Stop, default_action},
};

const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn a_stop_signal_is_told_and_cancels_the_wait_for_a_lock() {
    // Tests share the process: the flag checks run one signal after the
    // other, never together.
    for signal in [SIGHUP, SIGTERM] {
        assert!(default_action().load(Ordering::SeqCst));
        let mut stop = Stop::new().unwrap_or_else(|e| panic!("{e}"));
        let (tx, rx) = channel();
        stop.start(Feed::new(tx)).unwrap_or_else(|e| panic!("{e}"));
        let cancel = stop.cancel();
        assert!(!cancel.is_cancelled());
        raise(signal).unwrap_or_else(|e| panic!("{e}"));
        let got = rx.recv_timeout(PATIENCE);
        assert!(
            matches!(got, Ok(Event::Input(Input::Stop))),
            "signal {signal}"
        );
        assert!(cancel.is_cancelled(), "signal {signal}");
        assert!(!default_action().load(Ordering::SeqCst));
        drop(stop);
        assert!(default_action().load(Ordering::SeqCst));
    }
}
