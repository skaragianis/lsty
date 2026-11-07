//! Fuzzes `lsty_core::keys::fuzzing::keys_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::keys::fuzzing::keys_property;

fuzz_target!(|data: &[u8]| keys_property(data));
