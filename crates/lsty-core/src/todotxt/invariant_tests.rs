//! The properties the fuzz targets check, run over their seed files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::fuzzing::{append_remove_property, parse_file_property};

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

#[test]
fn parse_file_seeds_round_trip() -> io::Result<()> {
    let seeds = seeds("parse_file")?;
    assert!(seeds.len() >= 10, "seed files are missing");
    for data in seeds {
        parse_file_property(&data);
    }
    Ok(())
}

#[test]
fn append_remove_seeds_hold() -> io::Result<()> {
    let seeds = seeds("line_append_remove")?;
    assert!(seeds.len() >= 8, "seed files are missing");
    for data in seeds {
        append_remove_property(&data);
    }
    Ok(())
}
