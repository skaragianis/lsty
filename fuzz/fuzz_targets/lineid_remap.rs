//! Fuzzes `lsty_core::lineid::fuzzing::remap_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lsty_core::lineid::fuzzing::remap_bytes(data);
});
