//! Any bytes parse, and the parsed file writes back the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::todotxt::{File, Line};

fuzz_target!(|data: &[u8]| {
    let file = File::parse(data);
    assert_eq!(file.to_bytes(), data);
    drop(Line::parse(data));
});
