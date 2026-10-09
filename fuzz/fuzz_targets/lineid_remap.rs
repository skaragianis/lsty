//! Any two lists of lines map without a panic, every line of the new
//! list is the partner of at most one old line, a partner has the same
//! text, and following one line agrees with the whole mapping.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::lineid::fuzzing::{lines, remap_property};

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(3, |b| *b == b'|');
    let pick = parts.next().and_then(|p| p.first()).copied();
    let old = lines(parts.next().unwrap_or_default());
    let new = lines(parts.next().unwrap_or_default());
    remap_property(&old, &new, usize::from(pick.unwrap_or(0)));
});
