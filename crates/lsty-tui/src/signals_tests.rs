use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::low_level::raise;

use crate::signals::Stop;

#[test]
fn a_stop_signal_is_noticed_and_nothing_else_is() {
    for signal in [SIGHUP, SIGTERM] {
        let stop = Stop::new().unwrap_or_else(|e| panic!("{e}"));
        assert!(!stop.asked());
        raise(signal).unwrap_or_else(|e| panic!("{e}"));
        assert!(stop.asked(), "signal {signal}");
    }
}
