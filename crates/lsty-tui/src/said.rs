//! The message in the status line: its text, its tone, and the count that
//! lets a timer clear only the message it was set for.

use crate::messages::Tone;

/// The status message. Every write that puts up text that differs from what
/// was there counts it, so that the model starts a life timer without
/// comparing before and after.
#[derive(Default)]
pub(crate) struct Said {
    text: Option<String>,
    /// How the message is drawn.
    pub(crate) tone: Tone,
    count: u64,
}

impl Said {
    pub(crate) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The count of messages put up and cleared.
    pub(crate) const fn count(&self) -> u64 {
        self.count
    }

    /// Whether there is a message that was put up since the count was
    /// `count`.
    pub(crate) const fn is_new(&self, count: u64) -> bool {
        self.text.is_some() && self.count != count
    }

    /// Replaces the text.
    pub(crate) fn put(&mut self, text: Option<String>) {
        if text.is_some() && text != self.text {
            self.count += 1;
        }
        self.text = text;
    }

    pub(crate) fn say(&mut self, text: String) {
        self.put(Some(text));
    }

    /// Changes the text in place, if there is any.
    pub(crate) fn edit(&mut self, change: impl FnOnce(&str) -> String) {
        if let Some(new) = self.text().map(change) {
            self.say(new);
        }
    }

    /// Clears the message and its tone.
    pub(crate) fn clear(&mut self) {
        self.count += 1;
        self.text = None;
        self.tone = Tone::Notice;
    }
}
