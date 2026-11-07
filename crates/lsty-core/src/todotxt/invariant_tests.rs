//! The properties the fuzz targets check, run over their seed files.

use std::io;

use lsty_scripttest::holds;

use super::fuzzing::{append_remove_property, parse_file_property};

#[test]
fn parse_file_seeds_round_trip() -> io::Result<()> {
    holds("parse_file", 10, parse_file_property)
}

#[test]
fn append_remove_seeds_hold() -> io::Result<()> {
    holds("line_append_remove", 8, append_remove_property)
}

#[test]
fn append_remove_skips_a_token_that_splits() {
    append_remove_property(b"x\0+a@b");
    append_remove_property(b"x +c\0@a+b");
    append_remove_property(b"x +c\0+a");
}
