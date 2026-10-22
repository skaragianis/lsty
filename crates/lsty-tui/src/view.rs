//! What the list shows: the show switches, the active project and
//! context, the narrowing filter and the search, all at once.

use lsty_core::{
    list::{Snapshot, Task},
    query::{self, Category, Query},
    todotxt::Kind,
};

use crate::{
    connective,
    model::Model,
    rows::Shows,
    sidebar::{CONTEXTS, FILTERS, PROJECTS},
};

/// Decides which tasks the list shows. Built once for a change of the
/// view, then asked about each task.
pub(crate) struct Scope<'a> {
    query: Query,
    ctx: query::Context<'a>,
    /// The categories the narrowing filter and the search ask for.
    pub(crate) asked: Vec<Category>,
    shows: Shows,
}

impl Scope<'_> {
    /// Whether the list shows `task` of `snap`.
    pub(crate) fn holds(&self, snap: &Snapshot, task: &Task) -> bool {
        self.shows.holds(task, &self.asked)
            && self.query.matches(snap, task, &self.ctx)
    }
}

impl Model {
    /// The scope of the view as it is now.
    pub(crate) fn scope(&self) -> Scope<'_> {
        let split = self.searched();
        let names = [(PROJECTS, Kind::Project), (CONTEXTS, Kind::Context)];
        let mut parts: Vec<Query> = names
            .into_iter()
            .map(|(i, kind)| {
                connective::query(kind, split.active(&self.panels, i))
            })
            .collect();
        parts.push(self.active_query().cloned().unwrap_or_default());
        parts.extend(split.stays.into_iter().map(|(_, q)| q));
        let query = Query::and(parts);
        Scope {
            asked: query.overrides(),
            query,
            ctx: query::Context::from_clock(&self.clock),
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
