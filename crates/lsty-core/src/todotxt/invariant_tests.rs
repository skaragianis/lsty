//! The properties the fuzz targets check, run over their seed files.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::fuzzing::{append_remove_property, parse_file_property};

pub(crate) fn holds(
    target: &str,
    min: usize,
    property: fn(&[u8]),
) -> io::Result<()> {
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/seeds")
        .join(target);
    let mut files: Vec<_> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    files.sort();
    assert!(files.len() >= min, "seed files are missing");
    for file in files {
        property(&fs::read(file)?);
    }
    Ok(())
}

#[test]
fn parse_file_seeds_round_trip() -> io::Result<()> {
    holds("parse_file", 10, parse_file_property)
}

#[test]
fn append_remove_seeds_hold() -> io::Result<()> {
    holds("line_append_remove", 8, append_remove_property)
}
