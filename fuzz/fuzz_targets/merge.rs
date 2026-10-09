//! Any three lines merge without a panic into one line. No part clashes
//! twice, and no part clashes when two of the three lines are equal. A
//! draft equal to its base always merges and leaves the disk's line as it
//! is.
//! The merged line keeps the disk's done mark and dates. A clashing key
//! keeps the draft's value, and every token the draft added is in the
//! merged line unless its key clashes or the disk's line has white space
//! at either end.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::ops::fuzzing::merge;
use lsty_core::todotxt::{Kind, Line, TokenView, trim};

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
    let Some((line, clashes)) = merged else {
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
        if let Some(at) = left.iter().position(|t| t.text == tok.text) {
            left.remove(at);
        } else if trimmed && !keys.iter().any(|k| tok.is_tag(k.as_bytes())) {
            assert!(line.iter().any(|t| t.text == tok.text));
        }
    }
});

/// The projects, contexts and tags of a line, in order.
fn tokens<'a>(line: &'a Line) -> Vec<TokenView<'a>> {
    line.tokens().filter(|t| t.kind != Kind::Word).collect()
}

fn first_of<'a>(tokens: &[TokenView<'a>], key: &str) -> Option<&'a [u8]> {
    tokens
        .iter()
        .find(|t| t.is_tag(key.as_bytes()))
        .map(|t| t.text)
}
