//! The input is a line, a NUL byte and a token. A removal or an append is
//! refused exactly when it changes the done mark, priority or a date, and
//! removing an appended token restores the line.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lsty_core::todotxt::fuzzing::append_remove_property(data);
});
