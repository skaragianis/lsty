//! What the list shows: the show switches, the active project and
//! context, the narrowing filter and the search, all at once.

use lsty_core::{
    list::{Item, Snapshot},
    query::{self, Category, Query},
    todotxt::Kind,
};

use crate::{
    connective,
    model::Model,
    rows::Shows,
    sidebar::{CONTEXTS, FILTERS, PROJECTS},
};

/// Decides which items the list shows. Built once for a change of the
/// view, then asked about each item.
pub(crate) struct Scope<'a> {
    query: Query,
    ctx: query::Context<'a>,
    /// The categories the narrowing filter asks for.
    asked: Vec<Category>,
    shows: Shows,
}

impl Scope<'_> {
    /// Whether the list shows `item` of `snap`.
    pub(crate) fn holds(&self, snap: &Snapshot, item: &Item) -> bool {
        self.shows.holds(item, &self.asked)
            && self.query.matches(snap, item, &self.ctx)
    }
}

impl Model {
    /// The scope of the view as it is now.
    pub(crate) fn scope(&self) -> Scope<'_> {
        let narrow = self.active_query().cloned().unwrap_or_default();
        let asked = narrow.overrides();
        let names = [(PROJECTS, Kind::Project), (CONTEXTS, Kind::Context)];
        let mut parts: Vec<Query> = names
            .into_iter()
            .map(|(i, kind)| connective::query(kind, &self.panels[i].active))
            .collect();
        parts.push(narrow);
        let text = self.search_text();
        if !text.is_empty() {
            parts.push(Query::word(&text));
        }
        Scope {
            query: Query::and(parts),
            ctx: query::Context::from_clock(&self.clock),
            asked,
            shows: self.shows,
        }
    }

    /// The query of the active narrowing filter, when there is one.
    pub(crate) fn active_query(&self) -> Option<&Query> {
        let name = &self.panels[FILTERS].active.first()?.name;
        let found = self.catalog.iter().find(|e| e.name.as_bytes() == name);
        found.map(|e| &e.query)
    }
}
