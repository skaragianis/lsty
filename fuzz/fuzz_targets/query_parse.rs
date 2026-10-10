//! Fuzzes `lsty_core::query::fuzzing::query_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::query::fuzzing::query_property;

fuzz_target!(|data: &[u8]| query_property(data));
