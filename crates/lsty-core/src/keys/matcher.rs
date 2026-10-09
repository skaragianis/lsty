//! The matcher: key presses in, an action and a count out.

use std::num::NonZeroU32;

use super::actions::{Action, Kind};
use super::notation::{Key, Named, Stroke};

/// The most a count can be.
const MAX_COUNT: u32 = 9999;

/// One key sequence bound to an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    pub(super) action: Action,
    pub(super) written: String,
    pub(super) strokes: Vec<Stroke>,
}

/// What the matcher made of the keys so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The keys are the start of one or more bindings.
    Pending,
    /// The keys are no binding and start none.
    NoMatch,
    /// The keys are a binding.
    Matched {
        /// The action bound to them.
        action: Action,
        /// The count typed before them; `None` when none, or when the
        /// kind of action takes none.
        count: Option<NonZeroU32>,
    },
}

/// The result of feeding one stroke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fed {
    /// What the keys amount to.
    pub outcome: Outcome,
    /// Strokes to type as text before acting on `outcome`: in modes
    /// without counts, the keys of a failed sequence, and the new key when
    /// it matched nothing.
    pub text: Vec<Stroke>,
}

impl Fed {
    fn new(outcome: Outcome) -> Self {
        Self {
            outcome,
            text: Vec::new(),
        }
    }
}

/// Turns the strokes of one mode into actions and counts.
#[derive(Clone, Debug)]
pub struct Matcher {
    bindings: Vec<Binding>,
    counts: bool,
    pending: Vec<Stroke>,
    count: u32,
    /// The last key matched nothing: digits typed right after it are
    /// dropped, not read as a count, as for any unbound key (`z2j` moves
    /// once). Cleared by the next key and by `reset`.
    refused: bool,
}

impl Matcher {
    pub(super) fn new(bindings: Vec<Binding>, counts: bool) -> Self {
        Self {
            bindings,
            counts,
            pending: Vec::new(),
            count: 0,
            refused: false,
        }
    }

    /// Moves the pending keys of `other` into this matcher and returns
    /// those that start none of its bindings, to be typed as text.
    pub fn take(&mut self, other: &mut Self) -> Vec<Stroke> {
        let mut text = Vec::new();
        if other.pending.is_empty() {
            return text;
        }
        self.pending = std::mem::take(&mut other.pending);
        while !self.pending.is_empty() && !self.starts() {
            text.push(self.pending.remove(0));
        }
        text
    }

    /// Whether the pending keys start a longer binding.
    fn starts(&self) -> bool {
        self.bindings
            .iter()
            .any(|b| starts_with(&b.strokes, &self.pending))
    }

    /// Drops the pending keys, the count and the last refusal.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.count = 0;
        self.refused = false;
    }

    /// Adds a stroke and says whether it completes a binding.
    pub fn feed(&mut self, s: Stroke) -> Fed {
        let escape = Stroke::named(Named::Esc);
        let busy = !self.pending.is_empty() || self.count > 0;
        if self.counts && s == escape && busy {
            self.reset();
            return Fed::new(Outcome::NoMatch);
        }
        let refused = std::mem::take(&mut self.refused);
        if !refused && self.take_digit(s) {
            return Fed::new(Outcome::Pending);
        }
        self.pending.push(s);
        if !self.counts {
            return self.replay();
        }
        let mut fed = self.matching();
        if fed.outcome == Outcome::NoMatch {
            fed.text = vec![s];
            self.refused = true;
        }
        fed
    }

    /// Types the first pending key as text until what is left is a
    /// binding, the start of one, or nothing.
    fn replay(&mut self) -> Fed {
        let mut text = Vec::new();
        while !self.pending.is_empty() {
            let mut fed = self.matching();
            if fed.outcome != Outcome::NoMatch {
                fed.text = text;
                return fed;
            }
            text.push(self.pending.remove(0));
        }
        Fed {
            outcome: Outcome::NoMatch,
            text,
        }
    }

    /// Reads a digit as part of the count, if it is one here.
    fn take_digit(&mut self, s: Stroke) -> bool {
        let Key::Char(c) = s.key else {
            return false;
        };
        if !self.counts || !s.is_plain_digit() || !self.pending.is_empty() {
            return false;
        }
        if c == '0' && self.count == 0 {
            return false;
        }
        let next = self
            .count
            .saturating_mul(10)
            .saturating_add(c.to_digit(10).unwrap_or(0));
        if next <= MAX_COUNT {
            self.count = next;
        }
        true
    }

    /// Compares the pending keys with the bindings.
    fn matching(&mut self) -> Fed {
        let mut longer = false;
        for b in &self.bindings {
            if b.strokes == self.pending {
                let action = b.action;
                return Fed::new(self.matched(action));
            }
            longer = longer || starts_with(&b.strokes, &self.pending);
        }
        if longer {
            return Fed::new(Outcome::Pending);
        }
        if self.counts {
            self.reset();
        }
        Fed::new(Outcome::NoMatch)
    }

    fn matched(&mut self, action: Action) -> Outcome {
        let count = if action.kind() == Kind::Other {
            None
        } else {
            NonZeroU32::new(self.count)
        };
        self.reset();
        Outcome::Matched { action, count }
    }
}

/// Whether `strokes` is longer than `prefix` and begins with it.
pub(super) fn starts_with(strokes: &[Stroke], prefix: &[Stroke]) -> bool {
    strokes.len() > prefix.len() && strokes.starts_with(prefix)
}
