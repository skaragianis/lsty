//! The text line of the status line: the search, whose list follows the
//! text as it changes, and the prompts that share its keys.

use lsty_core::keys::{Action, Fed, Mode, Outcome};

use crate::field::Field;
use crate::model::Model;
use crate::msg::Cmd;
use crate::prompt::SEARCH_LABEL;

impl Model {
    /// `search`: opens the prompt with the kept search filled in.
    pub(crate) fn open_search(&mut self) {
        self.input = Some(Field::new(&self.search));
        self.session.prefill(!self.search.is_empty());
        self.mode = Mode::Command;
    }

    /// Closes the text line, whichever it is.
    pub(crate) fn close_input(&mut self) {
        self.input = None;
        self.prompt = None;
        self.session.typed();
        self.settle_mode();
        if let Some(matcher) = self.matchers.get_mut(&Mode::Command) {
            matcher.reset();
        }
    }

    /// The search that the list follows: the text being typed, or the
    /// one that was kept.
    pub(crate) fn search_text(&self) -> String {
        match (&self.input, &self.prompt) {
            (Some(field), None) => field.text(),
            _ => self.search.clone(),
        }
    }

    /// Takes the keys that the command table made of a stroke while the
    /// text line is open, and narrows the list to the new text when it
    /// is the search.
    pub(crate) fn prompt_keys(&mut self, fed: Fed) -> Vec<Cmd> {
        let searching = self.prompt.is_none();
        let completing = matches!(
            fed.outcome,
            Outcome::Matched {
                action: Action::CompleteNext | Action::CompletePrev,
                ..
            }
        );
        if !completing && let Some(prompt) = &mut self.prompt {
            prompt.drop_completion();
        }
        if !fed.text.is_empty() {
            self.session.typed();
        }
        if let Some(field) = &mut self.input {
            for stroke in fed.text {
                field.typed(stroke);
            }
        }
        let mut cmds = Vec::new();
        if let Outcome::Matched { action, .. } = fed.outcome {
            cmds = self.prompt_action(action);
        }
        if searching {
            self.reshow();
        }
        cmds
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the prompt takes only the editing actions"
    )]
    fn prompt_action(&mut self, action: Action) -> Vec<Cmd> {
        if !matches!(action, Action::HistoryPrev | Action::HistoryNext) {
            self.session.typed();
        }
        match action {
            Action::HistoryPrev | Action::HistoryNext => {
                self.recall_step(action);
            }
            Action::Execute if self.on_command_line() => {
                return self.execute_line();
            }
            Action::Execute if self.prompt.is_some() => self.execute_prompt(),
            Action::Execute => {
                self.search = self.search_text();
                let kept = self.search.clone();
                self.remember_search(&kept);
                self.close_input();
            }
            Action::Cancel => self.close_input(),
            Action::CompleteNext | Action::CompletePrev => {
                self.complete(action);
            }
            _ => {
                if let Some(field) = &mut self.input {
                    field.act(action);
                }
            }
        }
        Vec::new()
    }

    /// Where the text cursor is on the screen, when the field of
    /// INSERT mode or the prompt is open.
    pub fn cursor_at(&self) -> Option<(u16, u16)> {
        if self.edit.is_some() {
            return self.field_cursor();
        }
        let field = self.input.as_ref()?;
        let mode = self.mode.as_str().len() + 2;
        let label = if self.prompt.is_some() {
            self.prompt_label()
        } else {
            SEARCH_LABEL
        };
        let x = mode + 1 + label.chars().count() + field.cells();
        let y = self.height.checked_sub(1)?;
        Some((u16::try_from(x).ok()?.min(self.width.saturating_sub(1)), y))
    }
}
