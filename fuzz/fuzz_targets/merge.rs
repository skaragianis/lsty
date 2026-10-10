//! Fuzzes `lsty_core::ops::fuzzing::merge_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::ops::fuzzing::merge_property;

fuzz_target!(|data: &[u8]| merge_property(data));
