//! The names that the Filters panel lists.

use lsty_core::config::{Config, Warning};
use lsty_core::errs::quote;
use lsty_core::query::Query;

use crate::focus::Focus;
use crate::model::Model;
use crate::sidebar::FILTERS;

const CONFIG: &str = "config.toml";

/// The narrowing filters that are always there, in order.
pub(crate) const NARROWING: [&str; 6] = [
    "Today",
    "Overdue",
    "Upcoming",
    "Doing",
    "Waiting",
    "No project",
];

/// One name of the Filters panel and the query that it stands for.
pub(crate) struct Entry {
    /// What the panel lists.
    pub(crate) name: String,
    /// The query, parsed once.
    pub(crate) query: Query,
}

/// The narrowing filters, then the saved ones that parse, in the order
/// of the config. A saved filter that has the name of a narrowing filter
/// takes its place. A saved filter that does not parse is left out, with
/// a warning.
pub(crate) fn catalog(config: &Config) -> (Vec<Entry>, Vec<Warning>) {
    let mut saved: Vec<(&str, Query)> = Vec::new();
    let mut warnings = Vec::new();
    for filter in &config.filters {
        match Query::parse(&filter.query) {
            Ok(query) => saved.push((filter.name.as_str(), query)),
            Err(e) => warnings.push(Warning {
                source: CONFIG.to_owned(),
                message: format!(
                    "filters: {}: {}",
                    quote(&filter.name),
                    e.message()
                ),
            }),
        }
    }
    let mut entries = Vec::new();
    for name in NARROWING {
        let own = saved.iter().rev().find(|(n, _)| *n == name);
        let query = match own {
            Some((_, query)) => Some(query.clone()),
            None => Query::builtin(name, config.upcoming_days),
        };
        entries.push(Entry {
            name: name.to_owned(),
            query: query.unwrap_or_default(),
        });
    }
    let kept = saved.into_iter().filter(|(n, _)| !NARROWING.contains(n));
    entries.extend(kept.map(|(name, query)| Entry {
        name: name.to_owned(),
        query,
    }));
    (entries, warnings)
}

impl Model {
    /// Makes the filter that `start_filter` names the active one.
    /// A name that no filter has is a warning.
    pub(crate) fn start_filter(&mut self) -> Option<Warning> {
        let name = self.deps.config.start_filter.clone()?;
        if !self.catalog.iter().any(|e| e.name == name) {
            return Some(Warning {
                source: CONFIG.to_owned(),
                message: format!("start_filter: no filter {}", quote(&name)),
            });
        }
        self.panels[FILTERS].active = Some(name.into_bytes());
        None
    }

    /// Acts on the entry under the cursor of the focused panel: a
    /// switch changes, a name becomes the active one, or stops being it.
    /// Returns whether the entry was a switch.
    fn act_on_entry(&mut self) -> Option<bool> {
        let i = self.focus.panel()?;
        self.act_on_panel(i)
    }

    /// Acts on the entry under the cursor of panel `i`, as
    /// `act_on_entry` does.
    pub(crate) fn act_on_panel(&mut self, i: usize) -> Option<bool> {
        let panel = self.panels.get_mut(i)?;
        let entry = panel.entries.get(panel.cursor)?;
        if entry.is_rule() {
            return None;
        }
        let toggle = entry.toggles();
        if let Some(what) = toggle {
            self.shows.flip(what);
        } else {
            let name = entry.name().to_vec();
            let same = panel.active.as_deref() == Some(name.as_slice());
            panel.active = (!same).then_some(name);
        }
        self.refresh();
        Some(toggle.is_some())
    }

    /// `toggle_entry`: the focus stays where it is.
    pub(crate) fn toggle_entry(&mut self) {
        self.act_on_entry();
    }

    /// `select_and_jump`: like `toggle_entry`, then the focus goes to the
    /// list, unless the entry was a switch.
    pub(crate) fn select_and_jump(&mut self) {
        if self.act_on_entry() == Some(false) {
            self.set_focus(Focus::List);
        }
    }
}

impl Model {
    /// Makes `name` the active entry of panel `i`, or none.
    pub(crate) fn pick(&mut self, i: usize, name: Option<Vec<u8>>) {
        if let Some(panel) = self.panels.get_mut(i) {
            panel.active = name;
        }
        self.refresh();
    }
}
