//! What the word under the text cursor of the `:` line can be completed
//! to.

use lsty_cli::{inline_names, inline_options};
use lsty_core::{case::lower, list::SortField, theme};

use crate::{
    cmdline::{full_name, own_names},
    complete::Put,
    model::Model,
    setform,
    words::split_words,
};

impl Model {
    /// The words that `word` may become on the line whose text before
    /// it is `before`.
    pub(crate) fn command_pool(
        &mut self,
        word: &str,
        before: &str,
    ) -> Vec<String> {
        let Some(given) = split_words(before) else {
            return Vec::new();
        };
        let Some(first) = given.first() else {
            return command_names();
        };
        let Ok(command) = full_name(first) else {
            return Vec::new();
        };
        let command = lower(&command);
        if word.starts_with("--") {
            inline_options(&command)
        } else if word.starts_with(['+', '@']) {
            self.name_pool(word)
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
    own_names()
        .chain(inline_names())
        .map(str::to_owned)
        .collect()
}
