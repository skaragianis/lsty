//! The input is a line, a NUL byte and a token. A removal or an append is
//! refused exactly when it changes the done mark, priority or a date, and
//! removing an appended token restores the line.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::todotxt::{EditError, Line};

fn stamps(l: &Line) -> String {
    let pri = l.priority().map(|p| p.letter());
    format!(
        "{} {pri:?} {:?} {:?}",
        l.is_done(),
        l.completed(),
        l.created()
    )
}

fuzz_target!(|data: &[u8]| {
    let at = data.iter().position(|b| *b == 0).unwrap_or(data.len());
    let (raw, rest) = data.split_at(at);
    let token = rest.get(1..).unwrap_or_default();
    if token.is_empty() || token.iter().any(|b| matches!(b, b' ' | b'\t')) {
        return;
    }

    let line = Line::parse(raw);
    for i in 0..line.token_count() {
        match line.remove_token(i) {
            Ok(l) => assert_eq!(stamps(&l), stamps(&line)),
            Err(EditError::Refused(l)) => {
                assert_ne!(stamps(&l), stamps(&line));
            }
            Err(EditError::NoToken) => panic!("token {i} is missing"),
        }
    }

    let appended = match line.append_token(token) {
        Ok(l) => {
            assert_eq!(stamps(&l), stamps(&line));
            l
        }
        Err(EditError::Refused(l)) => {
            assert_ne!(stamps(&l), stamps(&line));
            return;
        }
        Err(EditError::NoToken) => panic!("append found no token"),
    };
    assert_eq!(appended.token_count(), line.token_count() + 1);
    let last = appended.token_count() - 1;
    assert_eq!(appended.remove_token(last).ok().as_ref(), Some(&line));
});
