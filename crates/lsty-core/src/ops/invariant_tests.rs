//! The properties the fuzz targets check, run over their seed files.

use std::io;

use super::fuzzing::merge_property;
use crate::{
    errs::fuzzing::quote_property, query::fuzzing::query_property,
    todotxt::invariant_tests::holds,
};

#[test]
fn merge_seeds_hold() -> io::Result<()> {
    holds("merge", 17, merge_property)
}

#[test]
fn quote_seeds_round_trip() -> io::Result<()> {
    holds("quote_bytes", 3, quote_property)
}

#[test]
fn query_seeds_hold() -> io::Result<()> {
    holds("query_parse", 10, query_property)
}
