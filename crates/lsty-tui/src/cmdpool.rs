//! What the word under the text cursor of the `:` line can be completed
//! to.

use lsty_cli::{inline_names, inline_options};
use lsty_core::list::SortField;
use lsty_core::theme;
use lsty_core::todotxt::Kind as TokenKind;

use crate::cmdline::OWN_COMMANDS;
use crate::complete::Put;
use crate::model::Model;
use crate::setform;

impl Model {
    /// The words that `word` may become on the line whose text before
    /// it is `before`.
    pub(crate) fn command_pool(
        &mut self,
        word: &str,
        before: &str,
    ) -> Vec<String> {
        let given: Vec<&str> = before.split_whitespace().collect();
        let Some(first) = given.first() else {
            return command_names();
        };
        let command = first.to_lowercase();
        if word.starts_with("--") {
            inline_options(&command)
        } else if word.starts_with('+') {
            self.names_of(TokenKind::Project)
        } else if word.starts_with('@') {
            self.names_of(TokenKind::Context)
        } else if word.starts_with("file:") {
            self.file_pool(word)
        } else {
            self.argument_pool(&command, given.len())
        }
    }

    /// The names of the commands of the line and of the commands the
    /// CLI runs inline.
    fn argument_pool(&self, command: &str, given: usize) -> Vec<String> {
        match command {
            "theme" if given == 1 => theme::names(
                &self.deps.themes,
                &self.deps.cwd,
                self.deps.config.no_config,
            ),
            "sort" if given == 1 => {
                SortField::NAMES.iter().map(|n| (*n).to_owned()).collect()
            }
            "set" => setform::names(),
            _ => Vec::new(),
        }
    }

    /// The paths that `word`, which starts with `file:`, may become.
    fn file_pool(&mut self, word: &str) -> Vec<String> {
        self.candidates_for(word)
            .into_iter()
            .filter_map(|c| match c.put {
                Put::Text(text) => Some(text),
                Put::NeedsId(_) => None,
            })
            .collect()
    }
}

fn command_names() -> Vec<String> {
    OWN_COMMANDS
        .iter()
        .copied()
        .chain(inline_names())
        .map(str::to_owned)
        .collect()
}
