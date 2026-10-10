//! The text line of the status line: the search, whose list follows the
//! text as it changes, and the prompts that share its keys.

use lsty_core::keys::{Action, Fed, Outcome};

use crate::{
    line::{Line, LineKind},
    model::Model,
    msg::Cmd,
    text::width_of,
};

impl Model {
    /// `search`: opens the prompt with the kept search filled in.
    pub(crate) fn open_search(&mut self) {
        self.line = Some(Line::new(LineKind::Search, &self.search));
        self.session.prefill(!self.search.is_empty());
    }

    /// Closes the text line, whichever it is.
    pub(crate) fn close_input(&mut self) {
        if self.choosing.is_none() {
            self.end_preview();
        }
        self.line = None;
        self.session.typed();
        self.reset_keys();
    }

    /// The search that the list follows: the text being typed, or the
    /// one that was kept.
    pub(crate) fn search_text(&self) -> String {
        match &self.line {
            Some(Line {
                kind: LineKind::Search,
                field,
                ..
            }) => field.text(),
            _ => self.search.clone(),
        }
    }

    /// Takes the keys that the command table made of a stroke while the
    /// text line is open, and narrows the list to the new text when it
    /// is the search.
    pub(crate) fn prompt_keys(&mut self, fed: Fed) -> Vec<Cmd> {
        let searching = self
            .line
            .as_ref()
            .is_some_and(|l| matches!(l.kind, LineKind::Search));
        let action = self.prompt_action_of(fed.outcome);
        let completing = matches!(
            action,
            Some(Action::CompleteNext | Action::CompletePrev)
        );
        if !completing && let Some(line) = &mut self.line {
            line.comp = None;
            self.preview_completed();
        }
        if !fed.text.is_empty() {
            self.session.typed();
        }
        if let Some(line) = &mut self.line {
            for stroke in fed.text {
                line.field.typed(stroke);
            }
        }
        let mut cmds = Vec::new();
        if let Some(action) = action {
            cmds = self.prompt_action(action);
        }
        if searching {
            self.reshow();
        }
        cmds
    }

    /// The action a key has on the text line; while candidates are
    /// shown, the arrows move through them.
    fn prompt_action_of(&self, outcome: Outcome) -> Option<Action> {
        let Outcome::Matched { action, .. } = outcome else {
            return None;
        };
        let Some(comp) = self.line.as_ref().and_then(|l| l.comp.as_ref())
        else {
            return Some(action);
        };
        let sideways =
            matches!(action, Action::CursorLeft | Action::CursorRight);
        Some(if sideways && !comp.stepped() {
            action
        } else {
            menu(action)
        })
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
            Action::Execute
                if self
                    .line
                    .as_ref()
                    .is_some_and(|l| matches!(l.kind, LineKind::Items(_))) =>
            {
                self.execute_prompt();
            }
            Action::Execute => {
                let kept = self.search_text();
                self.remember_search(&kept);
                self.search = kept;
                self.close_input();
            }
            Action::Cancel => self.close_input(),
            Action::CompleteNext | Action::CompletePrev => {
                self.complete(action);
            }
            _ => {
                if let Some(line) = &mut self.line {
                    line.field.act(action);
                }
            }
        }
        Vec::new()
    }

    /// Where the text cursor is on the screen, when the field of
    /// INSERT mode or the prompt is open.
    pub(crate) fn cursor_at(&self) -> Option<(u16, u16)> {
        if self.edit.is_some() {
            return self.field_cursor();
        }
        if self.typing_help() {
            return self.help_cursor();
        }
        if self.typing_theme() {
            return self.theme_cursor();
        }
        let line = self.line.as_ref()?;
        let mode = width_of(self.mode().as_str()) + 2;
        let x = mode + 1 + width_of(line.kind.label()) + line.field.cells();
        let y = self.height.checked_sub(1)?;
        Some((u16::try_from(x).ok()?.min(self.width.saturating_sub(1)), y))
    }
}

/// What a key does while candidates are shown: the arrows move through
/// them (`Left` and `Right` only once a candidate is in the text).
#[expect(clippy::wildcard_enum_match_arm, reason = "only the arrows change")]
const fn menu(action: Action) -> Action {
    match action {
        Action::CursorRight | Action::HistoryNext => Action::CompleteNext,
        Action::CursorLeft | Action::HistoryPrev => Action::CompletePrev,
        other => other,
    }
}
