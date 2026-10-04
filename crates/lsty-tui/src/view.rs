//! What the list shows: the show switches, the active project and
//! context, the narrowing filter and the search, all at once.

use lsty_core::list::{Item, Snapshot};
use lsty_core::query::{self, Category, Query};
use lsty_core::todotxt::Kind;

use crate::model::Model;
use crate::rows::Shows;
use crate::sidebar::{CONTEXTS, FILTERS, PROJECTS};

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
    /// Whether the archive switch or the filter lets `item` into the
    /// list at all.
    pub(crate) fn lists(&self, item: &Item) -> bool {
        self.shows.lists(item, &self.asked)
    }

    /// Whether the list shows `item` of `snap`.
    pub(crate) fn holds(&self, snap: &Snapshot, item: &Item) -> bool {
        self.shows.holds(item, &self.asked)
            && self.query.matches(snap, item, &self.ctx)
    }
}

impl Model {
    /// The scope of the view as it is now.
    pub(crate) fn scope(&self) -> Scope<'_> {
        let narrow = self.active_query();
        let mut parts = Vec::new();
        let names = [(PROJECTS, Kind::Project), (CONTEXTS, Kind::Context)];
        for (panel, kind) in names {
            if let Some(name) = &self.panels[panel].active {
                parts.push(Query::name(kind, name));
            }
        }
        let asked = narrow.overrides();
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

    /// The query of the active narrowing filter; the empty query when
    /// there is none.
    fn active_query(&self) -> Query {
        let Some(name) = &self.panels[FILTERS].active else {
            return Query::default();
        };
        let found = self.catalog.iter().find(|e| e.name.as_bytes() == name);
        found.map(|e| e.query.clone()).unwrap_or_default()
    }
}
