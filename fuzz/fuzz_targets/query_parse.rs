//! Any text either parses or is refused as an invalid `filter` value, and
//! a parsed query can be evaluated.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::query::fuzzing::query_property;

fuzz_target!(|data: &[u8]| query_property(data));
