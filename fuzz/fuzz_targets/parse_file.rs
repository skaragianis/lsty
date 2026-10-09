//! Any bytes parse, and the parsed file writes back the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lsty_core::todotxt::fuzzing::parse_file_property(data);
});
