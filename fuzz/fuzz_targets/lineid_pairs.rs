//! The pairs of a line diff are strictly increasing on both sides, pair
//! equal lines only, and are as many as a longest common subsequence.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::lineid::fuzzing::{lines, pairs_property};

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(2, |b| *b == b'|');
    let old = lines(parts.next().unwrap_or_default());
    let new = lines(parts.next().unwrap_or_default());
    pairs_property(&old, &new);
});
