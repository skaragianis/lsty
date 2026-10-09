//! The properties the fuzz targets check, behind the `fuzzing` feature. The
//! tests run them over the seed files.
#![expect(clippy::panic, reason = "a property fails by panicking")]

use super::{EditError, File, Line, Priority};

/// Any bytes parse, and the parsed file writes back the same bytes.
///
/// # Panics
///
/// When the property fails.
pub fn parse_file_property(data: &[u8]) {
    assert_eq!(File::parse(data).to_bytes(), data, "{data:?}");
    drop(Line::parse(data));
}

/// Done mark, priority letter, completion and creation dates.
type Stamps<'a> = (bool, Option<u8>, Option<&'a str>, Option<&'a str>);

fn stamps(l: &Line) -> Stamps<'_> {
    (
        l.is_done(),
        l.priority().map(Priority::letter),
        l.completed(),
        l.created(),
    )
}

/// The input is a line, a NUL byte and a token. A removal or an append is
/// refused exactly when it changes a stamp, and an append is undone by
/// removing the last token.
///
/// # Panics
///
/// When the property fails.
pub fn append_remove_property(data: &[u8]) {
    let at = data.iter().position(|b| *b == 0).unwrap_or(data.len());
    let (raw, rest) = data.split_at(at);
    let token = rest.get(1..).unwrap_or_default();
    if token.is_empty() || token.iter().any(|b| matches!(b, b' ' | b'\t')) {
        return;
    }
    let line = Line::parse(raw);
    for i in 0..line.tokens().len() {
        match line.remove_token(i) {
            Ok(l) => assert_eq!(stamps(&l), stamps(&line), "{raw:?} {i}"),
            Err(EditError::Refused(l)) => {
                assert_ne!(stamps(&l), stamps(&line), "{raw:?} {i}");
            }
            Err(EditError::NoToken) => panic!("token {i} of {raw:?}"),
        }
    }
    let appended = match line.append_token(token) {
        Ok(l) => {
            assert_eq!(stamps(&l), stamps(&line), "{raw:?}");
            l
        }
        Err(EditError::Refused(l)) => {
            assert_ne!(stamps(&l), stamps(&line), "{raw:?}");
            return;
        }
        Err(EditError::NoToken) => panic!("append found no token"),
    };
    assert_eq!(appended.tokens().len(), line.tokens().len() + 1, "{raw:?}");
    let last = appended.tokens().len() - 1;
    assert_eq!(appended.remove_token(last).ok().as_ref(), Some(&line));
}
