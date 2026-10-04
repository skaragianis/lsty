//! The search prompt: `/` opens it in the status line, the list
//! follows the text as it changes.

use lsty_core::keys::{Action, Fed, Mode, Outcome};

use crate::field::Field;
use crate::model::Model;

impl Model {
    /// `search`: opens the prompt with the kept search filled in.
    pub(crate) fn open_search(&mut self) {
        self.input = Some(Field::new(&self.search));
        self.mode = Mode::Command;
    }

    fn close_search(&mut self) {
        self.input = None;
        self.mode = Mode::Normal;
        for mode in [Mode::Normal, Mode::Visual, Mode::Insert, Mode::Command] {
            if let Some(matcher) = self.matchers.get_mut(&mode) {
                matcher.reset();
            }
        }
    }

    /// The search that the list follows: the text being typed, or the
    /// one that was kept.
    pub(crate) fn search_text(&self) -> String {
        self.input
            .as_ref()
            .map_or_else(|| self.search.clone(), Field::text)
    }

    /// Takes the keys that the command table made of a stroke while the
    /// prompt is open, and narrows the list to the new text.
    pub(crate) fn prompt_keys(&mut self, fed: Fed) {
        if let Some(field) = &mut self.input {
            for stroke in fed.text {
                field.typed(stroke);
            }
        }
        if let Outcome::Matched { action, .. } = fed.outcome {
            self.prompt_action(action);
        }
        self.reshow();
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the prompt takes only the editing actions"
    )]
    fn prompt_action(&mut self, action: Action) {
        match action {
            Action::Execute => {
                self.search = self.search_text();
                self.close_search();
            }
            Action::Cancel => self.close_search(),
            _ => {
                if let Some(field) = &mut self.input {
                    field.act(action);
                }
            }
        }
    }

    /// Where the text cursor of the prompt is on the screen, when the
    /// prompt is open.
    pub fn cursor_at(&self) -> Option<(u16, u16)> {
        let field = self.input.as_ref()?;
        let mode = self.mode.as_str().len() + 2;
        let x = mode + 1 + "/".len() + field.cells();
        let y = self.height.checked_sub(1)?;
        Some((u16::try_from(x).ok()?.min(self.width.saturating_sub(1)), y))
    }
}
