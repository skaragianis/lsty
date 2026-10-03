//! Any two lists of lines map without a panic, every line of the new
//! list is the partner of at most one old line, a partner has the same
//! text, and following one line agrees with the whole mapping.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::lineid::{follow, remap};

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(3, |b| *b == b'|');
    let pick = parts.next().and_then(|p| p.first()).copied();
    let old = lines(parts.next().unwrap_or_default());
    let new = lines(parts.next().unwrap_or_default());
    let mapping = remap(&old, &new);
    let mut seen = vec![false; new.len()];
    for (i, j) in mapping.iter().enumerate() {
        let Some(j) = *j else { continue };
        assert!(!seen[j]);
        seen[j] = true;
        assert_eq!(old[i], new[j]);
    }
    let i = usize::from(pick.unwrap_or(0));
    assert_eq!(follow(&old, &new, i), mapping.get(i).copied().flatten());
});

/// One line per byte, from a small alphabet that includes a blank line,
/// so that repeats are common.
fn lines(data: &[u8]) -> Vec<&'static [u8]> {
    const ALPHABET: [&[u8]; 5] = [b"", b"a", b"b", b"c", b"d"];
    data.iter().map(|b| ALPHABET[usize::from(b % 5)]).collect()
}
