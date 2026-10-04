//! Any three lines merge without a panic into one line. No part clashes
//! twice, and only parts that all three lines disagree on clash. A draft
//! equal to its base always merges and leaves the disk's line as it is.
//! The merged line keeps the disk's done mark and dates. A clashing key
//! keeps the draft's value, and every token the draft added is in the
//! merged line unless its key clashes or the disk's line ends in
//! whitespace other than spaces and tabs.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::ops::fuzzing::{Merged, merge};
use lsty_core::todotxt::{Kind, Line, trim};

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(3, |b| *b == b'\n');
    let base = parts.next().unwrap_or_default();
    let draft = parts.next().unwrap_or_default();
    let disk = parts.next().unwrap_or_default();
    if disk.contains(&b'\n') {
        return;
    }
    let merged = merge(base, draft, disk);
    assert!(draft != base || merged.is_some());
    let Some(Merged { line, clashes }) = merged else {
        return;
    };
    assert!(!line.contains(&b'\n'));
    for (i, part) in clashes.iter().enumerate() {
        assert!(!clashes[i + 1..].contains(part));
    }
    if draft == base {
        assert_eq!(line, disk);
    }
    if draft == base || disk == base || draft == disk {
        assert!(clashes.is_empty());
    }
    let trimmed = trim(disk) == disk;
    let (base, draft, line) =
        (Line::parse(base), Line::parse(draft), Line::parse(line));
    let disk = Line::parse(disk);
    assert_eq!(line.is_done(), disk.is_done());
    assert_eq!(line.completed(), disk.completed());
    assert_eq!(line.created(), disk.created());
    let (base, draft, line) = (tokens(&base), tokens(&draft), tokens(&line));
    let keys: Vec<&str> = clashes
        .iter()
        .copied()
        .filter(|p| !matches!(*p, "priority" | "text"))
        .collect();
    for key in &keys {
        assert_eq!(first_of(&line, key), first_of(&draft, key));
    }
    let mut left = base.clone();
    for tok in draft {
        if let Some(at) = left.iter().position(|t| *t == tok) {
            left.remove(at);
        } else if trimmed && !keys.iter().any(|k| has_key(tok, k)) {
            assert!(line.contains(&tok));
        }
    }
});

/// The projects, contexts and tags of a line, in order.
fn tokens(line: &Line) -> Vec<&[u8]> {
    line.tokens()
        .filter(|t| t.kind != Kind::Word)
        .map(|t| t.text)
        .collect()
}

fn first_of<'a>(tokens: &[&'a [u8]], key: &str) -> Option<&'a [u8]> {
    tokens.iter().copied().find(|t| has_key(t, key))
}

fn has_key(token: &[u8], key: &str) -> bool {
    token.starts_with(key.as_bytes()) && token.get(key.len()) == Some(&b':')
}
