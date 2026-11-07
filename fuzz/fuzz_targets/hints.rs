//! Fuzzes `lsty_tui::fuzzing::hints_property`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lsty_tui::fuzzing::hints_property(data);
});
