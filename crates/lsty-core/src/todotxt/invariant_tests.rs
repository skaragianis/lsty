//! The properties the fuzz targets check, run over their seed files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{EditError, File, Line};

fn seeds(target: &str) -> io::Result<Vec<Vec<u8>>> {
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/seeds")
        .join(target);
    let mut files: Vec<_> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    files.sort();
    files.iter().map(fs::read).collect()
}

fn parse_file_property(data: &[u8]) {
    assert_eq!(File::parse(data).to_bytes(), data, "{data:?}");
    drop(Line::parse(data));
}

#[test]
fn parse_file_seeds_round_trip() -> io::Result<()> {
    let seeds = seeds("parse_file")?;
    assert!(seeds.len() >= 10, "seed files are missing");
    for data in seeds {
        parse_file_property(&data);
    }
    Ok(())
}

/// Done mark, priority letter, completion and creation dates.
type Stamps = (bool, Option<u8>, Option<String>, Option<String>);

fn stamps(l: &Line) -> Stamps {
    (
        l.is_done(),
        l.priority().map(super::Priority::letter),
        l.completed().map(str::to_owned),
        l.created().map(str::to_owned),
    )
}

/// A removal or an append is refused exactly when it changes a stamp, and
/// an append is undone by removing the last token.
fn append_remove_property(raw: &[u8], token: &[u8]) {
    if token.is_empty() || token.iter().any(|b| matches!(b, b' ' | b'\t')) {
        return;
    }
    let line = Line::parse(raw);
    for i in 0..line.token_count() {
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
    assert_eq!(appended.token_count(), line.token_count() + 1, "{raw:?}");
    let last = appended.token_count() - 1;
    assert_eq!(appended.remove_token(last).ok().as_ref(), Some(&line));
}

#[test]
fn append_remove_seeds_hold() -> io::Result<()> {
    let seeds = seeds("line_append_remove")?;
    assert!(seeds.len() >= 8, "seed files are missing");
    for data in seeds {
        let at = data.iter().position(|b| *b == 0).unwrap_or(data.len());
        let (raw, token) = data.split_at(at);
        append_remove_property(raw, token.get(1..).unwrap_or_default());
    }
    Ok(())
}
