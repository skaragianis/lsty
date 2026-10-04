//! The names that the Filters panel lists.

use lsty_core::config::{Config, Warning};
use lsty_core::errs::quote;
use lsty_core::query::Query;

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

/// One name of the Filters panel. A saved filter keeps its parsed query;
/// a narrowing filter has none here, its query comes from the built-ins.
pub(crate) struct Entry {
    /// What the panel lists.
    pub name: String,
    /// The saved query, parsed once.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "applied when a filter is chosen")
    )]
    pub query: Option<Query>,
}

/// The narrowing filters, then the saved ones that parse, in the order
/// of the config. A saved filter that has the name of a narrowing filter
/// takes its place. A saved filter that does not parse is left out, with
/// a warning.
pub(crate) fn catalog(config: &Config) -> (Vec<Entry>, Vec<Warning>) {
    let mut saved = Vec::new();
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
    let mut entries: Vec<Entry> = NARROWING
        .iter()
        .map(|n| Entry {
            name: (*n).to_owned(),
            query: None,
        })
        .collect();
    let kept = saved.into_iter().filter(|(n, _)| !NARROWING.contains(n));
    entries.extend(kept.map(|(name, query)| Entry {
        name: name.to_owned(),
        query: Some(query),
    }));
    (entries, warnings)
}
