//! Fuzzes `lsty_core::errs::fuzzing::quote_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::errs::fuzzing::quote_property;

fuzz_target!(|data: &[u8]| quote_property(data));
