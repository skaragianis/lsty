//! Any three lines merge without a panic into one line. No part clashes
//! twice, and no part clashes when two of the three lines are equal. A
//! draft equal to its base always merges and leaves the disk's line as it
//! is.
//! The merged line keeps the disk's done mark. A date is the draft's where
//! it changed it (a completion date only on a done line), else the disk's.
//! A clashing key
//! keeps the draft's value, and every project, context and tag the draft
//! added is in the merged line unless its key clashes or the disk's line
//! has white space at either end. With a backtick in the draft they may be
//! plain words there: a backtick of the disk's line may pair with it and
//! quote them.

use super::merge::merge as merge_lines;
use crate::todotxt::{Kind, Line, TokenView, trim};

/// Any three lines (base, draft and disk, split at the first two newlines)
/// merge as the module doc says.
///
/// # Panics
///
/// When the property fails.
pub fn merge_property(data: &[u8]) {
    let mut parts = data.splitn(3, |b| *b == b'\n');
    let base = parts.next().unwrap_or_default();
    let draft = parts.next().unwrap_or_default();
    let disk = parts.next().unwrap_or_default();
    if disk.contains(&b'\n') {
        return;
    }
    let merged = merge_lines(base, draft, disk).ok().map(|m| {
        let clashes: Vec<&str> =
            m.clashes.into_iter().map(|c| c.part.name()).collect();
        (m.line, clashes)
    });
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
        assert!(clashes.is_empty(), "{clashes:?}");
    }
    parts_hold(&[base, draft, disk], &line, &clashes);
}

/// The dates of the merged `line` are, in order, the draft's where it
/// changed them, else the disk's; one alone on a done line reads as the
/// completion date.
fn dates_hold(base: &Line, draft: &Line, disk: &Line, line: &Line) {
    let pick = |was, mine, theirs| if mine == was { theirs } else { mine };
    let mine = draft.completed().filter(|_| disk.is_done());
    let kept = draft
        .completed()
        .filter(|c| !disk.is_done() && Some(*c) == base.created());
    let want = [
        pick(base.completed(), mine, disk.completed()),
        pick(base.created(), draft.created().or(kept), disk.created()),
    ];
    let got = [line.completed(), line.created()];
    assert!(got.iter().flatten().eq(want.iter().flatten()));
}

/// The merged `line` of `[base, draft, disk]` has the draft's priority and
/// dates where it changed them, the draft's added tokens, and the draft's
/// value for a clashing key.
fn parts_hold(lines: &[&[u8]; 3], line: &[u8], clashes: &[&str]) {
    let [base, draft, disk] = *lines;
    let trimmed = trim(disk) == disk;
    let (base, draft, merged) =
        (Line::parse(base), Line::parse(draft), Line::parse(line));
    let disk = Line::parse(disk);
    assert_eq!(merged.is_done(), disk.is_done());
    dates_hold(&base, &draft, &disk, &merged);
    // A backtick makes a word plain but leaves where it starts and ends.
    let held = |text: &[u8]| merged.tokens().any(|t| t.text == text);
    let (base, draft, line) = (tokens(&base), tokens(&draft), tokens(&merged));
    let keys: Vec<&str> = clashes
        .iter()
        .copied()
        .filter(|p| !matches!(*p, "priority" | "text"))
        .collect();
    for key in &keys {
        let (got, want) = (first_of(&line, key), first_of(&draft, key));
        assert!(got == want || got.is_none() && want.is_some_and(held));
    }
    let mut left = base.clone();
    for tok in draft {
        if let Some(at) = left.iter().position(|t| t.text == tok.text) {
            left.remove(at);
        } else if trimmed && !keys.iter().any(|k| tok.is_tag(k.as_bytes())) {
            assert!(held(tok.text));
        }
    }
}

/// The projects, contexts and tags of a line, in order.
fn tokens(line: &Line) -> Vec<TokenView<'_>> {
    line.tokens().filter(|t| t.kind != Kind::Word).collect()
}

fn first_of<'a>(tokens: &[TokenView<'a>], key: &str) -> Option<&'a [u8]> {
    tokens
        .iter()
        .find(|t| t.is_tag(key.as_bytes()))
        .map(|t| t.text)
}
