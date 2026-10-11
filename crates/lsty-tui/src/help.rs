//! The help popup: the bindings in use, grouped by mode and category,
//! and the filter on them.

use lsty_core::{
    case::lower,
    keys::{Action, Category, Fed, Keymap, Mode, Outcome, about},
    theme::Slot,
};

use crate::{cmdline::OWN_HELP, field::Field, model::Model, text::Span};

const TITLE: &str = "Help";
const KEYS_JOINT: &str = " , ";
const COMMANDS: &str = "Command line";
const COUNTS: &str = "Counts";
const MOUSE: &str = "Mouse";

const COUNT_LINES: [&str; 2] = [
    "On a motion a count repeats it: 5j moves five rows, 3G goes to item 3.",
    "On an item action it takes that many items: 3x, 2dd; 2gx opens link 2.",
];

const MOUSE_LINES: [&str; 10] = [
    "click a row: move the cursor",
    "click a file: link: open it",
    "shift-click, drag: select rows",
    "double-click a row: edit it",
    "click a panel entry: filter, again to clear",
    "click ▸ ▾: fold or unfold",
    "click a panel's title or blank space: move the keyboard there",
    "click a chip: clear that filter",
    "click ▶ in doing: show what is being done",
    "wheel: scroll the pane under the pointer",
];

/// One line of the help under its headings. A line of text has no keys.
#[derive(Debug)]
struct Entry {
    heading: &'static str,
    category: &'static str,
    keys: String,
    text: &'static str,
}

impl Entry {
    fn line(&self) -> Vec<Span<'static>> {
        if self.keys.is_empty() {
            let text = format!("  {}", self.text);
            return vec![Span::new(text, Some(Slot::HintDesc))];
        }
        vec![
            Span::new("    ", None),
            Span::new(self.keys.clone(), Some(Slot::HintKey)),
            Span::new("  ", None),
            Span::new(self.text, Some(Slot::HintDesc)),
        ]
    }

    /// Whether the keys or the text hold `query`, ignoring case unless
    /// `query` has a capital.
    fn holds(&self, query: &str) -> bool {
        if query.chars().any(lsty_core::query::is_upper) {
            self.keys.contains(query) || self.text.contains(query)
        } else {
            lower(&self.keys).contains(query)
                || lower(self.text).contains(query)
        }
    }
}

/// The content of the help popup and the filter on it.
#[derive(Debug)]
pub(crate) struct Help {
    entries: Vec<Entry>,
    filter: String,
    /// The filter while it is typed.
    pub(crate) field: Option<Field>,
}

/// Heading, category, keys and text of an entry.
#[cfg(test)]
type Listed<'a> = (&'a str, &'a str, &'a str, &'a str);

impl Help {
    /// Heading, category, keys and text of every entry.
    #[cfg(test)]
    pub(crate) fn listing(&self) -> Vec<Listed<'_>> {
        self.entries
            .iter()
            .map(|e| (e.heading, e.category, e.keys.as_str(), e.text))
            .collect()
    }

    /// The help of the bindings of `keys` and of the `:` commands, with
    /// the mouse lines when `mouse` is on.
    pub(crate) fn new(keys: &Keymap, mouse: bool) -> Self {
        let mut entries = Vec::new();
        for mode in Mode::ALL {
            for category in Category::ALL {
                entries.extend(category_entries(keys, mode, category));
            }
        }
        let inline = lsty_cli::inline_commands()
            .map(|(synopsis, about)| (format!(":{synopsis}"), about));
        let own = OWN_HELP
            .iter()
            .map(|(keys, text)| ((*keys).to_owned(), *text));
        entries.extend(own.chain(inline).map(|(keys, text)| Entry {
            heading: COMMANDS,
            category: "",
            keys,
            text: text.trim_end_matches('.'),
        }));
        entries.extend(COUNT_LINES.iter().map(|text| Entry {
            heading: COUNTS,
            category: "",
            keys: String::new(),
            text,
        }));
        entries.extend(MOUSE_LINES.iter().filter(|_| mouse).map(|text| {
            Entry {
                heading: MOUSE,
                category: "",
                keys: String::new(),
                text,
            }
        }));
        Self {
            entries,
            filter: String::new(),
            field: None,
        }
    }

    /// The text the lines are filtered by.
    pub(crate) fn query(&self) -> String {
        self.field
            .as_ref()
            .map_or_else(|| self.filter.clone(), Field::text)
    }

    /// Whether the foot of the popup shows the filter.
    pub(crate) fn filtered(&self) -> bool {
        self.field.is_some() || !self.filter.is_empty()
    }

    /// The entries that hold the query, under their headings.
    pub(crate) fn lines(&self) -> Vec<Vec<Span<'static>>> {
        let query = self.query();
        let mut out = Vec::new();
        let (mut heading, mut category) = ("", "");
        for entry in self.entries.iter().filter(|e| e.holds(&query)) {
            if entry.heading != heading {
                heading = entry.heading;
                category = "";
                out.push(vec![Span::new(heading, Some(Slot::Text))]);
            }
            if entry.category != category {
                category = entry.category;
                let text = format!("  {category}");
                out.push(vec![Span::new(text, Some(Slot::Text))]);
            }
            out.push(entry.line());
        }
        out
    }
}

/// The entries of the actions of `category` that have a key in `mode`.
fn category_entries(
    keys: &Keymap,
    mode: Mode,
    category: Category,
) -> Vec<Entry> {
    Action::ALL
        .iter()
        .filter(|a| a.category() == category)
        .filter_map(|&action| {
            let written = keys.keys(mode, action);
            if written.is_empty() {
                return None;
            }
            Some(Entry {
                heading: mode.as_str(),
                category: category.as_str(),
                keys: written.join(KEYS_JOINT),
                text: about(mode, action),
            })
        })
        .collect()
}

impl Model {
    /// `help` and `:help TEXT`: shows the bindings in use, filtered by
    /// `filter`.
    pub(crate) fn open_help(&mut self, filter: &str) {
        self.open_output(TITLE, &[]);
        let mut help = Help::new(&self.deps.keys, self.deps.config.mouse);
        filter.clone_into(&mut help.filter);
        if let Some(out) = &mut self.output {
            out.lines = help.lines();
            out.help = Some(help);
        }
    }

    /// Whether the filter of the help is being typed.
    pub(crate) fn typing_help(&self) -> bool {
        self.output
            .as_ref()
            .and_then(|o| o.help.as_ref())
            .is_some_and(|h| h.field.is_some())
    }

    /// `search` in the popup: starts typing the filter, if it is the
    /// help.
    pub(crate) fn help_search(&mut self) {
        let Some(help) = self.output.as_mut().and_then(|o| o.help.as_mut())
        else {
            return;
        };
        help.field = Some(Field::new(&help.filter));
        if let Some(matcher) = self.matchers.get_mut(&Mode::Command) {
            matcher.reset();
        }
    }

    /// The keys of the command table while the filter is typed.
    pub(crate) fn help_keys(&mut self, fed: &Fed) {
        if let Some(field) = self
            .output
            .as_mut()
            .and_then(|o| o.help.as_mut())
            .and_then(|h| h.field.as_mut())
        {
            for stroke in &fed.text {
                field.typed(*stroke);
            }
        }
        if let Outcome::Matched { action, .. } = fed.outcome {
            self.help_edit(action);
        }
        self.refilter();
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the filter takes only the editing actions"
    )]
    fn help_edit(&mut self, action: Action) {
        let Some(help) = self.output.as_mut().and_then(|o| o.help.as_mut())
        else {
            return;
        };
        match action {
            Action::Execute => {
                if let Some(field) = help.field.take() {
                    help.filter = field.text();
                }
            }
            Action::Cancel => help.field = None,
            _ => {
                if let Some(field) = &mut help.field {
                    field.act(action);
                }
            }
        }
    }
}
