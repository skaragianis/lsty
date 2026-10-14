//! The names that the Filters panel lists.

use std::time::Duration;

use lsty_core::{
    config::{Config, Warning},
    errs::quote,
    query::Query,
};

use crate::{
    connective::{self, Term},
    focus::Focus,
    model::Model,
    msg::{Cmd, Msg},
    sidebar::FILTERS,
};

/// How long after the last panel step the list is filtered again, so that
/// stepping through the states shows none of the views in between.
const REFILTER: Duration = Duration::from_millis(50);

/// The panel steps that the list has not caught up with.
#[derive(Debug, Default)]
pub(crate) struct Refilter {
    /// Counts the steps, so that only the timer of the last one acts.
    pub(crate) serial: u64,
    pub(crate) owed: bool,
}

impl Refilter {
    /// The timer for the steps taken since the count was `step`.
    pub(crate) fn timer(&self, step: u64) -> Option<Cmd> {
        (self.serial != step).then_some(Cmd::After {
            delay: REFILTER,
            msg: Msg::Refilter(self.serial),
        })
    }
}

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
            Err(e) => warnings.push(Warning::config(format!(
                "filters: {}: {}",
                quote(&filter.name),
                e.message()
            ))),
        }
    }
    let builtins = Query::builtins(config.upcoming_days);
    let mut entries: Vec<Entry> = builtins
        .iter()
        .map(|(name, query)| {
            let own = saved.iter().rev().find(|(n, _)| n == name);
            Entry {
                name: (*name).to_owned(),
                query: own.map_or(query, |(_, own)| own).clone(),
            }
        })
        .collect();
    let kept = saved
        .into_iter()
        .filter(|(n, _)| !builtins.iter().any(|(b, _)| b == n));
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
            return Some(Warning::config(format!(
                "start_filter: no filter {}",
                quote(&name)
            )));
        }
        self.panels[FILTERS].active = vec![Term::and(name)];
        None
    }

    /// Acts on the entry under the cursor of the focused panel: a
    /// switch changes, a name becomes active or stops being it (a filter
    /// replaces the active one).
    pub(crate) fn act_on_entry(&mut self) {
        if let Some(i) = self.focus.panel() {
            self.act_on_panel(i);
        }
    }

    /// Acts on the entry under the cursor of panel `i`, as
    /// `act_on_entry` does.
    pub(crate) fn act_on_panel(&mut self, i: usize) {
        let Some(panel) = self.panels.get_mut(i) else {
            return;
        };
        let Some(entry) = panel.entries.get(panel.cursor) else {
            return;
        };
        if entry.is_rule() {
            return;
        }
        if let Some(what) = entry.toggles() {
            self.shows.flip(what);
        } else {
            let pick = if i == FILTERS {
                connective::pick
            } else {
                connective::step
            };
            pick(&mut panel.active, entry.name());
        }
        self.rebuild_panels();
        self.scroll_panels();
        self.refilter.owed = true;
        self.refilter.serial += 1;
    }

    /// `clear_filters`: deselects every selected label and the selected
    /// filter; the search and the switches stay.
    pub(crate) fn clear_filters(&mut self) {
        let was = self.panels.iter().any(|p| !p.active.is_empty());
        for panel in &mut self.panels {
            panel.active.clear();
        }
        if was {
            self.refresh();
        }
    }

    /// Lists the rows again for the panel steps that wait for it.
    pub(crate) fn flush_refilter(&mut self) {
        if self.refilter.owed {
            self.reshow();
        }
    }

    /// `select_and_jump`: the focus goes to the list; nothing changes.
    pub(crate) fn select_and_jump(&mut self) {
        self.set_focus(Focus::List);
    }
}
